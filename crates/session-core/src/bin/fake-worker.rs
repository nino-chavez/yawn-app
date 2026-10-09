use std::env;
use std::fs;
use std::io::{self, BufRead, Write};
use std::process::{Command, Stdio};
use std::time::Duration;

use local_meeting_notes_session_core::supervision::{PARENT_FD_ENV, read_parent_liveness};
use uuid::Uuid;

fn parent_fd() -> i32 {
    env::var(PARENT_FD_ENV)
        .expect("parent liveness descriptor")
        .parse()
        .expect("numeric parent liveness descriptor")
}

fn ready() {
    println!(
        "{}",
        serde_json::json!({
            "schema": "worker-event/2",
            "event": "worker.ready",
            "protocol": 2,
            "admission": "boundary-test",
            "build": "fake-worker-build",
            "runtime": {"kind": "bundled", "digest": "fake-runtime"},
            "tap": {"build": "fake-tap", "available": true},
            "models": [{"id": "fake-model", "digest": "fake-model-digest", "available": true}],
            "operations": [
                "capture.start", "capture.finalize", "capture.inspect", "transcript.create"
            ]
        })
    );
    io::stdout().flush().unwrap();
}

#[allow(clippy::zombie_processes)]
fn spawn_tap_for_orphan_cleanup_test(mode: &str) -> u32 {
    let executable = env::current_exe().unwrap();
    Command::new(executable)
        .args(["--mode", mode])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap()
        .id()
}

// Tests poll for this file to appear and parse it at once, so it must never be
// visible empty: write a sibling temp file, then rename it into place.
fn write_pid_receipt(worker: u32, tap: u32) {
    if let Ok(path) = env::var("LMN_PID_FILE") {
        let staging = format!("{path}.tmp");
        fs::write(&staging, format!("{{\"worker\":{worker},\"tap\":{tap}}}\n")).unwrap();
        fs::rename(staging, path).unwrap();
    }
}

fn spawn_stubborn_tap_with_receipt() -> u32 {
    let tap_pid = spawn_tap_for_orphan_cleanup_test("tap-stubborn");
    write_pid_receipt(std::process::id(), tap_pid);
    tap_pid
}

fn main() {
    let mode = env::args().nth(2).unwrap_or_else(|| "ready".into());
    match mode.as_str() {
        "exit" => std::process::exit(42),
        "never-ready" => read_parent_liveness(parent_fd()).unwrap(),
        "malformed" => {
            println!("{{}}");
            io::stdout().flush().unwrap();
            read_parent_liveness(parent_fd()).unwrap();
        }
        "ready" => {
            ready();
            read_parent_liveness(parent_fd()).unwrap();
        }
        "protocol" => {
            ready();
            for line in io::stdin().lock().lines() {
                let command: serde_json::Value = serde_json::from_str(&line.unwrap()).unwrap();
                println!(
                    "{}",
                    serde_json::json!({
                        "schema": "worker-result/2",
                        "request_id": command["request_id"],
                        "ok": true,
                        "code": null,
                        "recoverable": null,
                        "artifact_digests": {"fixture": "digest"}
                    })
                );
                io::stdout().flush().unwrap();
            }
        }
        "protocol-progress" => {
            ready();
            for line in io::stdin().lock().lines() {
                let command: serde_json::Value = serde_json::from_str(&line.unwrap()).unwrap();
                println!(
                    "{}",
                    serde_json::json!({
                        "schema": "worker-event/2",
                        "request_id": command["request_id"],
                        "event": "capture.state",
                        "state": "recording",
                        "meeting_id": command["arguments"]["meeting_id"]
                    })
                );
                println!(
                    "{}",
                    serde_json::json!({
                        "schema": "worker-result/2",
                        "request_id": command["request_id"],
                        "ok": true,
                        "code": null,
                        "recoverable": null,
                        "artifact_digests": {"fixture": "digest"}
                    })
                );
                io::stdout().flush().unwrap();
            }
        }
        "transcription-heartbeats" => {
            ready();
            for line in io::stdin().lock().lines() {
                let command: serde_json::Value = serde_json::from_str(&line.unwrap()).unwrap();
                for _ in 0..2 {
                    println!(
                        "{}",
                        serde_json::json!({
                            "schema": "worker-event/2",
                            "request_id": command["request_id"],
                            "event": "capture.state",
                            "state": "transcribing",
                            "meeting_id": command["arguments"]["meeting_id"]
                        })
                    );
                }
                println!(
                    "{}",
                    serde_json::json!({
                        "schema": "worker-result/2",
                        "request_id": command["request_id"],
                        "ok": true,
                        "code": null,
                        "recoverable": null,
                        "artifact_digests": {"fixture": "digest"}
                    })
                );
                io::stdout().flush().unwrap();
            }
        }
        "wrong-progress" => {
            ready();
            let line = io::stdin().lock().lines().next().unwrap().unwrap();
            let command: serde_json::Value = serde_json::from_str(&line).unwrap();
            println!(
                "{}",
                serde_json::json!({
                    "schema": "worker-event/2",
                    "request_id": Uuid::new_v4(),
                    "event": "capture.state",
                    "state": "recording",
                    "meeting_id": command["arguments"]["meeting_id"]
                })
            );
            io::stdout().flush().unwrap();
            read_parent_liveness(parent_fd()).unwrap();
        }
        "never-result" => {
            ready();
            let _ = io::stdin().lock().lines().next();
            read_parent_liveness(parent_fd()).unwrap();
        }
        "never-read" => {
            ready();
            read_parent_liveness(parent_fd()).unwrap();
        }
        "record-command" => {
            ready();
            if io::stdin().lock().lines().next().is_some()
                && let Ok(path) = env::var("LMN_COMMAND_FILE")
            {
                fs::write(path, b"executed\n").unwrap();
            }
            read_parent_liveness(parent_fd()).unwrap();
        }
        "result-then-exit" => {
            ready();
            let line = io::stdin().lock().lines().next().unwrap().unwrap();
            let command: serde_json::Value = serde_json::from_str(&line).unwrap();
            println!(
                "{}",
                serde_json::json!({
                    "schema": "worker-result/2",
                    "request_id": command["request_id"],
                    "ok": true,
                    "code": null,
                    "recoverable": null,
                    "artifact_digests": {"fixture": "digest"}
                })
            );
            io::stdout().flush().unwrap();
        }
        "progress-result-then-exit" => {
            ready();
            let line = io::stdin().lock().lines().next().unwrap().unwrap();
            let command: serde_json::Value = serde_json::from_str(&line).unwrap();
            println!(
                "{}",
                serde_json::json!({
                    "schema": "worker-event/2",
                    "request_id": command["request_id"],
                    "event": "capture.state",
                    "state": "recording",
                    "meeting_id": command["arguments"]["meeting_id"]
                })
            );
            println!(
                "{}",
                serde_json::json!({
                    "schema": "worker-result/2",
                    "request_id": command["request_id"],
                    "ok": true,
                    "code": null,
                    "recoverable": null,
                    "artifact_digests": {"fixture": "digest"}
                })
            );
            io::stdout().flush().unwrap();
        }
        "stderr-overflow" => {
            io::stderr().write_all(&vec![b'x'; 16 * 1024 + 1]).unwrap();
            io::stderr().flush().unwrap();
            read_parent_liveness(parent_fd()).unwrap();
        }
        "ready-malformed-with-tap" => {
            let _ = spawn_stubborn_tap_with_receipt();
            ready();
            std::thread::sleep(Duration::from_millis(50));
            println!("{{}}");
            io::stdout().flush().unwrap();
            read_parent_liveness(parent_fd()).unwrap();
        }
        "ready-stderr-with-tap" => {
            let _ = spawn_stubborn_tap_with_receipt();
            ready();
            std::thread::sleep(Duration::from_millis(50));
            io::stderr().write_all(&vec![b'x'; 16 * 1024 + 1]).unwrap();
            io::stderr().flush().unwrap();
            read_parent_liveness(parent_fd()).unwrap();
        }
        "stubborn" => {
            unsafe {
                libc::signal(libc::SIGTERM, libc::SIG_IGN);
            }
            ready();
            loop {
                std::thread::sleep(Duration::from_secs(1));
            }
        }
        "ready-with-tap" => {
            let executable = env::current_exe().unwrap();
            let mut tap = Command::new(executable)
                .args(["--mode", "tap"])
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .unwrap();
            write_pid_receipt(std::process::id(), tap.id());
            ready();
            read_parent_liveness(parent_fd()).unwrap();
            let _ = tap.wait();
        }
        "ready-exit-with-tap" => {
            // This mode deliberately exits without waiting so the supervisor
            // must prove it cleans up the remaining process-group member.
            let _ = spawn_stubborn_tap_with_receipt();
            ready();
        }
        "tap" => read_parent_liveness(parent_fd()).unwrap(),
        "tap-stubborn" => {
            unsafe {
                libc::signal(libc::SIGTERM, libc::SIG_IGN);
            }
            loop {
                std::thread::sleep(Duration::from_secs(1));
            }
        }
        other => panic!("unknown fake mode {other}"),
    }
}
