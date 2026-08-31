use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum StartupState {
    ShellRendered,
    Checking,
    ModelRequired,
    Ready,
    RuntimeMissing,
    ServiceTimeout,
    DiagnosticWritten,
    Retrying,
    ReinstallRequired,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CaptureState {
    Idle,
    Arming,
    Recording,
    /// Recording is held open with no audio being acquired. The capture helper
    /// has released the microphone and the system tap, and the retained audio
    /// files contain nothing for the span this state covers. A pause is a
    /// capture-integrity event, so the span is recorded rather than hidden.
    Paused,
    Stopping,
    Captured,
    Transcribing,
    TranscriptReady,
    Summarizing,
    Ready,
    TranscriptionFailed,
    SummaryFailed,
    RecoveredInterrupted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExclusiveOperation {
    StartupRecovery,
    CaptureTransition,
    DestructiveStorage,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ReducerError {
    #[error("another exclusive operation is already active")]
    DuplicateOperation,
    #[error("invalid startup transition from {from:?} to {to:?}")]
    InvalidStartupTransition {
        from: StartupState,
        to: StartupState,
    },
    #[error("invalid capture transition from {from:?} to {to:?}")]
    InvalidCaptureTransition {
        from: CaptureState,
        to: CaptureState,
    },
}

#[derive(Debug)]
pub struct Reducer {
    startup: StartupState,
    capture: CaptureState,
    exclusive: Option<ExclusiveOperation>,
}

impl Default for Reducer {
    fn default() -> Self {
        Self {
            startup: StartupState::ShellRendered,
            capture: CaptureState::Idle,
            exclusive: None,
        }
    }
}

impl Reducer {
    pub fn startup(&self) -> StartupState {
        self.startup
    }

    pub fn capture(&self) -> CaptureState {
        self.capture
    }

    pub fn begin(&mut self, operation: ExclusiveOperation) -> Result<(), ReducerError> {
        if self.exclusive.is_some() {
            return Err(ReducerError::DuplicateOperation);
        }
        self.exclusive = Some(operation);
        Ok(())
    }

    pub fn finish(&mut self, operation: ExclusiveOperation) {
        if self.exclusive == Some(operation) {
            self.exclusive = None;
        }
    }

    pub fn transition_startup(&mut self, to: StartupState) -> Result<(), ReducerError> {
        let valid = matches!(
            (self.startup, to),
            (StartupState::ShellRendered, StartupState::Checking)
                | (StartupState::Checking, StartupState::Ready)
                | (StartupState::Checking, StartupState::ModelRequired)
                | (StartupState::Checking, StartupState::RuntimeMissing)
                | (StartupState::Checking, StartupState::ServiceTimeout)
                | (StartupState::Checking, StartupState::DiagnosticWritten)
                | (StartupState::RuntimeMissing, StartupState::Retrying)
                | (StartupState::ServiceTimeout, StartupState::Retrying)
                | (StartupState::DiagnosticWritten, StartupState::Retrying)
                | (StartupState::ModelRequired, StartupState::Retrying)
                | (StartupState::Ready, StartupState::Retrying)
                | (StartupState::Ready, StartupState::DiagnosticWritten)
                | (StartupState::Retrying, StartupState::Ready)
                | (StartupState::Retrying, StartupState::ModelRequired)
                | (StartupState::Retrying, StartupState::RuntimeMissing)
                | (StartupState::Retrying, StartupState::ServiceTimeout)
                | (StartupState::Retrying, StartupState::DiagnosticWritten)
                | (StartupState::Retrying, StartupState::ReinstallRequired)
        );
        if !valid {
            return Err(ReducerError::InvalidStartupTransition {
                from: self.startup,
                to,
            });
        }
        self.startup = to;
        Ok(())
    }

    pub fn transition_capture(&mut self, to: CaptureState) -> Result<(), ReducerError> {
        let valid = matches!(
            (self.capture, to),
            (CaptureState::Idle, CaptureState::Arming)
                | (CaptureState::Arming, CaptureState::Recording)
                | (CaptureState::Arming, CaptureState::RecoveredInterrupted)
                | (CaptureState::Recording, CaptureState::Stopping)
                | (CaptureState::Recording, CaptureState::RecoveredInterrupted)
                | (CaptureState::Recording, CaptureState::Paused)
                | (CaptureState::Paused, CaptureState::Recording)
                // Ending the meeting while paused is ordinary operator
                // behavior, not an error path: the audio already captured is
                // still a real take and is finalized the same way.
                | (CaptureState::Paused, CaptureState::Stopping)
                | (CaptureState::Paused, CaptureState::RecoveredInterrupted)
                | (CaptureState::Stopping, CaptureState::Captured)
                | (CaptureState::Captured, CaptureState::Idle)
                | (CaptureState::Stopping, CaptureState::RecoveredInterrupted)
                | (CaptureState::Captured, CaptureState::Transcribing)
                | (CaptureState::Transcribing, CaptureState::Summarizing)
                | (
                    CaptureState::Transcribing,
                    CaptureState::TranscriptionFailed
                )
                | (CaptureState::Transcribing, CaptureState::TranscriptReady)
                | (
                    CaptureState::TranscriptionFailed,
                    CaptureState::Transcribing
                )
                | (CaptureState::TranscriptionFailed, CaptureState::Idle)
                | (CaptureState::TranscriptReady, CaptureState::Idle)
                | (CaptureState::Summarizing, CaptureState::Ready)
                | (CaptureState::Summarizing, CaptureState::SummaryFailed)
                | (CaptureState::SummaryFailed, CaptureState::Summarizing)
                | (CaptureState::Ready, CaptureState::Idle)
                | (CaptureState::RecoveredInterrupted, CaptureState::Idle)
        );
        if !valid {
            return Err(ReducerError::InvalidCaptureTransition {
                from: self.capture,
                to,
            });
        }
        self.capture = to;
        Ok(())
    }

    pub fn restore_capture_projection(&mut self, to: CaptureState) -> Result<(), ReducerError> {
        let valid = self.exclusive.is_none()
            && matches!(
                self.startup,
                StartupState::Checking | StartupState::Retrying
            )
            && self.capture == CaptureState::Idle
            && to == CaptureState::TranscriptReady;
        if !valid {
            return Err(ReducerError::InvalidCaptureTransition {
                from: self.capture,
                to,
            });
        }
        self.capture = to;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refuses_duplicate_exclusive_operation() {
        let mut reducer = Reducer::default();
        reducer
            .begin(ExclusiveOperation::CaptureTransition)
            .unwrap();
        assert_eq!(
            reducer.begin(ExclusiveOperation::StartupRecovery),
            Err(ReducerError::DuplicateOperation)
        );
    }

    #[test]
    fn rejected_summary_never_reaches_ready_directly() {
        let mut reducer = Reducer {
            capture: CaptureState::SummaryFailed,
            ..Reducer::default()
        };
        assert!(reducer.transition_capture(CaptureState::Ready).is_err());
        reducer
            .transition_capture(CaptureState::Summarizing)
            .unwrap();
        reducer.transition_capture(CaptureState::Ready).unwrap();
    }

    #[test]
    fn ready_runtime_can_restart_after_a_model_change() {
        let mut reducer = Reducer::default();
        reducer.transition_startup(StartupState::Checking).unwrap();
        reducer.transition_startup(StartupState::Ready).unwrap();
        reducer.transition_startup(StartupState::Retrying).unwrap();
        reducer.transition_startup(StartupState::Ready).unwrap();
    }

    #[test]
    fn startup_restoration_allows_only_a_transcript_projection() {
        let mut reducer = Reducer::default();
        reducer.transition_startup(StartupState::Checking).unwrap();
        reducer
            .restore_capture_projection(CaptureState::TranscriptReady)
            .unwrap();
        assert_eq!(reducer.capture(), CaptureState::TranscriptReady);

        let mut idle = Reducer::default();
        idle.transition_startup(StartupState::Checking).unwrap();
        assert!(idle
            .restore_capture_projection(CaptureState::Ready)
            .is_err());

        let mut shell = Reducer::default();
        assert!(shell
            .restore_capture_projection(CaptureState::TranscriptReady)
            .is_err());
    }

    #[test]
    fn transcript_alpha_returns_to_idle_without_claiming_a_summary() {
        let mut reducer = Reducer::default();
        for state in [
            CaptureState::Arming,
            CaptureState::Recording,
            CaptureState::Stopping,
            CaptureState::Captured,
            CaptureState::Transcribing,
            CaptureState::TranscriptReady,
            CaptureState::Idle,
        ] {
            reducer.transition_capture(state).unwrap();
        }
    }

    #[test]
    fn a_recording_may_pause_and_resume_without_ending_the_session() {
        let mut reducer = Reducer::default();
        reducer.transition_capture(CaptureState::Arming).unwrap();
        reducer.transition_capture(CaptureState::Recording).unwrap();
        reducer.transition_capture(CaptureState::Paused).unwrap();
        assert_eq!(reducer.capture(), CaptureState::Paused);
        reducer.transition_capture(CaptureState::Recording).unwrap();
        reducer.transition_capture(CaptureState::Paused).unwrap();
        reducer.transition_capture(CaptureState::Recording).unwrap();
        reducer.transition_capture(CaptureState::Stopping).unwrap();
        reducer.transition_capture(CaptureState::Captured).unwrap();
    }

    #[test]
    fn a_paused_meeting_may_be_stopped_or_lost_without_resuming_first() {
        let mut stopped = Reducer {
            capture: CaptureState::Paused,
            ..Reducer::default()
        };
        stopped.transition_capture(CaptureState::Stopping).unwrap();
        stopped.transition_capture(CaptureState::Captured).unwrap();

        let mut interrupted = Reducer {
            capture: CaptureState::Paused,
            ..Reducer::default()
        };
        interrupted
            .transition_capture(CaptureState::RecoveredInterrupted)
            .unwrap();
    }

    #[test]
    fn pausing_is_reachable_only_from_an_established_recording() {
        for from in [
            CaptureState::Idle,
            CaptureState::Arming,
            CaptureState::Stopping,
            CaptureState::Captured,
            CaptureState::Transcribing,
            CaptureState::TranscriptReady,
            CaptureState::Summarizing,
            CaptureState::Ready,
            CaptureState::TranscriptionFailed,
            CaptureState::SummaryFailed,
            CaptureState::RecoveredInterrupted,
        ] {
            let mut reducer = Reducer {
                capture: from,
                ..Reducer::default()
            };
            assert_eq!(
                reducer.transition_capture(CaptureState::Paused),
                Err(ReducerError::InvalidCaptureTransition {
                    from,
                    to: CaptureState::Paused
                }),
                "pausing must be refused from {from:?}"
            );
        }
    }

    #[test]
    fn a_paused_capture_never_skips_the_stopping_and_captured_steps() {
        for to in [
            CaptureState::Idle,
            CaptureState::Arming,
            CaptureState::Captured,
            CaptureState::Transcribing,
            CaptureState::TranscriptReady,
            CaptureState::Summarizing,
            CaptureState::Ready,
            CaptureState::TranscriptionFailed,
            CaptureState::SummaryFailed,
        ] {
            let mut reducer = Reducer {
                capture: CaptureState::Paused,
                ..Reducer::default()
            };
            assert!(
                reducer.transition_capture(to).is_err(),
                "a paused capture must not reach {to:?} directly"
            );
        }
    }

    #[test]
    fn a_paused_state_never_restores_from_startup() {
        let mut reducer = Reducer::default();
        reducer.transition_startup(StartupState::Checking).unwrap();
        assert!(reducer
            .restore_capture_projection(CaptureState::Paused)
            .is_err());
    }

    #[test]
    fn failed_transcript_may_be_dismissed_without_discarding_its_capture() {
        let mut reducer = Reducer::default();
        for state in [
            CaptureState::Arming,
            CaptureState::Recording,
            CaptureState::Stopping,
            CaptureState::Captured,
            CaptureState::Transcribing,
            CaptureState::TranscriptionFailed,
            CaptureState::Idle,
        ] {
            reducer.transition_capture(state).unwrap();
        }
    }
}
