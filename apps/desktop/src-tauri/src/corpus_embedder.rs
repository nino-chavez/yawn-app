//! The transport half of the embedding seam: `corpus.embed` over the worker port.
//!
//! `session-core::corpus_embedding` decides which windows to send and what a
//! reply has to contain. This decides nothing — it moves text across the process
//! boundary, decodes what comes back, and turns any failure into one refusal.
//!
//! # Why the timeout is generous
//!
//! `ProcessWorkerPort::request` drops the worker from application state on **any**
//! error, so every later caller — transcription included — gets "worker is
//! unavailable" until the app restarts. A timeout that mistook a slow batch for a
//! dead process would take capture down to save a second.
//!
//! Measured on the packaged runtime, 2026-08-08: 39 ms to load the model, 210 ms
//! for a first 24-window batch, 32 ms warm. [`EMBED_TIMEOUT`] is two orders above
//! that, which is what a timeout for "the process died" should look like.
//!
//! # Why nothing is cached
//!
//! `corpus_embed` reloads the model per request, against its own docstring's
//! advice. The advice is right and the measurement says it is worth 1.3 seconds
//! across an entire 800-window corpus — 2.4 s reloading versus 1.1 s held open —
//! because MLX maps the weights lazily rather than reading 90 MB. A cache would
//! be complexity bought with a number nobody would notice.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use local_meeting_notes_session_core::corpus_embedding::{
    EmbedderUnavailable, WINDOWS_PER_REQUEST, WindowEmbedder,
};
use local_meeting_notes_session_core::corpus_index::PendingWindow;
use local_meeting_notes_session_core::protocol::Operation;
use serde_json::json;

use crate::product_coordinator::WorkerPort;

/// See the module doc: this is a liveness bound, not a performance one.
pub(crate) const EMBED_TIMEOUT: Duration = Duration::from_secs(30);

pub(crate) struct WorkerWindowEmbedder {
    port: Arc<dyn WorkerPort>,
    dimension: usize,
}

impl WorkerWindowEmbedder {
    pub(crate) fn new(port: Arc<dyn WorkerPort>, dimension: u32) -> Self {
        Self {
            port,
            dimension: dimension as usize,
        }
    }
}

impl WindowEmbedder for WorkerWindowEmbedder {
    fn embed(
        &self,
        windows: &[PendingWindow],
    ) -> Result<HashMap<String, Vec<f32>>, EmbedderUnavailable> {
        if windows.is_empty() || windows.len() > WINDOWS_PER_REQUEST {
            // The driver never does this. Refused here anyway, because the
            // worker refuses it too and a round trip to learn that is waste.
            return Err(EmbedderUnavailable(format!(
                "an embedding request must carry 1..={WINDOWS_PER_REQUEST} windows"
            )));
        }
        let arguments = json!({
            "windows": windows
                .iter()
                .map(|window| json!({
                    "text_sha256": window.text_sha256,
                    "text": window.text,
                }))
                .collect::<Vec<_>>(),
        });

        let result = self
            .port
            .request(Operation::CorpusEmbed, arguments, EMBED_TIMEOUT)
            .map_err(|error| EmbedderUnavailable(error.0))?;
        if !result.ok {
            return Err(EmbedderUnavailable("the worker refused the batch".into()));
        }

        let mut vectors = HashMap::with_capacity(result.artifact_digests.len());
        for (digest, encoded) in result.artifact_digests {
            let bytes = STANDARD
                .decode(&encoded)
                .map_err(|_| EmbedderUnavailable("a vector was not base64".into()))?;
            if bytes.len() != self.dimension * 4 {
                return Err(EmbedderUnavailable(format!(
                    "a vector was {} bytes rather than {}",
                    bytes.len(),
                    self.dimension * 4
                )));
            }
            vectors.insert(
                digest,
                bytes
                    .chunks_exact(4)
                    .map(|four| f32::from_le_bytes([four[0], four[1], four[2], four[3]]))
                    .collect(),
            );
        }
        Ok(vectors)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use local_meeting_notes_session_core::protocol::{ResultCode, ResultSchema, WorkerResult};
    use serde_json::Value;
    use uuid::Uuid;

    use super::*;
    use crate::product_coordinator::WorkerPortUnavailable;

    struct FakePort {
        reply: Mutex<Result<HashMap<String, String>, String>>,
        seen: Mutex<Vec<Value>>,
        ok: bool,
    }

    impl FakePort {
        fn answering(reply: HashMap<String, String>) -> Self {
            Self {
                reply: Mutex::new(Ok(reply)),
                seen: Mutex::new(Vec::new()),
                ok: true,
            }
        }
    }

    impl WorkerPort for FakePort {
        fn request(
            &self,
            operation: Operation,
            arguments: Value,
            _timeout: Duration,
        ) -> Result<WorkerResult, WorkerPortUnavailable> {
            assert_eq!(operation, Operation::CorpusEmbed);
            self.seen.lock().unwrap().push(arguments);
            match &*self.reply.lock().unwrap() {
                Ok(digests) => Ok(WorkerResult {
                    schema: ResultSchema::V2,
                    request_id: Uuid::new_v4(),
                    ok: self.ok,
                    code: (!self.ok).then_some(ResultCode::ProtocolFailure),
                    recoverable: None,
                    artifact_digests: digests.clone(),
                }),
                Err(reason) => Err(WorkerPortUnavailable(reason.clone())),
            }
        }
    }

    fn window(text: &str) -> PendingWindow {
        use sha2::{Digest, Sha256};
        PendingWindow {
            meeting_id: "meeting-a".into(),
            window_index: 0,
            text_sha256: format!("{:x}", Sha256::digest(text.as_bytes())),
            text: text.into(),
        }
    }

    fn encoded(values: &[f32]) -> String {
        let mut bytes = Vec::new();
        for value in values {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        STANDARD.encode(bytes)
    }

    #[test]
    fn a_reply_decodes_to_the_floats_the_worker_produced() {
        let mut values = vec![0.0_f32; 384];
        values[7] = 0.5;
        values[383] = -1.25;
        let asked = window("the lease renews in March");
        let port = Arc::new(FakePort::answering(HashMap::from([(
            asked.text_sha256.clone(),
            encoded(&values),
        )])));
        let embedder = WorkerWindowEmbedder::new(port.clone(), 384);

        let vectors = embedder
            .embed(std::slice::from_ref(&asked))
            .expect("decodes");
        assert_eq!(vectors[&asked.text_sha256], values);

        // The text crosses the boundary exactly once, beside its digest.
        let sent = port.seen.lock().unwrap();
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0]["windows"][0]["text"], "the lease renews in March");
        assert_eq!(sent[0]["windows"][0]["text_sha256"], asked.text_sha256);
    }

    #[test]
    fn a_vector_of_the_wrong_width_is_refused_at_the_boundary() {
        let asked = window("alpha");
        let port = Arc::new(FakePort::answering(HashMap::from([(
            asked.text_sha256.clone(),
            encoded(&[1.0, 0.0]),
        )])));
        let embedder = WorkerWindowEmbedder::new(port, 384);
        let error = embedder.embed(&[asked]).expect_err("refuses");
        assert!(error.0.contains("8 bytes rather than 1536"), "{}", error.0);
    }

    #[test]
    fn a_vector_that_is_not_base64_is_refused_rather_than_partly_decoded() {
        let asked = window("alpha");
        let port = Arc::new(FakePort::answering(HashMap::from([(
            asked.text_sha256.clone(),
            "not base64!!".into(),
        )])));
        let embedder = WorkerWindowEmbedder::new(port, 384);
        assert!(embedder.embed(&[asked]).is_err());
    }

    #[test]
    fn a_refused_batch_is_an_error_and_not_an_empty_answer() {
        let asked = window("alpha");
        let port = Arc::new(FakePort {
            reply: Mutex::new(Ok(HashMap::new())),
            seen: Mutex::new(Vec::new()),
            ok: false,
        });
        let embedder = WorkerWindowEmbedder::new(port, 384);
        let error = embedder.embed(&[asked]).expect_err("refuses");
        assert!(error.0.contains("refused the batch"));
    }

    /// An empty answer with `ok: true` is not an error here — the driver is what
    /// notices the missing digest, and it stops the pass rather than skipping.
    #[test]
    fn an_ok_reply_missing_a_digest_is_left_for_the_driver_to_judge() {
        let asked = window("alpha");
        let port = Arc::new(FakePort::answering(HashMap::new()));
        let embedder = WorkerWindowEmbedder::new(port, 384);
        assert!(
            embedder
                .embed(&[asked])
                .expect("no transport error")
                .is_empty()
        );
    }

    #[test]
    fn an_oversized_request_never_reaches_the_worker() {
        let port = Arc::new(FakePort::answering(HashMap::new()));
        let embedder = WorkerWindowEmbedder::new(port.clone(), 384);
        let many: Vec<_> = (0..WINDOWS_PER_REQUEST + 1)
            .map(|index| window(&format!("window {index}")))
            .collect();
        assert!(embedder.embed(&many).is_err());
        assert!(embedder.embed(&[]).is_err());
        assert!(port.seen.lock().unwrap().is_empty());
    }
}
