use std::fs;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use local_meeting_notes_session_core::protocol::{
    CaptureProgressState, Operation, ProtocolError, WorkerCommand,
};
use local_meeting_notes_session_core::supervision::{
    OwnedChild, SupervisionError, protocol_fixture_operations,
};
use tempfile::TempDir;

const FAKE_WORKER: &str = env!("CARGO_BIN_EXE_fake-worker");
const SUPERVISOR_HARNESS: &str = env!("CARGO_BIN_EXE_supervisor-harness");

#[test]
fn supervised_worker_keeps_protocol_after_readiness() {
    let mut command = Command::new(FAKE_WORKER);
    command.args(["--mode", "protocol"]);
    let mut child = OwnedChild::spawn(&mut command).unwrap();
    child
        .wait_ready(Duration::from_secs(1), &protocol_fixture_operations())
        .unwrap();
    let request = WorkerCommand::new(
        Operation::CaptureInspect,
        serde_json::json!({"meeting_id": "fixture"}),
    );
    let result = child
        .request_until(
            &request,
            Instant::now() + Duration::from_secs(1),
            |_| Ok(()),
        )
        .unwrap();
    assert!(result.ok);
    assert_eq!(result.artifact_digests["fixture"], "digest");
}

#[test]
fn supervised_worker_dispatches_progress_before_result() {
    let mut command = Command::new(FAKE_WORKER);
    command.args(["--mode", "protocol-progress"]);
    let mut child = OwnedChild::spawn(&mut command).unwrap();
    child
        .wait_ready(Duration::from_secs(1), &protocol_fixture_operations())
        .unwrap();
    let meeting_id = uuid::Uuid::new_v4();
    let request = WorkerCommand::new(
        Operation::CaptureStart,
        serde_json::json!({"meeting_id": meeting_id, "profile_id": "fixture"}),
    );
    let mut states = Vec::new();
    let result = child
        .request_until(
            &request,
            Instant::now() + Duration::from_secs(1),
            |progress| {
                states.push((progress.state, progress.meeting_id));
                Ok(())
            },
        )
        .unwrap();
    assert_eq!(states, vec![(CaptureProgressState::Recording, meeting_id)]);
    assert!(result.ok);
}

#[test]
fn supervised_worker_dispatches_repeatable_transcription_heartbeats() {
    let mut command = Command::new(FAKE_WORKER);
    command.args(["--mode", "transcription-heartbeats"]);
    let mut child = OwnedChild::spawn(&mut command).unwrap();
    child
        .wait_ready(Duration::from_secs(1), &protocol_fixture_operations())
        .unwrap();
    let meeting_id = uuid::Uuid::new_v4();
    let request = WorkerCommand::new(
        Operation::TranscriptCreate,
        serde_json::json!({"meeting_id": meeting_id}),
    );
    let mut states = Vec::new();
    let result = child
        .request_until(
            &request,
            Instant::now() + Duration::from_secs(1),
            |progress| {
                states.push((progress.state, progress.meeting_id));
                Ok(())
            },
        )
        .unwrap();

    assert_eq!(
        states,
        vec![
            (CaptureProgressState::Transcribing, meeting_id),
            (CaptureProgressState::Transcribing, meeting_id),
        ]
    );
    assert!(result.ok);
}

/// Runs `operation` against the progress fake worker and returns the heartbeat
/// count, the admitted measurements and the result.
fn run_transcription_with_progress(
    child: &mut OwnedChild,
    operation: Operation,
) -> (
    usize,
    Vec<(u32, u32)>,
    Result<local_meeting_notes_session_core::protocol::WorkerResult, SupervisionError>,
) {
    let meeting_id = uuid::Uuid::new_v4();
    let request = WorkerCommand::new(operation, serde_json::json!({"meeting_id": meeting_id}));
    let mut heartbeats = 0;
    let mut measured = Vec::new();
    let result = child.request_observing(
        &request,
        Instant::now() + Duration::from_secs(2),
        |_| {
            heartbeats += 1;
            Ok(())
        },
        |progress| {
            assert_eq!(progress.meeting_id, meeting_id);
            measured.push((progress.audio_seconds_done, progress.audio_seconds_total));
        },
    );
    (heartbeats, measured, result)
}

#[test]
fn transcription_progress_mid_request_never_stops_the_request_and_only_usable_values_arrive() {
    // The transcript commits only from an Ok result carrying its digests, so
    // "the request still returns its result" is what is proved here for both
    // operations the worker emits progress for.
    for operation in [Operation::TranscriptCreate, Operation::TranscriptRetry] {
        let mut command = Command::new(FAKE_WORKER);
        command.args(["--mode", "transcription-progress"]);
        let mut child = OwnedChild::spawn(&mut command).unwrap();
        child
            .wait_ready(Duration::from_secs(1), &protocol_fixture_operations())
            .unwrap();
        let (heartbeats, measured, result) = run_transcription_with_progress(&mut child, operation);
        let result = result.unwrap();
        assert!(result.ok);
        assert_eq!(result.artifact_digests["fixture"], "digest");
        assert_eq!(heartbeats, 2, "the 3 s heartbeat is unchanged");
        assert_eq!(measured, vec![(0, 100), (40, 100), (100, 100)]);
        child.stop_and_wait(Duration::from_millis(200)).unwrap();
    }
}

#[test]
fn a_progress_frame_after_the_result_is_dropped_and_the_next_request_still_completes() {
    let mut command = Command::new(FAKE_WORKER);
    command.args(["--mode", "transcription-progress"]);
    let mut child = OwnedChild::spawn(&mut command).unwrap();
    child
        .wait_ready(Duration::from_secs(1), &protocol_fixture_operations())
        .unwrap();
    let (_, measured, result) =
        run_transcription_with_progress(&mut child, Operation::TranscriptCreate);
    assert!(result.unwrap().ok);
    assert_eq!(measured, vec![(0, 100), (40, 100), (100, 100)]);

    // The progress rate bound is per second, and the worker's frames for one
    // request already use most of it; the late frame itself was sent after
    // that request's result.
    std::thread::sleep(Duration::from_millis(1100));
    let (_, measured, result) =
        run_transcription_with_progress(&mut child, Operation::TranscriptRetry);
    assert!(result.unwrap().ok);
    assert_eq!(measured, vec![(0, 100), (40, 100), (100, 100)]);
    child.stop_and_wait(Duration::from_millis(200)).unwrap();
}

#[test]
fn a_malformed_progress_frame_still_stops_and_reaps_the_worker() {
    let mut command = Command::new(FAKE_WORKER);
    command.args(["--mode", "transcription-progress-malformed"]);
    let mut child = OwnedChild::spawn(&mut command).unwrap();
    let pid = child.pid();
    child
        .wait_ready(Duration::from_secs(1), &protocol_fixture_operations())
        .unwrap();
    let (_, measured, result) =
        run_transcription_with_progress(&mut child, Operation::TranscriptCreate);
    assert!(matches!(
        result,
        Err(SupervisionError::Protocol(ProtocolError::Malformed))
    ));
    assert!(measured.is_empty());
    assert!(wait_until_gone(pid, Duration::from_secs(3)));
}

#[test]
fn a_flood_of_ignored_progress_still_trips_the_rate_bound() {
    let mut command = Command::new(FAKE_WORKER);
    command.args(["--mode", "transcription-progress-flood"]);
    let mut child = OwnedChild::spawn(&mut command).unwrap();
    let pid = child.pid();
    child
        .wait_ready(Duration::from_secs(1), &protocol_fixture_operations())
        .unwrap();
    let (_, measured, result) =
        run_transcription_with_progress(&mut child, Operation::TranscriptCreate);
    assert!(matches!(
        result,
        Err(SupervisionError::Protocol(ProtocolError::EventRateExceeded))
    ));
    assert!(measured.is_empty());
    assert!(wait_until_gone(pid, Duration::from_secs(3)));
}

#[test]
fn unknown_progress_request_stops_and_reaps_worker() {
    let mut command = Command::new(FAKE_WORKER);
    command.args(["--mode", "wrong-progress"]);
    let mut child = OwnedChild::spawn(&mut command).unwrap();
    let pid = child.pid();
    child
        .wait_ready(Duration::from_secs(1), &protocol_fixture_operations())
        .unwrap();
    let request = WorkerCommand::new(
        Operation::CaptureStart,
        serde_json::json!({
            "meeting_id": uuid::Uuid::new_v4(),
            "profile_id": "fixture"
        }),
    );
    assert!(matches!(
        child.request_until(
            &request,
            Instant::now() + Duration::from_secs(1),
            |_| Ok(())
        ),
        Err(SupervisionError::Protocol(ProtocolError::UnknownRequest))
    ));
    assert!(wait_until_gone(pid, Duration::from_secs(3)));
}

#[test]
fn request_deadline_stops_and_reaps_worker() {
    let mut command = Command::new(FAKE_WORKER);
    command.args(["--mode", "never-result"]);
    let mut child = OwnedChild::spawn(&mut command).unwrap();
    let pid = child.pid();
    child
        .wait_ready(Duration::from_secs(1), &protocol_fixture_operations())
        .unwrap();
    let request = WorkerCommand::new(
        Operation::CaptureInspect,
        serde_json::json!({"meeting_id": "fixture"}),
    );
    assert!(matches!(
        child.request_until(
            &request,
            Instant::now() + Duration::from_millis(100),
            |_| Ok(())
        ),
        Err(SupervisionError::RequestTimeout)
    ));
    assert!(wait_until_gone(pid, Duration::from_secs(1)));
}

#[test]
fn expired_deadline_writes_no_command() {
    let temp = TempDir::new().unwrap();
    let command_file = temp.path().join("command-executed");
    let mut command = Command::new(FAKE_WORKER);
    command
        .args(["--mode", "record-command"])
        .env("LMN_COMMAND_FILE", &command_file);
    let mut child = OwnedChild::spawn(&mut command).unwrap();
    child
        .wait_ready(Duration::from_secs(1), &protocol_fixture_operations())
        .unwrap();
    let request = WorkerCommand::new(
        Operation::CaptureInspect,
        serde_json::json!({"meeting_id": "fixture"}),
    );
    assert!(matches!(
        child.request_until(&request, Instant::now(), |_| Ok(())),
        Err(SupervisionError::RequestTimeout)
    ));
    std::thread::sleep(Duration::from_millis(100));
    assert!(!command_file.exists());
    child.stop_and_wait(Duration::from_millis(100)).unwrap();
}

#[test]
fn blocked_worker_stdin_obeys_deadline_and_reaps_worker() {
    let mut command = Command::new(FAKE_WORKER);
    command.args(["--mode", "never-read"]);
    let mut child = OwnedChild::spawn(&mut command).unwrap();
    let pid = child.pid();
    child
        .wait_ready(Duration::from_secs(1), &protocol_fixture_operations())
        .unwrap();
    let request = WorkerCommand::new(
        Operation::CaptureInspect,
        serde_json::json!({"meeting_id": "x".repeat(63 * 1024)}),
    );
    assert!(matches!(
        child.request_until(
            &request,
            Instant::now() + Duration::from_millis(100),
            |_| Ok(())
        ),
        Err(SupervisionError::RequestTimeout)
    ));
    assert!(wait_until_gone(pid, Duration::from_secs(1)));
}

#[test]
fn validated_result_precedes_and_survives_following_eof() {
    for _ in 0..20 {
        let mut command = Command::new(FAKE_WORKER);
        command.args(["--mode", "result-then-exit"]);
        let mut child = OwnedChild::spawn(&mut command).unwrap();
        let pid = child.pid();
        child
            .wait_ready(Duration::from_secs(1), &protocol_fixture_operations())
            .unwrap();
        let request = WorkerCommand::new(
            Operation::CaptureInspect,
            serde_json::json!({"meeting_id": "fixture"}),
        );
        let result = child
            .request_until(
                &request,
                Instant::now() + Duration::from_secs(1),
                |_| Ok(()),
            )
            .unwrap();
        assert_eq!(result.artifact_digests["fixture"], "digest");
        let deadline = Instant::now() + Duration::from_secs(2);
        while child.check_health().is_ok() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(matches!(
            child.check_health(),
            Err(SupervisionError::WorkerExited)
        ));
        assert!(wait_until_gone(pid, Duration::from_secs(1)));
    }
}

#[test]
fn queued_progress_and_result_both_precede_following_eof() {
    for _ in 0..40 {
        let mut command = Command::new(FAKE_WORKER);
        command.args(["--mode", "progress-result-then-exit"]);
        let mut child = OwnedChild::spawn(&mut command).unwrap();
        let pid = child.pid();
        child
            .wait_ready(Duration::from_secs(1), &protocol_fixture_operations())
            .unwrap();
        let meeting_id = uuid::Uuid::new_v4();
        let request = WorkerCommand::new(
            Operation::CaptureStart,
            serde_json::json!({"meeting_id": meeting_id, "profile_id": "fixture"}),
        );
        let mut progress = Vec::new();
        let result = child
            .request_until(&request, Instant::now() + Duration::from_secs(1), |event| {
                progress.push((event.state, event.meeting_id));
                Ok(())
            })
            .unwrap();
        assert_eq!(
            progress,
            vec![(CaptureProgressState::Recording, meeting_id)]
        );
        assert_eq!(result.artifact_digests["fixture"], "digest");
        let deadline = Instant::now() + Duration::from_secs(2);
        while child.check_health().is_ok() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(matches!(
            child.check_health(),
            Err(SupervisionError::WorkerExited)
        ));
        assert!(wait_until_gone(pid, Duration::from_secs(1)));
    }
}

fn process_exists(pid: u32) -> bool {
    let result = unsafe { libc::kill(pid as i32, 0) };
    result == 0 || std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
}

fn wait_until_gone(pid: u32, timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if !process_exists(pid) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    !process_exists(pid)
}

#[test]
fn missing_child_is_refused_without_spawn() {
    let mut command = Command::new("/definitely/missing/local-meeting-notes-worker");
    assert!(matches!(
        OwnedChild::spawn(&mut command),
        Err(SupervisionError::MissingChild)
    ));
}

#[test]
fn readiness_timeout_stops_and_reaps_worker() {
    let mut command = Command::new(FAKE_WORKER);
    command.args(["--mode", "never-ready"]);
    let mut child = OwnedChild::spawn(&mut command).unwrap();
    let pid = child.pid();
    assert!(matches!(
        child.wait_ready(Duration::from_millis(100), &protocol_fixture_operations()),
        Err(SupervisionError::ReadyTimeout)
    ));
    assert!(wait_until_gone(pid, Duration::from_secs(1)));
}

#[test]
fn forced_exit_escalates_and_reaps_stubborn_worker() {
    let mut command = Command::new(FAKE_WORKER);
    command.args(["--mode", "stubborn"]);
    let mut child = OwnedChild::spawn(&mut command).unwrap();
    let pid = child.pid();
    child
        .wait_ready(Duration::from_secs(1), &protocol_fixture_operations())
        .unwrap();
    child.stop_and_wait(Duration::from_millis(100)).unwrap();
    assert!(wait_until_gone(pid, Duration::from_secs(1)));
}

#[test]
fn malformed_handshake_fails_closed_and_reaps_worker() {
    let mut command = Command::new(FAKE_WORKER);
    command.args(["--mode", "malformed"]);
    let mut child = OwnedChild::spawn(&mut command).unwrap();
    let pid = child.pid();
    assert!(matches!(
        child.wait_ready(Duration::from_secs(1), &protocol_fixture_operations()),
        Err(SupervisionError::Protocol(_))
    ));
    assert!(wait_until_gone(pid, Duration::from_secs(1)));
}

#[test]
fn stderr_overflow_fails_without_deadlock_and_reaps_worker() {
    let mut command = Command::new(FAKE_WORKER);
    command.args(["--mode", "stderr-overflow"]);
    let mut child = OwnedChild::spawn(&mut command).unwrap();
    let pid = child.pid();
    let result = child.wait_ready(Duration::from_secs(1), &protocol_fixture_operations());
    assert!(matches!(result, Err(SupervisionError::StderrOverflow)));
    assert!(wait_until_gone(pid, Duration::from_secs(1)));
}

#[test]
fn worker_exit_cleanup_reaps_remaining_tap() {
    let temp = TempDir::new().unwrap();
    let pid_file = temp.path().join("children.json");
    let mut command = Command::new(FAKE_WORKER);
    command
        .args(["--mode", "ready-exit-with-tap"])
        .env("LMN_PID_FILE", &pid_file);
    let mut child = OwnedChild::spawn(&mut command).unwrap();
    child
        .wait_ready(Duration::from_secs(1), &protocol_fixture_operations())
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(1);
    while !pid_file.exists() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    let pids: serde_json::Value = serde_json::from_slice(&fs::read(&pid_file).unwrap()).unwrap();
    let worker = pids["worker"].as_u64().unwrap() as u32;
    let tap = pids["tap"].as_u64().unwrap() as u32;
    assert!(wait_until_gone(worker, Duration::from_secs(2)));
    assert!(wait_until_gone(tap, Duration::from_secs(2)));
    assert!(matches!(
        child.check_health(),
        Err(SupervisionError::WorkerExited)
    ));
}

#[test]
fn idle_protocol_and_stderr_faults_autonomously_reap_the_group() {
    for mode in ["ready-malformed-with-tap", "ready-stderr-with-tap"] {
        let temp = TempDir::new().unwrap();
        let pid_file = temp.path().join("children.json");
        let mut command = Command::new(FAKE_WORKER);
        command
            .args(["--mode", mode])
            .env("LMN_PID_FILE", &pid_file);
        let mut child = OwnedChild::spawn(&mut command).unwrap();
        child
            .wait_ready(Duration::from_secs(1), &protocol_fixture_operations())
            .unwrap();
        let pids: serde_json::Value =
            serde_json::from_slice(&fs::read(&pid_file).unwrap()).unwrap();
        let worker = pids["worker"].as_u64().unwrap() as u32;
        let tap = pids["tap"].as_u64().unwrap() as u32;
        assert!(wait_until_gone(worker, Duration::from_secs(2)));
        assert!(wait_until_gone(tap, Duration::from_secs(2)));
        match mode {
            "ready-malformed-with-tap" => assert!(matches!(
                child.check_health(),
                Err(SupervisionError::Protocol(ProtocolError::Malformed))
            )),
            "ready-stderr-with-tap" => assert!(matches!(
                child.check_health(),
                Err(SupervisionError::StderrOverflow)
            )),
            _ => unreachable!(),
        }
    }
}

#[test]
fn parent_sigkill_closes_liveness_channel_for_worker_and_tap() {
    let temp = TempDir::new().unwrap();
    let pid_file = temp.path().join("children.json");
    let mut harness = Command::new(SUPERVISOR_HARNESS)
        .arg(FAKE_WORKER)
        .arg(&pid_file)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();

    let deadline = Instant::now() + Duration::from_secs(3);
    while !pid_file.exists() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(20));
    }
    let pids: serde_json::Value = serde_json::from_slice(&fs::read(&pid_file).unwrap()).unwrap();
    let worker = pids["worker"].as_u64().unwrap() as u32;
    let tap = pids["tap"].as_u64().unwrap() as u32;

    unsafe {
        libc::kill(harness.id() as i32, libc::SIGKILL);
    }
    harness.wait().unwrap();

    assert!(wait_until_gone(worker, Duration::from_secs(3)));
    assert!(wait_until_gone(tap, Duration::from_secs(3)));
}
