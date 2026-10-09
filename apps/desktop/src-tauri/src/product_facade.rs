//! Internal command boundary for correction shapes and the retired note command.
//!
//! `restore_withheld_turn` was registered on 2026-08-04 by the operator's
//! correction-surface (J4) decision, with `DesktopProductCoordinator` as the
//! storage-backed owner.
//!
//! `regenerate_note` remains registered only to give stale webviews a quiet,
//! local refusal. It never reserves the single-operation slot, reads source
//! state, writes a receipt, or reaches a coordinator.

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
/// The single product policy for new local AI-note work. Existing AI drafts
/// remain readable evidence; this only closes admission for new generation.
pub(crate) const NOTE_GENERATION_RETIRED_COPY: &str =
    "Automatic note generation is no longer available.";

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
    NoteGenerationRetired,
}

impl ProductOperationFacadeError {
    pub(crate) fn safe_copy(self) -> &'static str {
        match self {
            Self::SourceChanged => SOURCE_CHANGED_COPY,
            Self::OperationUnavailable => OPERATION_UNAVAILABLE_COPY,
            Self::OperationAlreadyActive => OPERATION_ACTIVE_COPY,
            Self::NoteGenerationRetired => NOTE_GENERATION_RETIRED_COPY,
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
    fn settle(mut self, accepted: UiOperationAccepted) -> Result<(), ProductOperationFacadeError> {
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
        self.claim()
            .map_err(|_| "Wait for the current note, transcript or model change to finish.".into())
    }

    pub(crate) fn is_active(&self) -> bool {
        self.active
            .lock()
            .map(|slot| slot.is_some())
            .unwrap_or(true)
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
        _args: RegenerateNoteUiArgs,
    ) -> Result<UiOperationAccepted, ProductOperationFacadeError> {
        // Refuse before source reads, operation reservation, durable receipts,
        // or coordinator work. This must stay true even for stale UI state.
        Err(ProductOperationFacadeError::NoteGenerationRetired)
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

        let source = self.coordinator.source_for(meeting_id).map_err(|error| {
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

/// Compatibility command for older webviews. It always refuses locally and
/// does not claim an operation, read source state, or start a worker.
#[tauri::command(async, rename_all = "camelCase")]
pub(crate) fn regenerate_note(
    meeting_id: Uuid,
    source_transcript_sha256: String,
    _facade: State<'_, ProductOperationFacade>,
    _app: State<'_, crate::ApplicationState>,
) -> Result<UiOperationAccepted, String> {
    // Keep the command as a compatibility boundary for stale webviews, but
    // do not accept an operation or inspect any generation inputs.
    let _ = (meeting_id, source_transcript_sha256);
    Err(NOTE_GENERATION_RETIRED_COPY.into())
}

#[cfg(test)]
mod tests {
    use serde::de::DeserializeOwned;
    use serde_json::Value;
    use std::sync::Mutex;

    use super::*;

    struct FakeCoordinator {
        source: Mutex<Result<MeetingOperationSource, CoordinatorError>>,
        restore_result: Mutex<Result<Uuid, CoordinatorError>>,
        regeneration_result: Mutex<Result<Uuid, CoordinatorError>>,
        source_calls: Mutex<u32>,
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
                source_calls: Mutex::new(0),
                restore_calls: Mutex::new(0),
                regeneration_calls: Mutex::new(0),
            }
        }
    }

    impl ProductOperationCoordinator for FakeCoordinator {
        fn source_for(&self, _: Uuid) -> Result<MeetingOperationSource, CoordinatorError> {
            *self.source_calls.lock().unwrap() += 1;
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
    fn retired_generation_refuses_before_the_facade_reads_or_claims() {
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
            Uuid::new_v4(),
        ));
        let facade = ProductOperationFacade::new(coordinator.clone());
        assert_eq!(
            facade.regenerate_note(args),
            Err(ProductOperationFacadeError::NoteGenerationRetired)
        );
        assert_eq!(*coordinator.regeneration_calls.lock().unwrap(), 0);
        assert_eq!(*coordinator.source_calls.lock().unwrap(), 0);
        assert!(facade.claim_runtime_change().is_ok());
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
}
