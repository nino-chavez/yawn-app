//! Internal command boundary for the frozen correction and note-regeneration shapes.
//!
//! `restore_withheld_turn` was registered on 2026-08-04 by the operator's
//! correction-surface (J4) decision, with `DesktopProductCoordinator` as the
//! storage-backed owner.
//!
//! `regenerate_note` is registered in `tauri::generate_handler!` and, as of
//! the generation invocation chain landing (docs/note-runtime-decision.md,
//! slices 1–4), runs the real path: `DesktopProductCoordinator::
//! accept_regeneration` drives the sandboxed generate child and the worker's
//! `note.create` to a terminal receipt. Without an installed note model the
//! coordinator still refuses `Unavailable` before touching the worker (see
//! `regeneration_without_an_installed_note_model_is_unavailable_before_the_worker`
//! in `product_coordinator.rs`), so the rendered control degrades to the
//! facade's generic copy rather than claiming the single-operation slot.

use std::sync::{Arc, Mutex};

use local_meeting_notes_session_core::meeting::{ArtifactRef, MeetingLifecycle};
use local_meeting_notes_session_core::operations::{
    ProductOperationKind, RegenerateNoteUiArgs, RestoreWithheldTurnUiArgs, TranscriptRetryUiArgs,
    UiOperationAccepted, UiOperationSchema, UiOperationState,
};
use local_meeting_notes_session_core::transcript_retry::TranscriptRetryOutcome;
use tauri::State;
use uuid::Uuid;

const SOURCE_CHANGED_COPY: &str = "The transcript changed. Refresh the meeting and try again.";
const OPERATION_UNAVAILABLE_COPY: &str = "This action is not available right now. Try again.";
const OPERATION_ACTIVE_COPY: &str = "Another meeting action is already in progress.";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MeetingOperationSource {
    pub meeting_id: Uuid,
    pub current_transcript_sha256: String,
    pub lifecycle: MeetingLifecycle,
    pub has_current_note: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CoordinatorError {
    Unavailable,
    Refused,
}

/// References that have passed the authority's durable source/candidate
/// checks. The presentation layer still reads each artifact again before it
/// projects any text for the webview.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TranscriptRetryOperation {
    pub(crate) operation_id: Uuid,
    pub(crate) meeting_id: Uuid,
    pub(crate) source_transcript_sha256: String,
    pub(crate) candidate_transcript: ArtifactRef,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TranscriptRetryDecision {
    KeepCurrent,
    UseRetry,
}

/// The future storage/worker owner. It must re-check the current source while
/// preparing its durable operation receipt; this facade deliberately owns none
/// of that persistence or worker protocol.
pub(crate) trait ProductOperationCoordinator: Send + Sync {
    fn source_for(&self, meeting_id: Uuid) -> Result<MeetingOperationSource, CoordinatorError>;

    fn accept_restore(&self, args: &RestoreWithheldTurnUiArgs) -> Result<Uuid, CoordinatorError>;

    fn accept_regeneration(&self, args: &RegenerateNoteUiArgs) -> Result<Uuid, CoordinatorError>;

    fn start_transcript_retry(
        &self,
        _: &TranscriptRetryUiArgs,
    ) -> Result<TranscriptRetryOperation, CoordinatorError> {
        Err(CoordinatorError::Unavailable)
    }

    fn pending_transcript_retry(
        &self,
        _: &TranscriptRetryUiArgs,
    ) -> Result<Option<TranscriptRetryOperation>, CoordinatorError> {
        Err(CoordinatorError::Unavailable)
    }

    fn decide_transcript_retry(
        &self,
        _: &TranscriptRetryOperation,
        _: TranscriptRetryDecision,
    ) -> Result<TranscriptRetryOutcome, CoordinatorError> {
        Err(CoordinatorError::Unavailable)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProductOperationFacadeError {
    SourceChanged,
    OperationUnavailable,
    OperationAlreadyActive,
}

impl ProductOperationFacadeError {
    pub(crate) fn safe_copy(self) -> &'static str {
        match self {
            Self::SourceChanged => SOURCE_CHANGED_COPY,
            Self::OperationUnavailable => OPERATION_UNAVAILABLE_COPY,
            Self::OperationAlreadyActive => OPERATION_ACTIVE_COPY,
        }
    }
}

/// The one product-operation slot.
///
/// `Starting` is the reservation a caller holds while its coordinator call
/// runs. Before it existed, the slot's mutex guard was held across that call
/// -- which for a note generation is minutes -- and that only became
/// observable when D-FREEZE moved the generation off the main thread: a
/// second operation stopped being *refused* and started *queueing* behind
/// the mutex, on the main thread, for as long as the first one took. The
/// facade's contract is one at a time and a prompt refusal, so the slot is
/// claimed under the lock and the lock is then released.
enum ActiveOperation {
    Starting,
    Running(UiOperationAccepted),
}

/// A held claim on the slot. Dropping it without `settle` releases the slot,
/// so an early return, an error, or a panic cannot strand it as `Starting`.
pub(crate) struct OperationClaim {
    slot: Arc<Mutex<Option<ActiveOperation>>>,
    settled: bool,
}

impl OperationClaim {
    fn settle(
        mut self,
        accepted: UiOperationAccepted,
    ) -> Result<(), ProductOperationFacadeError> {
        let mut slot = self
            .slot
            .lock()
            .map_err(|_| ProductOperationFacadeError::OperationUnavailable)?;
        *slot = Some(ActiveOperation::Running(accepted));
        self.settled = true;
        Ok(())
    }
}

impl Drop for OperationClaim {
    fn drop(&mut self) {
        if self.settled {
            return;
        }
        if let Ok(mut slot) = self.slot.lock() {
            *slot = None;
        }
    }
}

pub(crate) struct ProductOperationFacade {
    coordinator: Arc<dyn ProductOperationCoordinator>,
    active: Arc<Mutex<Option<ActiveOperation>>>,
}

impl ProductOperationFacade {
    pub(crate) fn new(coordinator: Arc<dyn ProductOperationCoordinator>) -> Self {
        Self {
            coordinator,
            active: Arc::new(Mutex::new(None)),
        }
    }

    /// Reserves the slot or refuses. Held only for the check and the write.
    fn claim(&self) -> Result<OperationClaim, ProductOperationFacadeError> {
        let mut slot = self
            .active
            .lock()
            .map_err(|_| ProductOperationFacadeError::OperationUnavailable)?;
        if slot.is_some() {
            return Err(ProductOperationFacadeError::OperationAlreadyActive);
        }
        *slot = Some(ActiveOperation::Starting);
        Ok(OperationClaim {
            slot: self.active.clone(),
            settled: false,
        })
    }

    /// Holds the same slot as notes, corrections and retries across a runtime
    /// change. Ownership can move to the background task; drop releases it on
    /// every exit, including a failed thread spawn.
    pub(crate) fn claim_runtime_change(&self) -> Result<OperationClaim, String> {
        self.claim().map_err(|_| "Wait for the current note, transcript or model change to finish.".into())
    }

    pub(crate) fn is_active(&self) -> bool {
        self.active.lock().map(|slot| slot.is_some()).unwrap_or(true)
    }

    pub(crate) fn restore_withheld_turn(
        &self,
        args: RestoreWithheldTurnUiArgs,
    ) -> Result<UiOperationAccepted, ProductOperationFacadeError> {
        args.validate()
            .map_err(|_| ProductOperationFacadeError::SourceChanged)?;
        self.accept(
            args.meeting_id,
            &args.source_transcript_sha256,
            ProductOperationKind::RestoreWithheldTurn,
            UiOperationState::Correcting,
            |source| match source.lifecycle {
                MeetingLifecycle::Ready => source.has_current_note,
                MeetingLifecycle::TranscriptReady | MeetingLifecycle::SummaryFailed => {
                    !source.has_current_note
                }
                _ => false,
            },
            || self.coordinator.accept_restore(&args),
        )
    }

    pub(crate) fn regenerate_note(
        &self,
        args: RegenerateNoteUiArgs,
    ) -> Result<UiOperationAccepted, ProductOperationFacadeError> {
        args.validate()
            .map_err(|_| ProductOperationFacadeError::SourceChanged)?;
        self.accept(
            args.meeting_id,
            &args.source_transcript_sha256,
            ProductOperationKind::GenerateNote,
            UiOperationState::Summarizing,
            |source| {
                matches!(
                    (source.lifecycle, source.has_current_note),
                    (MeetingLifecycle::Ready, true)
                        | (
                            MeetingLifecycle::TranscriptReady | MeetingLifecycle::SummaryFailed,
                            false
                        )
                )
            },
            || self.coordinator.accept_regeneration(&args),
        )
    }

    pub(crate) fn start_transcript_retry(
        &self,
        args: TranscriptRetryUiArgs,
    ) -> Result<TranscriptRetryOperation, ProductOperationFacadeError> {
        args.validate()
            .map_err(|_| ProductOperationFacadeError::SourceChanged)?;
        let claim = self.claim()?;
        let source = self
            .coordinator
            .source_for(args.meeting_id)
            .map_err(|_| ProductOperationFacadeError::OperationUnavailable)?;
        if source.meeting_id != args.meeting_id
            || source.current_transcript_sha256 != args.source_transcript_sha256
            || !matches!(
                source.lifecycle,
                MeetingLifecycle::TranscriptReady
                    | MeetingLifecycle::SummaryFailed
                    | MeetingLifecycle::Ready
            )
        {
            return Err(ProductOperationFacadeError::SourceChanged);
        }
        let operation = self
            .coordinator
            .start_transcript_retry(&args)
            .map_err(|_| ProductOperationFacadeError::OperationUnavailable)?;
        if operation.meeting_id != args.meeting_id
            || operation.source_transcript_sha256 != args.source_transcript_sha256
        {
            return Err(ProductOperationFacadeError::OperationUnavailable);
        }
        let accepted = UiOperationAccepted {
            schema: UiOperationSchema::V1,
            operation_id: operation.operation_id,
            meeting_id: args.meeting_id,
            kind: ProductOperationKind::TranscriptRetry,
            state: UiOperationState::Transcribing,
        };
        accepted
            .validate()
            .map_err(|_| ProductOperationFacadeError::OperationUnavailable)?;
        claim.settle(accepted)?;
        Ok(operation)
    }

    pub(crate) fn pending_transcript_retry(
        &self,
        args: TranscriptRetryUiArgs,
    ) -> Result<Option<TranscriptRetryOperation>, ProductOperationFacadeError> {
        args.validate()
            .map_err(|_| ProductOperationFacadeError::SourceChanged)?;
        let source = self
            .coordinator
            .source_for(args.meeting_id)
            .map_err(|_| ProductOperationFacadeError::OperationUnavailable)?;
        if source.meeting_id != args.meeting_id
            || source.current_transcript_sha256 != args.source_transcript_sha256
        {
            return Err(ProductOperationFacadeError::SourceChanged);
        }
        self.coordinator
            .pending_transcript_retry(&args)
            .map_err(|_| ProductOperationFacadeError::OperationUnavailable)
    }

    pub(crate) fn decide_transcript_retry(
        &self,
        operation: TranscriptRetryOperation,
        decision: TranscriptRetryDecision,
    ) -> Result<TranscriptRetryOutcome, ProductOperationFacadeError> {
        let source = self
            .coordinator
            .source_for(operation.meeting_id)
            .map_err(|_| ProductOperationFacadeError::OperationUnavailable)?;
        if source.meeting_id != operation.meeting_id
            || source.current_transcript_sha256 != operation.source_transcript_sha256
        {
            return Err(ProductOperationFacadeError::SourceChanged);
        }
        self.coordinator
            .decide_transcript_retry(&operation, decision)
            .map_err(|_| ProductOperationFacadeError::OperationUnavailable)
    }

    fn accept(
        &self,
        meeting_id: Uuid,
        source_transcript_sha256: &str,
        kind: ProductOperationKind,
        state: UiOperationState,
        source_is_eligible: impl FnOnce(&MeetingOperationSource) -> bool,
        accept: impl FnOnce() -> Result<Uuid, CoordinatorError>,
    ) -> Result<UiOperationAccepted, ProductOperationFacadeError> {
        let claim = self.claim()?;

        let source = self
            .coordinator
            .source_for(meeting_id)
            .map_err(|error| {
                if local_meeting_notes_session_core::note_projector_process::note_trace_enabled() {
                    eprintln!("[note-trace] facade source_for failed: {error:?}");
                }
                ProductOperationFacadeError::OperationUnavailable
            })?;
        if source.meeting_id != meeting_id
            || source.current_transcript_sha256 != source_transcript_sha256
            || !source_is_eligible(&source)
        {
            if local_meeting_notes_session_core::note_projector_process::note_trace_enabled() {
                eprintln!(
                    "[note-trace] facade source changed: id_match={} sha_match={} lifecycle={:?} has_note={}",
                    source.meeting_id == meeting_id,
                    source.current_transcript_sha256 == source_transcript_sha256,
                    source.lifecycle,
                    source.has_current_note
                );
            }
            return Err(ProductOperationFacadeError::SourceChanged);
        }

        let accepted = UiOperationAccepted {
            schema: UiOperationSchema::V1,
            operation_id: accept()
                .map_err(|_| ProductOperationFacadeError::OperationUnavailable)?,
            meeting_id,
            kind,
            state,
        };
        accepted
            .validate()
            .map_err(|_| ProductOperationFacadeError::OperationUnavailable)?;
        claim.settle(accepted.clone())?;
        Ok(accepted)
    }

    /// Called only by the future coordinator after it reaches a terminal receipt.
    pub(crate) fn finish(&self, operation_id: Uuid) {
        let Ok(mut active) = self.active.lock() else {
            return;
        };
        let settled_match = match active.as_ref() {
            Some(ActiveOperation::Running(operation)) => operation.operation_id == operation_id,
            // `Starting` carries no id yet; its own claim releases the slot.
            _ => false,
        };
        if settled_match {
            *active = None;
        }
    }
}

/// Registered correction command. The storage-backed coordinator completes
/// the restoration synchronously — worker round trip, re-verification,
/// publication — before returning, so a successful operation is already
/// terminal and releases the single-operation slot here rather than waiting
/// on a coordinator callback that will never come.
// R28: the coordinator runs to a terminal receipt here, through a worker
// request bounded by WORKER_REQUEST_TIMEOUT, so this waits seconds on the
// thread that draws the window unless it is moved off it. Attribute rather
// than `async fn`, because the signature borrows `State<'_, _>`.
#[tauri::command(async, rename_all = "camelCase")]
pub(crate) fn restore_withheld_turn(
    meeting_id: Uuid,
    source_transcript_sha256: String,
    source_turn_index: u32,
    facade: State<'_, ProductOperationFacade>,
    app: State<'_, crate::ApplicationState>,
) -> Result<UiOperationAccepted, String> {
    // An active setup recording holds the app-data writer lock this
    // restoration's coordination handle would queue behind; refuse in the
    // recorder's own vocabulary instead of hanging the call.
    if crate::sitting_task_active(&app) {
        return Err("Finish the setup recording first.".into());
    }
    let capture = app
        .model
        .lock()
        .map_err(|_| "Finish the current recording first.".to_owned())?
        .reducer
        .capture();
    if capture != local_meeting_notes_session_core::reducer::CaptureState::Idle {
        return Err("Finish the current recording first.".into());
    }
    let accepted = facade
        .restore_withheld_turn(RestoreWithheldTurnUiArgs {
            meeting_id,
            source_transcript_sha256,
            source_turn_index,
        })
        .map_err(ProductOperationFacadeError::safe_copy)
        .map_err(str::to_owned)?;
    facade.finish(accepted.operation_id);
    Ok(accepted)
}

/// Registered generation command. Like restoration, the storage-backed
/// coordinator runs to a terminal receipt before returning — but the middle
/// of this one is the sandboxed generate child, so the call legitimately
/// lasts minutes.
///
/// `async` in the attribute is load-bearing, and its absence was a real
/// freeze. Tauri v2 runs a command without it **on the main thread**
/// ("Commands without the async keyword are executed on the main thread
/// unless defined with `#[tauri::command(async)]`", v2.tauri.app/develop/
/// calling-rust, tauri 2.11.5), so this call held the UI for the whole
/// generation: no repaint, no snapshot poll, and the help text under the
/// button promising "you can keep using Yawn" was false. Written in the
/// attribute rather than as an `async fn` because the signature takes
/// `State<'_, _>`, which an async command cannot borrow.
///
/// The rest of the design already assumed this: the UI's snapshot polling
/// continues meanwhile, and the facade's single-operation slot keeps a
/// second product operation from starting underneath it. A rejected note is
/// still a terminal receipt: the command returns the accepted operation and
/// the meeting's summary-failed state carries the product answer.
#[tauri::command(async, rename_all = "camelCase")]
pub(crate) fn regenerate_note(
    meeting_id: Uuid,
    source_transcript_sha256: String,
    facade: State<'_, ProductOperationFacade>,
    app: State<'_, crate::ApplicationState>,
) -> Result<UiOperationAccepted, String> {
    // Same writer-lock contention rule as restoration: an active setup
    // recording holds the app-data writer lock this generation's
    // coordination handle would queue behind.
    if crate::sitting_task_active(&app) {
        return Err("Finish the setup recording first.".into());
    }
    let accepted = facade
        .regenerate_note(RegenerateNoteUiArgs {
            meeting_id,
            speaker_label_overrides: crate::speaker_label_overrides_for(
                meeting_id,
                &source_transcript_sha256,
                &app,
            )?,
            vocabulary_replacements: crate::vocabulary_replacements_for(
                meeting_id,
                &source_transcript_sha256,
                &app,
            )?,
            pre_meeting_context: crate::pre_meeting_context_for(meeting_id, &app)?,
            source_transcript_sha256,
        })
        .map_err(|error| {
            if local_meeting_notes_session_core::note_projector_process::note_trace_enabled() {
                eprintln!("[note-trace] regenerate_note command refused: {error:?}");
            }
            error
        })
        .map_err(ProductOperationFacadeError::safe_copy)
        .map_err(str::to_owned)?;
    facade.finish(accepted.operation_id);
    Ok(accepted)
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;
    use std::time::Duration;

    use serde::de::DeserializeOwned;
    use serde_json::Value;

    use super::*;

    struct FakeCoordinator {
        source: Mutex<Result<MeetingOperationSource, CoordinatorError>>,
        restore_result: Mutex<Result<Uuid, CoordinatorError>>,
        regeneration_result: Mutex<Result<Uuid, CoordinatorError>>,
        restore_calls: Mutex<u32>,
        regeneration_calls: Mutex<u32>,
    }

    impl FakeCoordinator {
        fn accepting(
            source: MeetingOperationSource,
            restore_id: Uuid,
            regeneration_id: Uuid,
        ) -> Self {
            Self {
                source: Mutex::new(Ok(source)),
                restore_result: Mutex::new(Ok(restore_id)),
                regeneration_result: Mutex::new(Ok(regeneration_id)),
                restore_calls: Mutex::new(0),
                regeneration_calls: Mutex::new(0),
            }
        }
    }

    impl ProductOperationCoordinator for FakeCoordinator {
        fn source_for(&self, _: Uuid) -> Result<MeetingOperationSource, CoordinatorError> {
            self.source.lock().unwrap().clone()
        }

        fn accept_restore(&self, _: &RestoreWithheldTurnUiArgs) -> Result<Uuid, CoordinatorError> {
            *self.restore_calls.lock().unwrap() += 1;
            *self.restore_result.lock().unwrap()
        }

        fn accept_regeneration(&self, _: &RegenerateNoteUiArgs) -> Result<Uuid, CoordinatorError> {
            *self.regeneration_calls.lock().unwrap() += 1;
            *self.regeneration_result.lock().unwrap()
        }
    }

    fn fixture() -> Value {
        serde_json::from_str(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../tests/fixtures/product-operations-v1.json"
        )))
        .unwrap()
    }

    fn at(root: &Value, path: &[&str]) -> Value {
        path.iter()
            .fold(root, |value, segment| &value[*segment])
            .clone()
    }

    fn parse<T: DeserializeOwned>(root: &Value, path: &[&str]) -> T {
        serde_json::from_value(at(root, path)).unwrap()
    }

    fn source_for(
        meeting_id: Uuid,
        current_transcript_sha256: String,
        lifecycle: MeetingLifecycle,
        has_current_note: bool,
    ) -> MeetingOperationSource {
        MeetingOperationSource {
            meeting_id,
            current_transcript_sha256,
            lifecycle,
            has_current_note,
        }
    }

    #[test]
    fn shared_fixture_freezes_the_accepted_facade_states() {
        let fixture = fixture();

        let restore_args: RestoreWithheldTurnUiArgs =
            parse(&fixture, &["restoration", "ui_arguments"]);
        let restore_expected: UiOperationAccepted =
            parse(&fixture, &["restoration", "ui_response"]);
        let restore_coordinator = Arc::new(FakeCoordinator::accepting(
            source_for(
                restore_args.meeting_id,
                restore_args.source_transcript_sha256.clone(),
                MeetingLifecycle::Ready,
                true,
            ),
            restore_expected.operation_id,
            Uuid::nil(),
        ));
        let restore_facade = ProductOperationFacade::new(restore_coordinator.clone());
        assert_eq!(
            restore_facade.restore_withheld_turn(restore_args).unwrap(),
            restore_expected
        );
        assert_eq!(*restore_coordinator.restore_calls.lock().unwrap(), 1);

        let note_args: RegenerateNoteUiArgs = parse(&fixture, &["accepted_note", "ui_arguments"]);
        let note_expected: UiOperationAccepted = parse(&fixture, &["accepted_note", "ui_response"]);
        let note_coordinator = Arc::new(FakeCoordinator::accepting(
            source_for(
                note_args.meeting_id,
                note_args.source_transcript_sha256.clone(),
                MeetingLifecycle::TranscriptReady,
                false,
            ),
            Uuid::nil(),
            note_expected.operation_id,
        ));
        let note_facade = ProductOperationFacade::new(note_coordinator.clone());
        assert_eq!(
            note_facade.regenerate_note(note_args.clone()).unwrap(),
            note_expected
        );
        assert_eq!(*note_coordinator.regeneration_calls.lock().unwrap(), 1);

        let replacement_coordinator = Arc::new(FakeCoordinator::accepting(
            source_for(
                note_args.meeting_id,
                note_args.source_transcript_sha256.clone(),
                MeetingLifecycle::Ready,
                true,
            ),
            Uuid::nil(),
            note_expected.operation_id,
        ));
        let replacement_facade = ProductOperationFacade::new(replacement_coordinator.clone());
        assert_eq!(
            replacement_facade.regenerate_note(note_args).unwrap(),
            note_expected
        );
        assert_eq!(
            *replacement_coordinator.regeneration_calls.lock().unwrap(),
            1
        );
    }

    #[test]
    fn stale_source_and_coordinator_refusal_do_not_start_an_operation() {
        let fixture = fixture();
        let args: RestoreWithheldTurnUiArgs = parse(&fixture, &["restoration", "ui_arguments"]);
        let expected: UiOperationAccepted = parse(&fixture, &["restoration", "ui_response"]);
        let coordinator = Arc::new(FakeCoordinator::accepting(
            source_for(
                args.meeting_id,
                "b".repeat(64),
                MeetingLifecycle::Ready,
                true,
            ),
            expected.operation_id,
            Uuid::nil(),
        ));
        let facade = ProductOperationFacade::new(coordinator.clone());
        assert_eq!(
            facade.restore_withheld_turn(args.clone()),
            Err(ProductOperationFacadeError::SourceChanged)
        );
        assert_eq!(*coordinator.restore_calls.lock().unwrap(), 0);

        *coordinator.source.lock().unwrap() = Ok(source_for(
            args.meeting_id,
            args.source_transcript_sha256.clone(),
            MeetingLifecycle::Ready,
            true,
        ));
        *coordinator.restore_result.lock().unwrap() = Err(CoordinatorError::Refused);
        assert_eq!(
            facade.restore_withheld_turn(args),
            Err(ProductOperationFacadeError::OperationUnavailable)
        );
        assert_eq!(*coordinator.restore_calls.lock().unwrap(), 1);
    }

    #[test]
    fn restore_refuses_lifecycle_and_note_pointer_mismatches() {
        let fixture = fixture();
        let args: RestoreWithheldTurnUiArgs = parse(&fixture, &["restoration", "ui_arguments"]);
        let expected: UiOperationAccepted = parse(&fixture, &["restoration", "ui_response"]);

        for (lifecycle, has_current_note) in [
            (MeetingLifecycle::Ready, false),
            (MeetingLifecycle::TranscriptReady, true),
            (MeetingLifecycle::SummaryFailed, true),
        ] {
            let coordinator = Arc::new(FakeCoordinator::accepting(
                source_for(
                    args.meeting_id,
                    args.source_transcript_sha256.clone(),
                    lifecycle,
                    has_current_note,
                ),
                expected.operation_id,
                Uuid::nil(),
            ));
            let facade = ProductOperationFacade::new(coordinator.clone());
            assert_eq!(
                facade.restore_withheld_turn(args.clone()),
                Err(ProductOperationFacadeError::SourceChanged)
            );
            assert_eq!(*coordinator.restore_calls.lock().unwrap(), 0);
        }
    }

    #[test]
    fn runtime_changes_and_meeting_actions_exclude_each_other() {
        let fixture = fixture();
        let args: RegenerateNoteUiArgs = parse(&fixture, &["accepted_note", "ui_arguments"]);
        let expected: UiOperationAccepted = parse(&fixture, &["accepted_note", "ui_response"]);
        let facade = ProductOperationFacade::new(Arc::new(FakeCoordinator::accepting(
            source_for(args.meeting_id, args.source_transcript_sha256.clone(),
                MeetingLifecycle::TranscriptReady, false),
            Uuid::nil(), expected.operation_id,
        )));
        let change = facade.claim_runtime_change().unwrap();
        assert!(facade.is_active());
        assert_eq!(facade.regenerate_note(args.clone()),
            Err(ProductOperationFacadeError::OperationAlreadyActive));
        assert!(facade.claim_runtime_change().is_err());
        // An unrelated completion cannot clear the runtime reservation.
        facade.finish(expected.operation_id);
        assert!(facade.is_active());
        // The background task owns the claim, including its error exit.
        std::thread::spawn(move || drop(change)).join().unwrap();
        assert!(!facade.is_active());
        let accepted = facade.regenerate_note(args).unwrap();
        assert!(facade.claim_runtime_change().is_err());
        facade.finish(accepted.operation_id);
        assert!(!facade.is_active());
        assert!(facade.claim_runtime_change().is_ok());
    }

    #[test]
    fn a_second_operation_is_refused_until_the_first_reaches_a_terminal_receipt() {
        let fixture = fixture();
        let args: RegenerateNoteUiArgs = parse(&fixture, &["accepted_note", "ui_arguments"]);
        let expected: UiOperationAccepted = parse(&fixture, &["accepted_note", "ui_response"]);
        let coordinator = Arc::new(FakeCoordinator::accepting(
            source_for(
                args.meeting_id,
                args.source_transcript_sha256.clone(),
                MeetingLifecycle::TranscriptReady,
                false,
            ),
            Uuid::nil(),
            expected.operation_id,
        ));
        let facade = ProductOperationFacade::new(coordinator.clone());
        let accepted = facade.regenerate_note(args.clone()).unwrap();
        assert_eq!(
            facade.regenerate_note(args.clone()),
            Err(ProductOperationFacadeError::OperationAlreadyActive)
        );
        assert_eq!(*coordinator.regeneration_calls.lock().unwrap(), 1);

        facade.finish(accepted.operation_id);
        assert_eq!(facade.regenerate_note(args).unwrap(), expected);
        assert_eq!(*coordinator.regeneration_calls.lock().unwrap(), 2);
    }

    /// A coordinator that parks inside `accept_regeneration`, so a test can
    /// ask the facade a question while an operation is genuinely in flight.
    struct ParkingCoordinator {
        source: MeetingOperationSource,
        operation_id: Uuid,
        entered: std::sync::mpsc::SyncSender<()>,
        release: Mutex<std::sync::mpsc::Receiver<()>>,
    }

    impl ProductOperationCoordinator for ParkingCoordinator {
        fn source_for(&self, _: Uuid) -> Result<MeetingOperationSource, CoordinatorError> {
            Ok(self.source.clone())
        }
        fn accept_restore(&self, _: &RestoreWithheldTurnUiArgs) -> Result<Uuid, CoordinatorError> {
            Err(CoordinatorError::Unavailable)
        }
        fn accept_regeneration(
            &self,
            _: &RegenerateNoteUiArgs,
        ) -> Result<Uuid, CoordinatorError> {
            self.entered.send(()).expect("the test is listening");
            self.release
                .lock()
                .expect("release receiver")
                .recv()
                .expect("the test releases this operation");
            Ok(self.operation_id)
        }
    }

    #[test]
    fn a_second_operation_is_refused_while_the_first_is_still_running_not_queued_behind_it() {
        // D-FREEZE made operations able to overlap for the first time, which
        // exposed that the slot's mutex was held across the coordinator call:
        // a second attempt blocked for the length of the first instead of
        // being refused. Without a released lock this test does not fail, it
        // hangs, so the refusal is asserted from a thread with a deadline.
        let fixture = fixture();
        let args: RegenerateNoteUiArgs = parse(&fixture, &["accepted_note", "ui_arguments"]);
        let expected: UiOperationAccepted = parse(&fixture, &["accepted_note", "ui_response"]);
        let (entered_tx, entered_rx) = std::sync::mpsc::sync_channel(1);
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        let facade = Arc::new(ProductOperationFacade::new(Arc::new(ParkingCoordinator {
            source: source_for(
                args.meeting_id,
                args.source_transcript_sha256.clone(),
                MeetingLifecycle::TranscriptReady,
                false,
            ),
            operation_id: expected.operation_id,
            entered: entered_tx,
            release: Mutex::new(release_rx),
        })));

        let running = std::thread::spawn({
            let facade = facade.clone();
            let args = args.clone();
            move || facade.regenerate_note(args)
        });
        entered_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("the first operation reached the coordinator");

        // The first operation is parked inside the coordinator right now.
        let (refused_tx, refused_rx) = std::sync::mpsc::channel();
        std::thread::spawn({
            let facade = facade.clone();
            let args = args.clone();
            move || {
                let _ = refused_tx.send(facade.regenerate_note(args));
            }
        });
        let refused = refused_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("the second attempt answered instead of queueing behind the first");
        assert_eq!(refused, Err(ProductOperationFacadeError::OperationAlreadyActive));
        assert!(facade.claim_runtime_change().is_err());

        release_tx.send(()).expect("the parked operation is waiting");
        assert_eq!(running.join().expect("the first operation finished"), Ok(expected.clone()));

        // The slot is settled, so it still takes a terminal receipt to clear.
        assert_eq!(
            facade.regenerate_note(args.clone()),
            Err(ProductOperationFacadeError::OperationAlreadyActive)
        );
        facade.finish(expected.operation_id);
    }

    #[test]
    fn a_refused_start_releases_the_slot_it_reserved() {
        // The claim is RAII: an early return between reserving the slot and
        // settling it must not strand the facade as permanently busy.
        let fixture = fixture();
        let args: RegenerateNoteUiArgs = parse(&fixture, &["accepted_note", "ui_arguments"]);
        let expected: UiOperationAccepted = parse(&fixture, &["accepted_note", "ui_response"]);
        let coordinator = Arc::new(FakeCoordinator::accepting(
            source_for(
                args.meeting_id,
                args.source_transcript_sha256.clone(),
                // Wrong lifecycle: `accept` returns SourceChanged after the
                // claim is taken and before it is settled.
                MeetingLifecycle::Captured,
                false,
            ),
            Uuid::nil(),
            expected.operation_id,
        ));
        let facade = ProductOperationFacade::new(coordinator.clone());
        assert_eq!(
            facade.regenerate_note(args.clone()),
            Err(ProductOperationFacadeError::SourceChanged)
        );
        assert_eq!(*coordinator.regeneration_calls.lock().unwrap(), 0);

        // The slot is free again, so an eligible source is accepted.
        *coordinator.source.lock().unwrap() = Ok(source_for(
            args.meeting_id,
            args.source_transcript_sha256.clone(),
            MeetingLifecycle::TranscriptReady,
            false,
        ));
        assert_eq!(facade.regenerate_note(args).unwrap(), expected);
    }

    #[test]
    fn coordinator_failures_have_generic_safe_copy() {
        let fixture = fixture();
        let args: RegenerateNoteUiArgs = parse(&fixture, &["accepted_note", "ui_arguments"]);
        let coordinator = Arc::new(FakeCoordinator::accepting(
            source_for(
                args.meeting_id,
                args.source_transcript_sha256.clone(),
                MeetingLifecycle::TranscriptReady,
                false,
            ),
            Uuid::nil(),
            Uuid::nil(),
        ));
        *coordinator.source.lock().unwrap() = Err(CoordinatorError::Unavailable);
        let facade = ProductOperationFacade::new(coordinator);

        assert_eq!(
            facade.regenerate_note(args).unwrap_err().safe_copy(),
            OPERATION_UNAVAILABLE_COPY
        );
    }
}
