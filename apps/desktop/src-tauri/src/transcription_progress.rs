//! How much of a meeting's audio the Apple speech path has transcribed, held in
//! memory while the request runs.
//!
//! Progress is best-effort and it is never stored: it lives here only from the
//! worker's first measurement until the request ends, and a drop guard clears it
//! on every way out. Nothing here can fail a transcript. The protocol hands over
//! only measurements that passed admission, through a sink that cannot fail the
//! request (`OwnedChild::request_observing`), and a measurement that is missing,
//! stale or for another meeting is simply never shown.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use local_meeting_notes_session_core::protocol::{
    CaptureProgressState, Operation, ProgressEvent, ProtocolError, TranscriptionProgress,
};
use serde::Serialize;
use serde_json::json;
use uuid::Uuid;

use crate::{WorkerCallError, request_worker_on};

/// The latest `(audio_seconds_done, audio_seconds_total)` for each meeting that
/// is being transcribed. In practice that is one meeting, because one worker
/// transcribes one meeting at a time; the map keeps a late measurement for one
/// meeting from ever being shown for another.
#[derive(Default)]
pub(crate) struct TranscriptionProgressStore {
    latest: Mutex<HashMap<Uuid, (u32, u32)>>,
}

impl TranscriptionProgressStore {
    // A poisoned lock means another thread panicked mid-write of two integers.
    // The map is still usable and progress is not worth a panic of its own.
    fn entries(&self) -> MutexGuard<'_, HashMap<Uuid, (u32, u32)>> {
        self.latest.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub(crate) fn record(&self, progress: &TranscriptionProgress) {
        self.entries().insert(
            progress.meeting_id,
            (progress.audio_seconds_done, progress.audio_seconds_total),
        );
    }

    pub(crate) fn latest(&self, meeting_id: Uuid) -> Option<AudioProgress> {
        self.entries()
            .get(&meeting_id)
            .map(|&(done, total)| AudioProgress {
                audio_seconds_done: done,
                audio_seconds_total: total,
            })
    }

    pub(crate) fn clear(&self, meeting_id: Uuid) {
        self.entries().remove(&meeting_id);
    }
}

/// Clears one meeting's progress when it goes out of scope, however the request
/// ended: a result, a worker failure, an early `continue`, or a panic.
pub(crate) struct ProgressGuard {
    store: Arc<TranscriptionProgressStore>,
    meeting_id: Uuid,
}

impl ProgressGuard {
    pub(crate) fn new(store: &Arc<TranscriptionProgressStore>, meeting_id: Uuid) -> Self {
        Self {
            store: Arc::clone(store),
            meeting_id,
        }
    }
}

impl Drop for ProgressGuard {
    fn drop(&mut self) {
        self.store.clear(self.meeting_id);
    }
}

/// The library payload's shape, in its camelCase:
/// `{"audioSecondsDone": 3, "audioSecondsTotal": 9}`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AudioProgress {
    pub(crate) audio_seconds_done: u32,
    pub(crate) audio_seconds_total: u32,
}

/// The library page's progress for one meeting, or `None` when it has none,
/// including when `meeting_id` is not a meeting id at all.
pub(crate) fn library_progress(
    store: &TranscriptionProgressStore,
    meeting_id: &str,
) -> Option<AudioProgress> {
    let meeting_id = Uuid::parse_str(meeting_id).ok()?;
    store.latest(meeting_id)
}

/// One queued `transcript.create` request, start to finish.
///
/// The guard is created before the request and dropped when it returns, so the
/// progress is gone before the caller commits the transcript or takes any of its
/// early-exit branches. The heartbeat rules are exactly what the queue executor
/// applied inline before; the measurement sink is infallible, so a progress
/// event cannot make this request fail.
pub(crate) fn request_queued_transcription(
    worker: Arc<Mutex<Option<local_meeting_notes_session_core::supervision::OwnedChild>>>,
    store: &Arc<TranscriptionProgressStore>,
    meeting_id: &str,
    expected_meeting: Uuid,
    timeout: std::time::Duration,
) -> Result<HashMap<String, String>, WorkerCallError> {
    let _progress = ProgressGuard::new(store, expected_meeting);
    let worker_request_id = Mutex::new(None::<Uuid>);
    request_worker_on(
        worker,
        Operation::TranscriptCreate,
        json!({"meeting_id": meeting_id}),
        timeout,
        |progress| {
            if progress.event != ProgressEvent::CaptureState
                || progress.state != CaptureProgressState::Transcribing
                || progress.meeting_id != expected_meeting
            {
                return Err(ProtocolError::InvalidEvent);
            }
            let mut id = worker_request_id
                .lock()
                .map_err(|_| ProtocolError::InvalidEvent)?;
            if let Some(existing) = *id {
                if existing != progress.request_id {
                    return Err(ProtocolError::InvalidEvent);
                }
            } else {
                *id = Some(progress.request_id);
            }
            Ok(())
        },
        |measured| {
            if measured.meeting_id == expected_meeting {
                store.record(measured);
            }
        },
    )
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;
    use std::fs;
    use std::path::Path;
    use std::process::Command;
    use std::time::{Duration, Instant};

    use local_meeting_notes_session_core::supervision::OwnedChild;

    use super::*;
    use crate::product_coordinator::{ProcessWorkerPort, WorkerPort};
    use crate::{ApplicationState, request_worker};

    fn measurement(meeting_id: Uuid, done: u32, total: u32) -> TranscriptionProgress {
        TranscriptionProgress {
            meeting_id,
            audio_seconds_done: done,
            audio_seconds_total: total,
        }
    }

    #[test]
    fn the_store_keeps_the_latest_value_per_meeting() {
        let store = TranscriptionProgressStore::default();
        let first = Uuid::new_v4();
        let second = Uuid::new_v4();
        assert_eq!(store.latest(first), None);
        store.record(&measurement(first, 10, 100));
        store.record(&measurement(first, 40, 100));
        store.record(&measurement(second, 5, 9));
        assert_eq!(
            store.latest(first),
            Some(AudioProgress {
                audio_seconds_done: 40,
                audio_seconds_total: 100
            })
        );
        store.clear(first);
        assert_eq!(store.latest(first), None);
        assert!(
            store.latest(second).is_some(),
            "clearing one meeting leaves another's"
        );
    }

    #[test]
    fn the_payload_is_camel_case_like_the_rest_of_the_library() {
        let progress = AudioProgress {
            audio_seconds_done: 3,
            audio_seconds_total: 9,
        };
        assert_eq!(
            serde_json::to_value(progress).unwrap(),
            json!({"audioSecondsDone": 3, "audioSecondsTotal": 9})
        );
    }

    #[test]
    fn library_progress_answers_only_for_a_meeting_id_it_holds() {
        let store = TranscriptionProgressStore::default();
        let meeting_id = Uuid::new_v4();
        store.record(&measurement(meeting_id, 1, 2));
        assert!(library_progress(&store, &meeting_id.to_string()).is_some());
        assert!(library_progress(&store, &meeting_id.to_string().to_uppercase()).is_some());
        assert_eq!(library_progress(&store, &Uuid::new_v4().to_string()), None);
        assert_eq!(library_progress(&store, "not-a-meeting"), None);
        assert_eq!(library_progress(&store, ""), None);
    }

    #[test]
    fn the_guard_clears_its_meeting_on_drop_and_on_unwind() {
        let store = Arc::new(TranscriptionProgressStore::default());
        let meeting_id = Uuid::new_v4();
        let other = Uuid::new_v4();
        store.record(&measurement(other, 1, 2));
        {
            let _guard = ProgressGuard::new(&store, meeting_id);
            store.record(&measurement(meeting_id, 5, 10));
            assert!(store.latest(meeting_id).is_some());
        }
        assert_eq!(store.latest(meeting_id), None);
        assert!(store.latest(other).is_some(), "only its own meeting");

        let unwound = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _guard = ProgressGuard::new(&store, meeting_id);
            store.record(&measurement(meeting_id, 6, 10));
            panic!("the request thread died");
        }));
        assert!(unwound.is_err());
        assert_eq!(store.latest(meeting_id), None);
    }

    #[test]
    fn a_poisoned_store_still_answers_instead_of_panicking() {
        let store = Arc::new(TranscriptionProgressStore::default());
        let poisoner = store.clone();
        let _ = std::thread::spawn(move || {
            let _held = poisoner.latest.lock().unwrap();
            panic!("poison the lock");
        })
        .join();
        let meeting_id = Uuid::new_v4();
        store.record(&measurement(meeting_id, 1, 2));
        assert!(store.latest(meeting_id).is_some());
        store.clear(meeting_id);
    }

    // ---- The request paths, against a real worker process -----------------
    //
    // A shell script stands in for the Python worker: it prints the ready
    // frame, reads one command, and then plays a scenario with the request and
    // meeting ids it was given. These run the same supervisor, tracker and
    // dispatcher the app runs, so "a progress event arrives mid-request and the
    // transcript still commits" is exercised, not mocked.

    const READY: &str = r#"printf '%s\n' '{"schema":"worker-event/2","event":"worker.ready","protocol":2,"admission":"boundary-test","build":"b","runtime":{"kind":"bundled","digest":"d"},"tap":{"build":"t","available":true},"models":[],"operations":["transcript.create","transcript.retry","capture.finalize"]}'"#;
    const PRELUDE: &str = r#"
read line
rid=$(printf '%s' "$line" | sed -n 's/.*"request_id":"\([^"]*\)".*/\1/p')
mid=$(printf '%s' "$line" | sed -n 's/.*"meeting_id":"\([^"]*\)".*/\1/p')
beat() { printf '{"event":"capture.state","meeting_id":"%s","request_id":"%s","schema":"worker-event/2","state":"transcribing"}\n' "$mid" "$rid"; }
frame() { printf '{"audio_seconds_done":%s,"audio_seconds_total":%s,"event":"transcription.progress","meeting_id":"%s","request_id":"%s","schema":"worker-event/2"}\n' "$1" "$2" "$mid" "$rid"; }
ok() { printf '{"schema":"worker-result/2","request_id":"%s","ok":true,"code":null,"recoverable":null,"artifact_digests":{%s}}\n' "$rid" "$1"; }
"#;

    fn transcript_digests() -> String {
        format!(r#""transcript":"{}""#, "a".repeat(64))
    }

    fn scripted_worker(directory: &Path, body: &str) -> Arc<Mutex<Option<OwnedChild>>> {
        let script = directory.join("worker.sh");
        fs::write(&script, format!("#!/bin/sh\n{READY}\n{PRELUDE}\n{body}\n")).unwrap();
        let mut command = Command::new("/bin/sh");
        command.arg(&script);
        let mut child = OwnedChild::spawn(&mut command).unwrap();
        child
            .wait_ready(
                Duration::from_secs(5),
                &HashSet::from([
                    Operation::TranscriptCreate,
                    Operation::TranscriptRetry,
                    Operation::CaptureFinalize,
                ]),
            )
            .unwrap();
        Arc::new(Mutex::new(Some(child)))
    }

    fn stop(worker: &Arc<Mutex<Option<OwnedChild>>>) {
        if let Some(mut child) = worker.lock().unwrap().take() {
            let _ = child.stop_and_wait(Duration::from_millis(300));
        }
    }

    const TIMEOUT: Duration = Duration::from_secs(20);

    #[test]
    fn a_queued_transcription_shows_progress_mid_request_then_commits_and_clears() {
        let directory = tempfile::tempdir().unwrap();
        let gate = directory.path().join("gate");
        let worker = scripted_worker(
            directory.path(),
            &format!(
                r#"beat; frame 0 100; beat; frame 40 100
while [ ! -e '{}' ]; do sleep 0.05; done
frame 100 100; ok '{}'"#,
                gate.display(),
                transcript_digests()
            ),
        );
        let store = Arc::new(TranscriptionProgressStore::default());
        let meeting_id = Uuid::new_v4();

        let seen_mid_request = std::thread::scope(|scope| {
            let request = scope.spawn(|| {
                request_queued_transcription(
                    worker.clone(),
                    &store,
                    &meeting_id.to_string(),
                    meeting_id,
                    TIMEOUT,
                )
            });
            let started = Instant::now();
            let mut seen = None;
            while started.elapsed() < Duration::from_secs(10) {
                if let Some(progress) = store.latest(meeting_id)
                    && progress.audio_seconds_done == 40
                {
                    seen = Some(progress);
                    break;
                }
                std::thread::sleep(Duration::from_millis(20));
            }
            fs::write(&gate, b"go").unwrap();
            let digests = request.join().unwrap().expect("the request still succeeds");
            assert_eq!(digests["transcript"], "a".repeat(64));
            seen
        });

        assert_eq!(
            seen_mid_request,
            Some(AudioProgress {
                audio_seconds_done: 40,
                audio_seconds_total: 100
            }),
            "the amount was readable while the request ran"
        );
        assert_eq!(
            store.latest(meeting_id),
            None,
            "and gone before the caller commits"
        );
        stop(&worker);
    }

    #[test]
    fn progress_the_app_cannot_use_never_fails_a_queued_transcription() {
        let directory = tempfile::tempdir().unwrap();
        let worker = scripted_worker(
            directory.path(),
            &format!(
                r#"beat; frame 0 100; frame 150 100; frame -1 100; frame 7 0; frame 12.5 100; frame '"x"' 100; frame 50 100
beat; ok '{}'"#,
                transcript_digests()
            ),
        );
        let store = Arc::new(TranscriptionProgressStore::default());
        let meeting_id = Uuid::new_v4();
        let digests = request_queued_transcription(
            worker.clone(),
            &store,
            &meeting_id.to_string(),
            meeting_id,
            TIMEOUT,
        )
        .expect("an unusable amount is ignored, not fatal");
        assert_eq!(digests["transcript"], "a".repeat(64));
        assert_eq!(store.latest(meeting_id), None);
        stop(&worker);
    }

    #[test]
    fn a_rejected_queued_transcription_clears_its_progress() {
        let directory = tempfile::tempdir().unwrap();
        let worker = scripted_worker(
            directory.path(),
            r#"beat; frame 10 100
printf '{"schema":"worker-result/2","request_id":"%s","ok":false,"code":"transcription_failed","recoverable":false,"artifact_digests":{}}\n' "$rid""#,
        );
        let store = Arc::new(TranscriptionProgressStore::default());
        let meeting_id = Uuid::new_v4();
        let result = request_queued_transcription(
            worker.clone(),
            &store,
            &meeting_id.to_string(),
            meeting_id,
            TIMEOUT,
        );
        assert!(matches!(result, Err(WorkerCallError::Rejected)));
        assert_eq!(store.latest(meeting_id), None);
        stop(&worker);
    }

    #[test]
    fn a_worker_that_dies_mid_request_clears_its_progress() {
        let directory = tempfile::tempdir().unwrap();
        let worker = scripted_worker(directory.path(), "beat; frame 10 100; sleep 0.3; exit 3");
        let store = Arc::new(TranscriptionProgressStore::default());
        let meeting_id = Uuid::new_v4();
        let result = request_queued_transcription(
            worker.clone(),
            &store,
            &meeting_id.to_string(),
            meeting_id,
            TIMEOUT,
        );
        assert!(matches!(result, Err(WorkerCallError::Supervisor(_))));
        assert_eq!(store.latest(meeting_id), None);
        stop(&worker);
    }

    #[test]
    fn an_unavailable_worker_clears_progress_and_the_caller_can_continue() {
        let store = Arc::new(TranscriptionProgressStore::default());
        let meeting_id = Uuid::new_v4();
        store.record(&measurement(meeting_id, 1, 2));
        let result = request_queued_transcription(
            Arc::new(Mutex::new(None)),
            &store,
            &meeting_id.to_string(),
            meeting_id,
            TIMEOUT,
        );
        assert!(matches!(result, Err(WorkerCallError::Supervisor(_))));
        assert_eq!(store.latest(meeting_id), None);
    }

    #[test]
    fn a_transcript_retry_with_progress_still_returns_its_result() {
        // `retry_progress_is_admitted` still admits only the heartbeat. The
        // measurements go to a sink that cannot fail the request, so the retry
        // carries on and completes.
        let directory = tempfile::tempdir().unwrap();
        let worker = scripted_worker(
            directory.path(),
            &format!(
                r#"beat; frame 0 100; frame 60 100; beat; frame 100 100; ok '"candidate-transcript":"{}"'"#,
                "b".repeat(64)
            ),
        );
        let port = ProcessWorkerPort::new(worker.clone());
        let result = port
            .request_transcript_retry(json!({"meeting_id": Uuid::new_v4()}), TIMEOUT)
            .expect("retry completes");
        assert!(result.ok);
        assert_eq!(
            result.artifact_digests["candidate-transcript"],
            "b".repeat(64)
        );
        stop(&worker);
    }

    #[test]
    fn a_stray_progress_event_on_a_capture_request_is_ignored_not_fatal() {
        // `request_worker` is the path `record_transcription_heartbeat` serves,
        // and it records no progress. A worker never measures a capture, but
        // if one frame arrives it is dropped: it neither fails the request nor
        // lands in the store.
        let directory = tempfile::tempdir().unwrap();
        let worker = scripted_worker(
            directory.path(),
            r#"frame 5 10; frame 6 10; ok '"capture-session":"x","capture-mic":"y","capture-system":"z"'"#,
        );
        let state = ApplicationState::default();
        *state.worker.lock().unwrap() = worker.lock().unwrap().take();
        let meeting_id = Uuid::new_v4();
        let digests = request_worker(
            &state,
            Operation::CaptureFinalize,
            json!({"meeting_id": meeting_id}),
            TIMEOUT,
        )
        .expect("the capture request completes");
        assert_eq!(digests.len(), 3);
        assert_eq!(state.transcription_progress.latest(meeting_id), None);
        stop(&state.worker);
    }
}
