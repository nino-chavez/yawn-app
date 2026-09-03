//! Asking the corpus a question in words, and quoting what answered it.
//!
//! [`corpus_embedding`](crate::corpus_embedding) fills the vector column.
//! [`corpus_index::CorpusIndex::nearest_windows`] ranks against a vector that
//! already exists. This is the half between them that nothing had yet: turning a
//! typed sentence into that vector, and turning the ranking back into passages a
//! person can read.
//!
//! # A question goes through the same encoder as a passage
//!
//! It is embedded through [`WindowEmbedder`] — the same trait, the same
//! transport, the same manifest check before the first request. That is the
//! whole reason it is not a second code path. Two encoders that agree today
//! drift silently, and a question encoded by a different tokenizer produces a
//! vector of the right width pointing the wrong way, which no assertion
//! downstream can catch.
//!
//! # What is done to the question, and what is not
//!
//! It is trimmed. Nothing else. `notes/semantic_scale_probe.py` embedded each
//! question string as it stood — only *passages* go through the word-join in
//! [`corpus_window::window_text`], because the harness had re-split those on
//! whitespace and a stored slice would have embedded different bytes than were
//! measured. Applying that join to a question would be symmetry for its own
//! sake, and it would not be what produced the 7-of-10 figure.
//!
//! # Why a long question is refused here rather than by the worker
//!
//! The worker raises `SequenceTooLong` past 256 tokens, and it reaches this
//! process as `ok: false` — indistinguishable from a worker that died mid-batch.
//! So the length gate is local and in words: a question longer than
//! [`corpus_window::WINDOW_WORDS`] is longer than any passage it could match,
//! and refusing it costs no round trip and names its own cause.
//!
//! # Nothing prepared is not nothing found
//!
//! Coverage is read *before* the model is asked. A corpus with windows and no
//! vectors returns [`AskStop::NothingPrepared`] without a round trip, because
//! "no meeting matched" and "this Mac has not embedded anything yet" are
//! different sentences and only one of them is about the question.

use crate::corpus_embedding::{EmbedderUnavailable, WindowEmbedder};
use crate::corpus_index::{CorpusIndex, CorpusIndexError, PendingWindow, VectorCoverage};
use crate::corpus_window::{self, EmbedderIdentity};

/// Meetings offered for one question.
///
/// Five rather than one because the measured failure mode is a tie: both
/// failures at scale sat at a margin of 0.0000 against the right answer. A
/// single row hides that; five rows put the tie in front of the person who can
/// recognise the meeting the score could not.
pub const ANSWER_LIMIT: usize = 5;

/// Why an ask ended where it did. A fixed vocabulary, never transport text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AskStop {
    /// A ranking was produced. It may still be empty if nothing scored.
    Answered,
    /// The corpus holds no windows at all — no meetings, or none with speech.
    NothingToSearch,
    /// Windows exist and this embedder has answered for none of them.
    NothingPrepared,
    /// The question was empty once trimmed.
    QuestionBlank,
    /// Longer than a window, refused without a round trip.
    QuestionTooLong,
    /// The embedder failed. Carries the transport's reason for a log, never for
    /// a surface — see [`AskOutcome`].
    EmbedderUnavailable(String),
    /// The reply came back without a vector for the question that was asked.
    QuestionUnanswered,
    /// The runtime packages a different model than this identity names.
    ModelMismatch,
}

/// One meeting's best passage for the question.
#[derive(Debug, Clone, PartialEq)]
pub struct Answer {
    pub meeting_id: String,
    pub title: Option<String>,
    pub folder: Option<String>,
    pub created_at_epoch_seconds: u64,
    pub similarity: f32,
    pub quote: String,
    /// Store-numbered turns, counted from zero. See
    /// [`corpus_index::QuotedWindow`](crate::corpus_index::QuotedWindow).
    pub first_turn_index: u64,
    pub last_turn_index: u64,
}

/// What one ask produced, and what it actually searched to produce it.
#[derive(Debug, Clone, PartialEq)]
pub struct AskOutcome {
    /// Windows in the corpus against windows this embedder has answered for. A
    /// caller that reports answers without reporting this is claiming a search
    /// of the whole corpus that may have covered a fraction of it.
    pub coverage: VectorCoverage,
    /// Meetings within [`corpus_window::DENSITY_BAND`] of the top score,
    /// counted before [`ANSWER_LIMIT`] truncated anything.
    pub near_ties: usize,
    pub answers: Vec<Answer>,
    pub stop: AskStop,
}

impl AskOutcome {
    fn stopped(coverage: VectorCoverage, stop: AskStop) -> Self {
        Self {
            coverage,
            near_ties: 0,
            answers: Vec::new(),
            stop,
        }
    }
}

/// Embed `question`, rank the corpus against it, and quote the window that
/// answered for each meeting.
///
/// `packaged_models` is the runtime manifest's `(id, sha256)` pairs, checked
/// against `identity` before anything else happens — the same refusal
/// [`crate::corpus_embedding::fill_vectors`] makes, for the same reason.
///
/// The checks run cheapest-and-most-certain first: the manifest, then the
/// question's own shape, then what the store actually holds, and only then the
/// model. Every one of those can end the ask, and each names itself.
pub fn ask(
    index: &CorpusIndex,
    identity: &EmbedderIdentity,
    embedder: &dyn WindowEmbedder,
    packaged_models: &[(&str, &str)],
    question: &str,
    limit: usize,
) -> Result<AskOutcome, CorpusIndexError> {
    let coverage = index.vector_coverage(identity)?;
    if !identity.matches_manifest(packaged_models) {
        return Ok(AskOutcome::stopped(coverage, AskStop::ModelMismatch));
    }

    let question = question.trim();
    let words = question
        .split(corpus_window::is_word_separator)
        .filter(|word| !word.is_empty())
        .count();
    if words == 0 {
        return Ok(AskOutcome::stopped(coverage, AskStop::QuestionBlank));
    }
    if words > corpus_window::WINDOW_WORDS {
        return Ok(AskOutcome::stopped(coverage, AskStop::QuestionTooLong));
    }

    if coverage.windows == 0 {
        return Ok(AskOutcome::stopped(coverage, AskStop::NothingToSearch));
    }
    if coverage.embedded == 0 {
        return Ok(AskOutcome::stopped(coverage, AskStop::NothingPrepared));
    }

    // A question is not a window and this type says so in its name. It is used
    // anyway because the transport reads exactly two of its fields — the text
    // and its digest — and reusing the seam is the point. The identifiers below
    // are inert: a question's vector is returned to this function and never
    // offered to `store_window_vectors`, which is the only thing that would
    // read them.
    let digest = corpus_window::window_digest(question);
    let asked = PendingWindow {
        meeting_id: String::new(),
        window_index: 0,
        text_sha256: digest.clone(),
        text: question.to_owned(),
    };
    let replies = match embedder.embed(std::slice::from_ref(&asked)) {
        Ok(replies) => replies,
        Err(EmbedderUnavailable(reason)) => {
            return Ok(AskOutcome::stopped(
                coverage,
                AskStop::EmbedderUnavailable(reason),
            ));
        }
    };
    let Some(vector) = replies.get(&digest) else {
        return Ok(AskOutcome::stopped(coverage, AskStop::QuestionUnanswered));
    };

    let found = index.nearest_windows(identity, vector, limit)?;
    let mut answers = Vec::with_capacity(found.hits.len());
    for hit in found.hits {
        // Propagated rather than skipped. A hit the store ranked and cannot
        // quote means the index disagrees with itself, and answering with the
        // remaining rows would hide that behind a shorter list.
        let quoted = index.quote_window(&hit.meeting_id, hit.window_index)?;
        answers.push(Answer {
            meeting_id: hit.meeting_id,
            // The library list's precedence, from the module that owns it, so
            // one meeting cannot carry two names on one screen.
            title: crate::meeting_title::label(quoted.title.as_deref(), quoted.derived_title).0,
            folder: quoted.folder,
            created_at_epoch_seconds: quoted.created_at_epoch_seconds,
            similarity: hit.similarity,
            quote: quoted.quote,
            first_turn_index: quoted.first_turn_index,
            last_turn_index: quoted.last_turn_index,
        });
    }
    Ok(AskOutcome {
        coverage: found.coverage,
        near_ties: found.near_ties,
        answers,
        stop: AskStop::Answered,
    })
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::collections::HashMap;

    use super::*;
    use crate::corpus_embedding::fill_vectors;
    use crate::corpus_index::tests::{Fixture, synced};

    /// Answers with a unit vector chosen by the text's leading digest bytes, so
    /// a ranking assertion is about ranking and not about floating point. The
    /// same shape `corpus_embedding`'s tests use, plus a record of the exact
    /// strings it was handed — which is what pins how a question is prepared.
    struct FakeEmbedder {
        dimension: usize,
        seen: RefCell<Vec<String>>,
        answer_for: RefCell<Option<(String, Vec<f32>)>>,
        omit: bool,
        fail: Option<String>,
    }

    impl FakeEmbedder {
        fn new(dimension: usize) -> Self {
            Self {
                dimension,
                seen: RefCell::new(Vec::new()),
                answer_for: RefCell::new(None),
                omit: false,
                fail: None,
            }
        }

        /// Force the question's vector, so a test can aim it at a known window.
        fn aim(self, values: Vec<f32>) -> Self {
            *self.answer_for.borrow_mut() = Some((String::new(), values));
            self
        }
    }

    impl WindowEmbedder for FakeEmbedder {
        fn embed(
            &self,
            windows: &[PendingWindow],
        ) -> Result<HashMap<String, Vec<f32>>, EmbedderUnavailable> {
            for window in windows {
                self.seen.borrow_mut().push(window.text.clone());
            }
            if let Some(reason) = &self.fail {
                return Err(EmbedderUnavailable(reason.clone()));
            }
            if self.omit {
                return Ok(HashMap::new());
            }
            Ok(windows
                .iter()
                .map(|window| {
                    // A question carries no meeting, which is how the fake tells
                    // the two apart without inspecting text.
                    if window.meeting_id.is_empty()
                        && let Some((_, aimed)) = self.answer_for.borrow().as_ref()
                    {
                        return (window.text_sha256.clone(), aimed.clone());
                    }
                    let mut values = vec![0.0_f32; self.dimension];
                    let slot = usize::from_str_radix(&window.text_sha256[..4], 16).unwrap_or(0);
                    values[slot % self.dimension] = 1.0;
                    (window.text_sha256.clone(), values)
                })
                .collect())
        }
    }

    fn measured_models(identity: &EmbedderIdentity) -> Vec<(&str, &str)> {
        vec![
            (
                EmbedderIdentity::TOKENIZER_MODEL_ID,
                identity.tokenizer_sha256.as_str(),
            ),
            (
                EmbedderIdentity::WEIGHTS_MODEL_ID,
                identity.weights_sha256.as_str(),
            ),
        ]
    }

    /// A corpus of one-turn meetings, with every window embedded.
    fn prepared(turns: &[(&str, &str)]) -> (Fixture, CorpusIndex, EmbedderIdentity) {
        let fixture = Fixture::new();
        for (index, (id, text)) in turns.iter().enumerate() {
            fixture.meeting(id, 100 + index as u64, &[text]);
        }
        let mut index = synced(&fixture);
        let identity = EmbedderIdentity::measured();
        let models = measured_models(&identity);
        let embedder = FakeEmbedder::new(identity.dimension as usize);
        let outcome = fill_vectors(&mut index, &identity, &embedder, &models, 512).expect("fills");
        assert_eq!(
            outcome.stop,
            crate::corpus_embedding::FillStop::Complete,
            "the fixture corpus did not finish preparing"
        );
        (fixture, index, identity)
    }

    /// The vector the fake would produce for a given window text, so a test can
    /// ask a question that lands on a meeting it names.
    fn vector_for(text: &str, dimension: usize) -> Vec<f32> {
        let digest = corpus_window::window_digest(text);
        let mut values = vec![0.0_f32; dimension];
        let slot = usize::from_str_radix(&digest[..4], 16).unwrap_or(0);
        values[slot % dimension] = 1.0;
        values
    }

    #[test]
    fn a_question_comes_back_as_the_passage_that_answered_it() {
        let (_fixture, index, identity) = prepared(&[
            (
                "meeting-a",
                "the roof lease renews in March and the landlord wants a decision",
            ),
            (
                "meeting-b",
                "nothing to do with property at all, only the hiring loop",
            ),
        ]);
        let aimed = vector_for(
            "the roof lease renews in March and the landlord wants a decision",
            identity.dimension as usize,
        );
        let embedder = FakeEmbedder::new(identity.dimension as usize).aim(aimed);

        let outcome = ask(
            &index,
            &identity,
            &embedder,
            &measured_models(&identity),
            "when does the lease end",
            ANSWER_LIMIT,
        )
        .expect("asks");

        assert_eq!(outcome.stop, AskStop::Answered);
        assert_eq!(outcome.answers.len(), 2);
        let top = &outcome.answers[0];
        assert_eq!(top.meeting_id, "meeting-a");
        // The evidence, not a score: the words the vector was computed from.
        assert_eq!(
            top.quote,
            "the roof lease renews in March and the landlord wants a decision"
        );
        assert_eq!((top.first_turn_index, top.last_turn_index), (0, 0));
        assert!(top.similarity > outcome.answers[1].similarity);
        assert_eq!(outcome.coverage.windows, outcome.coverage.embedded);
    }

    /// The decision recorded in the module doc, pinned rather than described: a
    /// question crosses as typed, and only the whitespace around it is touched.
    #[test]
    fn a_question_crosses_as_typed_and_is_not_re_joined_like_a_passage() {
        let (_fixture, index, identity) = prepared(&[("meeting-a", "alpha beta gamma")]);
        let embedder = FakeEmbedder::new(identity.dimension as usize);

        ask(
            &index,
            &identity,
            &embedder,
            &measured_models(&identity),
            "  when   does\tthe lease\nend  ",
            ANSWER_LIMIT,
        )
        .expect("asks");

        assert_eq!(
            embedder.seen.borrow().as_slice(),
            ["when   does\tthe lease\nend"],
            "the runs of whitespace inside a question were collapsed, \
             which the measured harness never did"
        );
    }

    #[test]
    fn an_unprepared_corpus_says_so_instead_of_finding_nothing() {
        let fixture = Fixture::new();
        fixture.meeting("meeting-a", 100, &["a meeting with words in it"]);
        let index = synced(&fixture);
        let identity = EmbedderIdentity::measured();
        let embedder = FakeEmbedder::new(identity.dimension as usize);

        let outcome = ask(
            &index,
            &identity,
            &embedder,
            &measured_models(&identity),
            "anything at all",
            ANSWER_LIMIT,
        )
        .expect("asks");

        assert_eq!(outcome.stop, AskStop::NothingPrepared);
        assert!(outcome.coverage.windows > 0);
        assert_eq!(outcome.coverage.embedded, 0);
        assert!(
            embedder.seen.borrow().is_empty(),
            "the model was asked a question the store could not have answered"
        );
    }

    #[test]
    fn an_empty_corpus_and_an_unprepared_one_are_different_answers() {
        let fixture = Fixture::new();
        let index = synced(&fixture);
        let identity = EmbedderIdentity::measured();
        let embedder = FakeEmbedder::new(identity.dimension as usize);

        let outcome = ask(
            &index,
            &identity,
            &embedder,
            &measured_models(&identity),
            "anything at all",
            ANSWER_LIMIT,
        )
        .expect("asks");

        assert_eq!(outcome.stop, AskStop::NothingToSearch);
        assert_eq!(outcome.coverage.windows, 0);
    }

    #[test]
    fn a_mismatched_manifest_refuses_before_the_model_is_asked() {
        let (_fixture, index, identity) = prepared(&[("meeting-a", "alpha beta gamma")]);
        let embedder = FakeEmbedder::new(identity.dimension as usize);

        let outcome = ask(
            &index,
            &identity,
            &embedder,
            &[(EmbedderIdentity::WEIGHTS_MODEL_ID, &"f".repeat(64))],
            "anything at all",
            ANSWER_LIMIT,
        )
        .expect("asks");

        assert_eq!(outcome.stop, AskStop::ModelMismatch);
        assert!(embedder.seen.borrow().is_empty());
    }

    #[test]
    fn a_blank_question_and_one_longer_than_a_window_never_reach_the_model() {
        let (_fixture, index, identity) = prepared(&[("meeting-a", "alpha beta gamma")]);
        let embedder = FakeEmbedder::new(identity.dimension as usize);
        let models = measured_models(&identity);

        for (question, expected) in [
            ("   \t\n ".to_string(), AskStop::QuestionBlank),
            (
                vec!["word"; corpus_window::WINDOW_WORDS + 1].join(" "),
                AskStop::QuestionTooLong,
            ),
        ] {
            let outcome = ask(
                &index,
                &identity,
                &embedder,
                &models,
                &question,
                ANSWER_LIMIT,
            )
            .expect("asks");
            assert_eq!(outcome.stop, expected);
        }
        // Exactly a window's worth is the longest thing a passage can be, so it
        // is not too long to ask about.
        let edge = vec!["word"; corpus_window::WINDOW_WORDS].join(" ");
        let outcome =
            ask(&index, &identity, &embedder, &models, &edge, ANSWER_LIMIT).expect("asks");
        assert_eq!(outcome.stop, AskStop::Answered);
        assert_eq!(embedder.seen.borrow().len(), 1);
    }

    #[test]
    fn a_reply_without_the_question_in_it_is_not_a_ranking() {
        let (_fixture, index, identity) = prepared(&[("meeting-a", "alpha beta gamma")]);
        let mut embedder = FakeEmbedder::new(identity.dimension as usize);
        embedder.omit = true;

        let outcome = ask(
            &index,
            &identity,
            &embedder,
            &measured_models(&identity),
            "anything at all",
            ANSWER_LIMIT,
        )
        .expect("asks");

        assert_eq!(outcome.stop, AskStop::QuestionUnanswered);
        assert!(outcome.answers.is_empty());
    }

    #[test]
    fn a_failed_embedder_stops_the_ask_and_keeps_its_reason_out_of_the_stop() {
        let (_fixture, index, identity) = prepared(&[("meeting-a", "alpha beta gamma")]);
        let mut embedder = FakeEmbedder::new(identity.dimension as usize);
        embedder.fail = Some("/Users/someone/Library/... went away".into());

        let outcome = ask(
            &index,
            &identity,
            &embedder,
            &measured_models(&identity),
            "anything at all",
            ANSWER_LIMIT,
        )
        .expect("asks");

        match outcome.stop {
            AskStop::EmbedderUnavailable(reason) => assert!(reason.contains("went away")),
            other => panic!("expected an unavailable embedder, got {other:?}"),
        }
    }

    /// The count a surface shows must not be the count after truncation, or the
    /// tie the measurement warned about disappears exactly when it matters.
    #[test]
    fn near_ties_counts_the_crowd_and_not_the_rows_returned() {
        let identical = "the same words in every meeting so every score ties";
        let (_fixture, index, identity) = prepared(&[
            ("meeting-a", identical),
            ("meeting-b", identical),
            ("meeting-c", identical),
            ("meeting-d", identical),
        ]);
        let embedder = FakeEmbedder::new(identity.dimension as usize)
            .aim(vector_for(identical, identity.dimension as usize));

        let outcome = ask(
            &index,
            &identity,
            &embedder,
            &measured_models(&identity),
            "what did we say",
            2,
        )
        .expect("asks");

        assert_eq!(outcome.answers.len(), 2);
        assert_eq!(outcome.near_ties, 4);
    }

    /// A rename reaches the answer through the same sync that reaches exact
    /// search, so a passage is labelled with the name the operator gave it.
    #[test]
    fn an_answer_carries_the_name_the_operator_gave_the_meeting() {
        let fixture = Fixture::new();
        fixture.meeting("meeting-a", 100, &["the roof lease renews in March"]);
        crate::library_metadata::set_meeting_title(
            &fixture.storage,
            0,
            "meeting-a",
            Some("Landlord call"),
        )
        .expect("renames");
        let mut index = synced(&fixture);
        let identity = EmbedderIdentity::measured();
        let models = measured_models(&identity);
        let embedder = FakeEmbedder::new(identity.dimension as usize);
        fill_vectors(&mut index, &identity, &embedder, &models, 512).expect("fills");

        let outcome = ask(
            &index,
            &identity,
            &embedder,
            &models,
            "the lease",
            ANSWER_LIMIT,
        )
        .expect("asks");

        assert_eq!(outcome.answers[0].title.as_deref(), Some("Landlord call"));
    }

    /// The tier between an operator title and a timestamp. Before this landed,
    /// an untitled meeting read as its opening sentence in the library list and
    /// as its capture time in a search result — the same meeting, the same
    /// screen, two names.
    #[test]
    fn an_untitled_meeting_is_named_the_way_the_library_names_it() {
        let opening =
            "so the roof lease renews in March and the landlord wants a decision by Friday";
        let (_fixture, index, identity) = prepared(&[("meeting-a", opening)]);
        let embedder = FakeEmbedder::new(identity.dimension as usize);

        let outcome = ask(
            &index,
            &identity,
            &embedder,
            &measured_models(&identity),
            "the lease",
            ANSWER_LIMIT,
        )
        .expect("asks");

        let derived = crate::meeting_title::derived_title([(opening, false)]);
        assert!(
            derived.is_some(),
            "the fixture has no derived title to carry"
        );
        assert_eq!(outcome.answers[0].title, derived);
    }

    /// The release verifier's model allowlist against the manifest builder's.
    ///
    /// Two independent statements of the same set, on purpose: a verifier that
    /// read its expectations from the builder would assert the manifest equals
    /// itself, which is the charitable-self-attestation shape this repository
    /// keeps re-learning. Two statements can drift, and this one did — the four
    /// MiniLM entries were added to `build_manifest.py` on 2026-08-08 and not to
    /// `verify-release-bundle.py`, so the first build carrying the embedding
    /// model was refused at the release lane rather than in a test run.
    ///
    /// Same shape as `the_alpha_operation_set_is_read_from_the_worker_itself`,
    /// and for the same reason: the comment above the list was already correct
    /// and did not help.
    #[test]
    fn the_release_verifier_expects_the_models_the_builder_stages() {
        fn ids(source: &str, marker: &str, end_marker: &str) -> Vec<String> {
            let block = source
                .split_once(marker)
                .unwrap_or_else(|| panic!("{marker} is still present"))
                .1
                .split_once(end_marker)
                .unwrap_or_else(|| panic!("{end_marker} is still present"))
                .0;
            let mut found: Vec<String> = block
                .match_indices("all-minilm-l6-v2-")
                .chain(block.match_indices("whisper-large-v3-turbo-"))
                .map(|(at, _)| {
                    let rest = &block[at..];
                    let end = rest
                        .find('"')
                        .expect("a model identifier is a quoted string");
                    rest[..end].to_owned()
                })
                .collect();
            found.sort();
            found.dedup();
            found
        }

        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let builder = std::fs::read_to_string(root.join("worker/build_manifest.py"))
            .expect("the manifest builder is committed");
        let verifier = std::fs::read_to_string(root.join("scripts/verify-release-bundle.py"))
            .expect("the release verifier is committed");

        let staged = ids(&builder, "\"models\": [", "        ],");
        let admitted = ids(&verifier, "expected_models = {", "        actual_models =");
        assert!(!staged.is_empty(), "no models were read from the builder");
        assert_eq!(
            staged, admitted,
            "the release verifier and the manifest builder disagree about which \
             models an internal-alpha bundle carries; a build would be refused at \
             the signing lane rather than here"
        );
    }

    /// Frozen artifact. `notes/packaged_question_receipt.json` records a run
    /// through the staged runtime, and the numbers here are the literals it was
    /// produced with — deliberately not `PAIRS.len()` or a threshold constant. A
    /// test that followed a live value would keep passing while the receipt
    /// described a measurement nobody had re-run.
    ///
    /// It also asserts the receipt still describes the files on disk. Change
    /// `worker/embedding.py` or `notes/mlx_minilm.py` and this fails, which is
    /// the point: the measurement is of those bytes.
    ///
    /// It has fired once, on the 2026-09-01 import move in `embed_windows`, and
    /// the re-run that cleared it reproduced every cosine and every margin to
    /// the last digit against the 2026-08-09 original. That is the outcome this
    /// assertion is built to be unable to assume: the hash says the measured
    /// bytes moved, and only a re-run can say whether the measurement did.
    #[test]
    fn the_packaged_question_receipt_describes_the_files_it_measured() {
        use sha2::{Digest, Sha256};

        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let receipt: serde_json::Value = serde_json::from_slice(
            &std::fs::read(root.join("notes/packaged_question_receipt.json"))
                .expect("the receipt is committed"),
        )
        .expect("the receipt is JSON");

        assert_eq!(receipt["schema"], "packaged-question-parity/1");
        assert_eq!(receipt["question_ranking"]["pairs"].as_u64(), Some(5));
        assert_eq!(
            receipt["question_ranking"]["targets_first"].as_u64(),
            Some(4)
        );
        let margin = receipt["question_ranking"]["smallest_margin"]
            .as_f64()
            .expect("a margin");
        assert!(
            (margin - -0.02247599958733712).abs() < 1e-15,
            "the receipt's failing margin is not the one recorded: {margin}"
        );
        // The registered threshold was 1e-6 and both legs cleared it by six
        // orders. Asserted as the registration wrote it.
        for leg in ["padding_independence", "wrapper_parity"] {
            let worst = receipt[leg]["worst_cosine"].as_f64().expect("a cosine");
            assert!(worst >= 1.0 - 1e-6, "{leg} regressed to {worst}");
        }

        for (field, path) in [
            ("embedding_sha256", "worker/embedding.py"),
            ("mlx_minilm_sha256", "notes/mlx_minilm.py"),
            ("harness_sha256", "notes/packaged_question_parity.py"),
        ] {
            let bytes = std::fs::read(root.join(path)).expect("the measured file is committed");
            assert_eq!(
                receipt[field].as_str(),
                Some(format!("{:x}", Sha256::digest(&bytes)).as_str()),
                "{path} changed since the receipt was produced; re-run the probe"
            );
        }
    }
}
