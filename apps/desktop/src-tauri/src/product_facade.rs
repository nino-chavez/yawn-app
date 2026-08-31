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

pub(crate) struct ProductOperationFacade {
    coordinator: Arc<dyn ProductOperationCoordinator>,
    active: Mutex<Option<UiOperationAccepted>>,
}

impl ProductOperationFacade {
    pub(crate) fn new(coordinator: Arc<dyn ProductOperationCoordinator>) -> Self {
        Self {
            coordinator,
            active: Mutex::new(None),
        }
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
        let mut active = self
            .active
            .lock()
            .map_err(|_| ProductOperationFacadeError::OperationUnavailable)?;
        if active.is_some() {
            return Err(ProductOperationFacadeError::OperationAlreadyActive);
        }
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
        *active = Some(accepted);
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
        let mut active = self
            .active
            .lock()
            .map_err(|_| ProductOperationFacadeError::OperationUnavailable)?;
        if active.is_some() {
            return Err(ProductOperationFacadeError::OperationAlreadyActive);
        }

        let source = self
            .coordinator
            .source_for(meeting_id)
            .map_err(|_| ProductOperationFacadeError::OperationUnavailable)?;
        if source.meeting_id != meeting_id
            || source.current_transcript_sha256 != source_transcript_sha256
            || !source_is_eligible(&source)
        {
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
        *active = Some(accepted.clone());
        Ok(accepted)
    }

    /// Called only by the future coordinator after it reaches a terminal receipt.
    pub(crate) fn finish(&self, operation_id: Uuid) {
        let Ok(mut active) = self.active.lock() else {
            return;
        };
        if active
            .as_ref()
            .is_some_and(|operation| operation.operation_id == operation_id)
        {
            *active = None;
        }
    }
}

/// Registered correction command. The storage-backed coordinator completes
/// the restoration synchronously — worker round trip, re-verification,
/// publication — before returning, so a successful operation is already
/// terminal and releases the single-operation slot here rather than waiting
/// on a coordinator callback that will never come.
#[tauri::command(rename_all = "camelCase")]
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
/// lasts minutes. Tauri runs sync commands off the main thread, the UI's
/// snapshot polling continues meanwhile, and the facade's single-operation
/// slot keeps a second product operation from starting underneath it. A
/// rejected note is still a terminal receipt: the command returns the
/// accepted operation and the meeting's summary-failed state carries the
/// product answer.
#[tauri::command(rename_all = "camelCase")]
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
        .map_err(ProductOperationFacadeError::safe_copy)
        .map_err(str::to_owned)?;
    facade.finish(accepted.operation_id);
    Ok(accepted)
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

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
