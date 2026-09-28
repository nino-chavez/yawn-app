use std::collections::HashSet;
use std::fs::File;
use std::io::{self, BufRead, BufReader, Read, Write};
use std::os::fd::{AsRawFd, FromRawFd, RawFd};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStderr, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, mpsc};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sysinfo::{Pid, ProcessesToUpdate, System};
use thiserror::Error;

use crate::protocol::{
    MAX_FRAME_BYTES, MAX_PROGRESS_EVENTS_PER_SECOND, MAX_QUEUED_OUTPUTS, Operation, ProtocolError,
    RequestTracker, WorkerCommand, WorkerOutput, WorkerProgress, WorkerReady, WorkerResult,
    parse_output, parse_ready,
};

pub const PARENT_FD_ENV: &str = "LMN_PARENT_LIVENESS_FD";
pub const MAX_STDERR_BYTES: usize = 16 * 1024;

#[derive(Debug, Error)]
pub enum SupervisionError {
    #[error("worker executable is missing")]
    MissingChild,
    #[error("worker readiness timed out")]
    ReadyTimeout,
    #[error("worker exited before readiness")]
    EarlyExit,
    #[error("worker exited before completing its request")]
    WorkerExited,
    #[error("worker request exceeded its deadline")]
    RequestTimeout,
    #[error("worker standard error exceeded its byte limit")]
    StderrOverflow,
    #[error("worker cleanup failed: {0}")]
    CleanupFailed(#[source] io::Error),
    #[error("worker protocol is not ready or is no longer usable")]
    Unavailable,
    #[error(transparent)]
    Protocol(#[from] ProtocolError),
    #[error(transparent)]
    Io(#[from] io::Error),
}

#[derive(Debug, Clone, Copy)]
enum WorkerFault {
    Protocol(ProtocolError),
    WorkerExited,
    StdoutIo,
    StderrIo,
    StderrOverflow,
}

#[derive(Debug, Clone, Copy)]
struct WorkerFaultRecord {
    fault: WorkerFault,
    occurred_at: Instant,
}

#[derive(Debug, Clone, Copy)]
enum CleanupMode {
    Graceful,
    Abort,
}

#[derive(Debug, Clone)]
struct CleanupFailure {
    kind: io::ErrorKind,
    message: String,
}

impl CleanupFailure {
    fn from_error(error: &io::Error) -> Self {
        Self {
            kind: error.kind(),
            message: error.to_string(),
        }
    }

    fn into_error(self) -> io::Error {
        io::Error::new(self.kind, self.message)
    }
}

#[derive(Default)]
struct CleanupState {
    finished: bool,
    failure: Option<CleanupFailure>,
}

struct ProcessControl {
    pid: u32,
    process_group_id: i32,
    child: Mutex<Child>,
    stdin: Mutex<Option<ChildStdin>>,
    liveness_writer: Mutex<Option<File>>,
    cleanup: Mutex<CleanupState>,
    cleanup_finished: Condvar,
    shutting_down: AtomicBool,
    #[cfg(test)]
    cleanup_failure_injection: Mutex<Option<CleanupFailure>>,
}

struct DispatchItem {
    received_at: Instant,
    output: WorkerOutput,
}

pub struct OwnedChild {
    control: Arc<ProcessControl>,
    readiness_stdout: Option<ChildStdout>,
    output_receiver: Option<mpsc::Receiver<DispatchItem>>,
    dispatcher_thread: Option<JoinHandle<()>>,
    stderr_thread: Option<JoinHandle<()>>,
    requests: Arc<Mutex<RequestTracker>>,
    fault: Arc<Mutex<Option<WorkerFaultRecord>>>,
    stopped: bool,
}

impl OwnedChild {
    pub fn spawn(command: &mut Command) -> Result<Self, SupervisionError> {
        let program = PathBuf::from(command.get_program());
        if !program.is_file() {
            return Err(SupervisionError::MissingChild);
        }
        let (read_fd, write_fd) = create_liveness_pipe()?;
        command
            .env(PARENT_FD_ENV, read_fd.to_string())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        unsafe {
            command.pre_exec(move || {
                if libc::setpgid(0, 0) != 0 {
                    return Err(io::Error::last_os_error());
                }
                Ok(())
            });
        }
        let mut child = match command.spawn() {
            Ok(child) => child,
            Err(error) => {
                unsafe {
                    libc::close(read_fd);
                    libc::close(write_fd);
                }
                return Err(error.into());
            }
        };
        unsafe {
            libc::close(read_fd);
        }
        let liveness_writer = unsafe { File::from_raw_fd(write_fd) };
        let process_group_id = child.id() as i32;
        let pid = child.id();
        let fault = Arc::new(Mutex::new(None));
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| io::Error::other("worker stdin missing"))?;
        if let Err(error) = set_nonblocking(&stdin) {
            let _ = signal_group(process_group_id, libc::SIGKILL);
            let _ = child.wait();
            return Err(error.into());
        }
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| io::Error::other("worker stdout missing"))?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| io::Error::other("worker stderr missing"))?;
        let control = Arc::new(ProcessControl {
            pid,
            process_group_id,
            child: Mutex::new(child),
            stdin: Mutex::new(Some(stdin)),
            liveness_writer: Mutex::new(Some(liveness_writer)),
            cleanup: Mutex::new(CleanupState::default()),
            cleanup_finished: Condvar::new(),
            shutting_down: AtomicBool::new(false),
            #[cfg(test)]
            cleanup_failure_injection: Mutex::new(None),
        });
        let stderr_thread = Some(spawn_stderr_monitor(
            stderr,
            Arc::clone(&fault),
            Arc::clone(&control),
        ));
        Ok(Self {
            control,
            readiness_stdout: Some(stdout),
            output_receiver: None,
            dispatcher_thread: None,
            stderr_thread,
            requests: Arc::new(Mutex::new(RequestTracker::default())),
            fault,
            stopped: false,
        })
    }

    pub fn pid(&self) -> u32 {
        self.control.pid
    }

    pub fn process_group_id(&self) -> i32 {
        self.control.process_group_id
    }

    pub fn check_health(&self) -> Result<(), SupervisionError> {
        if let Some(error) = self.control.cleanup_failure_error() {
            return Err(error);
        }
        if self.stopped || self.output_receiver.is_none() {
            return Err(SupervisionError::Unavailable);
        }
        if let Some(error) = self.current_fault_error() {
            return Err(error);
        }
        if self.control.shutting_down.load(Ordering::SeqCst) {
            return Err(SupervisionError::Unavailable);
        }
        Ok(())
    }

    pub fn wait_ready(
        &mut self,
        timeout: Duration,
        expected_operations: &HashSet<Operation>,
    ) -> Result<WorkerReady, SupervisionError> {
        if self.output_receiver.is_some() || self.stopped {
            return Err(SupervisionError::Unavailable);
        }
        let stdout = self
            .readiness_stdout
            .take()
            .ok_or_else(|| io::Error::other("worker stdout missing"))?;
        let (sender, receiver) = mpsc::channel();
        let ready_thread = std::thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            let result = read_bounded_frame(&mut reader);
            let _ = sender.send((result, reader));
        });
        let received = receiver.recv_timeout(timeout);
        match received {
            Ok((Ok(frame), reader)) if !frame.is_empty() => {
                let _ = ready_thread.join();
                if let Some(error) = self.current_fault_error() {
                    return Err(self.abort_error(error));
                }
                match parse_ready(&frame, expected_operations) {
                    Ok(ready) => {
                        self.start_dispatcher(reader);
                        Ok(ready)
                    }
                    Err(error) => Err(self.abort_error(error.into())),
                }
            }
            Ok((Ok(_), _)) => {
                let _ = ready_thread.join();
                let error = self
                    .current_fault_error()
                    .unwrap_or(SupervisionError::EarlyExit);
                Err(self.abort_error(error))
            }
            Ok((Err(read_error), _)) => {
                let _ = ready_thread.join();
                let error = self.current_fault_error().unwrap_or(read_error);
                Err(self.abort_error(error))
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                let error = self.abort_error(SupervisionError::ReadyTimeout);
                let _ = ready_thread.join();
                Err(error)
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                let _ = ready_thread.join();
                let error = self
                    .current_fault_error()
                    .unwrap_or(SupervisionError::EarlyExit);
                Err(self.abort_error(error))
            }
        }
    }

    pub fn request_until<F>(
        &mut self,
        command: &WorkerCommand,
        deadline: Instant,
        mut on_progress: F,
    ) -> Result<WorkerResult, SupervisionError>
    where
        F: FnMut(&WorkerProgress) -> Result<(), ProtocolError>,
    {
        if self.stopped || self.output_receiver.is_none() {
            return Err(SupervisionError::Unavailable);
        }
        if let Some(error) = self.control.cleanup_failure_error() {
            return Err(error);
        }
        if let Some(error) = self.current_fault_error() {
            return Err(self.abort_error(error));
        }
        if self.control.shutting_down.load(Ordering::SeqCst) {
            return Err(SupervisionError::Unavailable);
        }
        if Instant::now() >= deadline {
            return Err(SupervisionError::RequestTimeout);
        }
        let mut frame = serde_json::to_vec(command).map_err(|_| ProtocolError::Malformed)?;
        frame.push(b'\n');
        if frame.len() > MAX_FRAME_BYTES {
            return Err(ProtocolError::FrameTooLarge.into());
        }
        self.requests
            .lock()
            .expect("request tracker lock")
            .register(command)?;
        if let Err(error) = self.control.write_until(&frame, deadline) {
            self.cancel_request(command.request_id);
            let error = match error {
                DeadlineWriteError::Timeout => SupervisionError::RequestTimeout,
                DeadlineWriteError::Io(error) => SupervisionError::Io(error),
            };
            return Err(self.abort_error(error));
        }

        loop {
            let now = Instant::now();
            let receiver = self
                .output_receiver
                .as_ref()
                .expect("output receiver checked");
            let received = match receiver.try_recv() {
                Ok(item) => Ok(item),
                Err(mpsc::TryRecvError::Disconnected) => Err(mpsc::RecvTimeoutError::Disconnected),
                Err(mpsc::TryRecvError::Empty) if self.current_fault_record().is_some() => {
                    match receiver.try_recv() {
                        Ok(item) => Ok(item),
                        Err(mpsc::TryRecvError::Empty | mpsc::TryRecvError::Disconnected) => {
                            Err(mpsc::RecvTimeoutError::Disconnected)
                        }
                    }
                }
                Err(mpsc::TryRecvError::Empty) if now >= deadline => {
                    Err(mpsc::RecvTimeoutError::Timeout)
                }
                Err(mpsc::TryRecvError::Empty) => {
                    receiver.recv_timeout(deadline.saturating_duration_since(now))
                }
            };
            let item = match received {
                Ok(item) => item,
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    self.cancel_request(command.request_id);
                    return Err(self.abort_error(SupervisionError::RequestTimeout));
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    let error = self
                        .current_fault_error()
                        .unwrap_or(SupervisionError::WorkerExited);
                    self.cancel_request(command.request_id);
                    return Err(self.abort_error(error));
                }
            };
            if item.received_at > deadline {
                self.cancel_request(command.request_id);
                return Err(self.abort_error(SupervisionError::RequestTimeout));
            }
            match item.output {
                WorkerOutput::Progress(progress) => {
                    if let Some(fault) = self.current_fault_record()
                        && !queued_output_precedes_eof(fault, item.received_at)
                    {
                        let error = worker_fault_error(fault.fault);
                        self.cancel_request(command.request_id);
                        return Err(self.abort_error(error));
                    }
                    if let Err(error) = on_progress(&progress) {
                        self.cancel_request(command.request_id);
                        return Err(self.abort_error(error.into()));
                    }
                }
                WorkerOutput::Result(result) => {
                    if let Some(fault) = self.current_fault_record()
                        && !queued_output_precedes_eof(fault, item.received_at)
                    {
                        let error = worker_fault_error(fault.fault);
                        return Err(self.abort_error(error));
                    }
                    return Ok(result);
                }
            }
        }
    }

    pub fn shutdown_and_wait(&mut self, grace: Duration) -> io::Result<()> {
        self.stop_owned_group(grace, false)
    }

    pub fn stop_and_wait(&mut self, grace: Duration) -> io::Result<()> {
        self.abort_and_wait(grace)
    }

    fn abort_and_wait(&mut self, grace: Duration) -> io::Result<()> {
        self.stop_owned_group(grace, true)
    }

    fn abort_error(&mut self, primary: SupervisionError) -> SupervisionError {
        match self.abort_and_wait(Duration::from_millis(500)) {
            Ok(()) => primary,
            Err(error) => SupervisionError::CleanupFailed(error),
        }
    }

    fn stop_owned_group(&mut self, grace: Duration, abort: bool) -> io::Result<()> {
        if self.stopped {
            return Ok(());
        }
        self.control.begin_cleanup(
            if abort {
                CleanupMode::Abort
            } else {
                CleanupMode::Graceful
            },
            grace,
        );
        self.control.wait_for_cleanup()?;
        if let Err(error) = self.finish_stopped() {
            self.control.record_cleanup_failure(&error);
            return Err(error);
        }
        Ok(())
    }

    fn finish_stopped(&mut self) -> io::Result<()> {
        self.output_receiver.take();
        join_thread(self.dispatcher_thread.take())?;
        join_thread(self.stderr_thread.take())?;
        self.stopped = true;
        Ok(())
    }

    fn start_dispatcher(&mut self, reader: BufReader<ChildStdout>) {
        let (sender, receiver) = mpsc::sync_channel(MAX_QUEUED_OUTPUTS);
        self.output_receiver = Some(receiver);
        self.dispatcher_thread = Some(spawn_dispatcher(
            reader,
            sender,
            Arc::clone(&self.requests),
            Arc::clone(&self.fault),
            Arc::clone(&self.control),
        ));
    }

    fn cancel_request(&self, request_id: uuid::Uuid) {
        self.requests
            .lock()
            .expect("request tracker lock")
            .cancel(request_id);
    }

    fn current_fault_error(&self) -> Option<SupervisionError> {
        self.current_fault_record()
            .map(|record| worker_fault_error(record.fault))
    }

    fn current_fault_record(&self) -> Option<WorkerFaultRecord> {
        self.fault
            .lock()
            .expect("worker fault lock")
            .as_ref()
            .copied()
    }
}

#[derive(Debug)]
enum DeadlineWriteError {
    Timeout,
    Io(io::Error),
}

impl ProcessControl {
    fn write_until(&self, frame: &[u8], deadline: Instant) -> Result<(), DeadlineWriteError> {
        if Instant::now() >= deadline {
            return Err(DeadlineWriteError::Timeout);
        }
        if self.shutting_down.load(Ordering::SeqCst) {
            return Err(DeadlineWriteError::Io(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "worker is shutting down",
            )));
        }
        let mut stdin = self.stdin.lock().expect("worker stdin lock");
        let stdin = stdin.as_mut().ok_or_else(|| {
            DeadlineWriteError::Io(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "worker stdin is closed",
            ))
        })?;
        write_all_until(stdin, frame, deadline, &self.shutting_down)
    }

    fn begin_cleanup(self: &Arc<Self>, mode: CleanupMode, grace: Duration) {
        if self.shutting_down.swap(true, Ordering::SeqCst) {
            return;
        }
        let control = Arc::clone(self);
        std::thread::spawn(move || {
            let failure = control
                .cleanup_process_group(mode, grace)
                .err()
                .map(|error| CleanupFailure::from_error(&error));
            let mut state = control.cleanup.lock().expect("cleanup state lock");
            state.failure = failure;
            state.finished = true;
            control.cleanup_finished.notify_all();
        });
    }

    fn wait_for_cleanup(&self) -> io::Result<()> {
        let mut state = self.cleanup.lock().expect("cleanup state lock");
        while !state.finished {
            state = self
                .cleanup_finished
                .wait(state)
                .expect("cleanup state lock");
        }
        match state.failure.clone() {
            Some(failure) => Err(failure.into_error()),
            None => Ok(()),
        }
    }

    fn cleanup_failure_error(&self) -> Option<SupervisionError> {
        self.cleanup
            .lock()
            .expect("cleanup state lock")
            .failure
            .clone()
            .map(|failure| SupervisionError::CleanupFailed(failure.into_error()))
    }

    fn record_cleanup_failure(&self, error: &io::Error) {
        let mut state = self.cleanup.lock().expect("cleanup state lock");
        if state.failure.is_none() {
            state.failure = Some(CleanupFailure::from_error(error));
        }
    }

    fn cleanup_process_group(&self, mode: CleanupMode, grace: Duration) -> io::Result<()> {
        let result = self.cleanup_process_group_inner(mode, grace);
        #[cfg(test)]
        if result.is_ok()
            && let Some(failure) = self
                .cleanup_failure_injection
                .lock()
                .expect("cleanup failure injection lock")
                .clone()
        {
            return Err(failure.into_error());
        }
        result
    }

    fn cleanup_process_group_inner(&self, mode: CleanupMode, grace: Duration) -> io::Result<()> {
        let mut terminate_error = if matches!(mode, CleanupMode::Abort) {
            signal_group(self.process_group_id, libc::SIGTERM).err()
        } else {
            None
        };
        self.stdin.lock().expect("worker stdin lock").take();
        self.liveness_writer
            .lock()
            .expect("liveness writer lock")
            .take();
        let mut child = self.child.lock().expect("worker child lock");
        if matches!(mode, CleanupMode::Graceful)
            && wait_for_group_exit(&mut child, self.process_group_id, grace)?
        {
            return finish_child(&mut child);
        }
        if matches!(mode, CleanupMode::Graceful) {
            terminate_error = signal_group(self.process_group_id, libc::SIGTERM).err();
        }
        if wait_for_group_exit(&mut child, self.process_group_id, grace)? {
            return finish_child(&mut child);
        }
        let kill_error = signal_group(self.process_group_id, libc::SIGKILL).err();
        if wait_for_group_exit(
            &mut child,
            self.process_group_id,
            grace.max(Duration::from_millis(250)),
        )? {
            return finish_child(&mut child);
        }
        Err(kill_error.or(terminate_error).unwrap_or_else(|| {
            io::Error::new(
                io::ErrorKind::TimedOut,
                "owned process group did not exit after SIGKILL",
            )
        }))
    }

    #[cfg(test)]
    fn inject_cleanup_failure(&self, error: &io::Error) {
        *self
            .cleanup_failure_injection
            .lock()
            .expect("cleanup failure injection lock") = Some(CleanupFailure::from_error(error));
    }
}

fn set_nonblocking(stdin: &ChildStdin) -> io::Result<()> {
    let descriptor = stdin.as_raw_fd();
    let flags = unsafe { libc::fcntl(descriptor, libc::F_GETFL) };
    if flags == -1
        || unsafe { libc::fcntl(descriptor, libc::F_SETFL, flags | libc::O_NONBLOCK) } == -1
    {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

fn write_all_until(
    stdin: &mut ChildStdin,
    frame: &[u8],
    deadline: Instant,
    shutting_down: &AtomicBool,
) -> Result<(), DeadlineWriteError> {
    let descriptor = stdin.as_raw_fd();
    let mut written = 0_usize;
    while written < frame.len() {
        if shutting_down.load(Ordering::SeqCst) {
            return Err(DeadlineWriteError::Io(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "worker stopped while writing a command",
            )));
        }
        if Instant::now() >= deadline {
            return Err(DeadlineWriteError::Timeout);
        }
        match stdin.write(&frame[written..]) {
            Ok(0) => {
                return Err(DeadlineWriteError::Io(io::Error::new(
                    io::ErrorKind::WriteZero,
                    "worker stdin accepted zero bytes",
                )));
            }
            Ok(count) => written += count,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                wait_until_writable(descriptor, deadline, shutting_down)?;
            }
            Err(error) => return Err(DeadlineWriteError::Io(error)),
        }
    }
    Ok(())
}

fn wait_until_writable(
    descriptor: RawFd,
    deadline: Instant,
    shutting_down: &AtomicBool,
) -> Result<(), DeadlineWriteError> {
    loop {
        if shutting_down.load(Ordering::SeqCst) {
            return Err(DeadlineWriteError::Io(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "worker stopped while waiting to write",
            )));
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(DeadlineWriteError::Timeout);
        }
        let timeout_ms = remaining
            .min(Duration::from_millis(50))
            .as_millis()
            .clamp(1, i32::MAX as u128) as i32;
        let mut poll_descriptor = libc::pollfd {
            fd: descriptor,
            events: libc::POLLOUT,
            revents: 0,
        };
        let result = unsafe { libc::poll(&mut poll_descriptor, 1, timeout_ms) };
        if result == 0 {
            continue;
        }
        if result < 0 {
            let error = io::Error::last_os_error();
            if error.kind() == io::ErrorKind::Interrupted {
                continue;
            }
            return Err(DeadlineWriteError::Io(error));
        }
        if poll_descriptor.revents & libc::POLLOUT != 0 {
            return Ok(());
        }
        if poll_descriptor.revents & (libc::POLLERR | libc::POLLHUP | libc::POLLNVAL) != 0 {
            return Err(DeadlineWriteError::Io(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "worker stdin closed while waiting to write",
            )));
        }
    }
}

fn spawn_dispatcher(
    mut reader: BufReader<ChildStdout>,
    sender: mpsc::SyncSender<DispatchItem>,
    requests: Arc<Mutex<RequestTracker>>,
    fault: Arc<Mutex<Option<WorkerFaultRecord>>>,
    control: Arc<ProcessControl>,
) -> JoinHandle<()> {
    std::thread::spawn(move || {
        let mut progress_window = Instant::now();
        let mut progress_events = 0_usize;
        loop {
            let frame = match read_bounded_frame(&mut reader) {
                Ok(frame) if frame.is_empty() => {
                    if !control.shutting_down.load(Ordering::SeqCst) {
                        install_fault(WorkerFault::WorkerExited, &fault, &control);
                    }
                    break;
                }
                Ok(frame) => frame,
                Err(SupervisionError::Protocol(error)) => {
                    install_fault(WorkerFault::Protocol(error), &fault, &control);
                    break;
                }
                Err(_) => {
                    install_fault(WorkerFault::StdoutIo, &fault, &control);
                    break;
                }
            };
            let output = match parse_output(&frame) {
                Ok(output) => output,
                Err(error) => {
                    install_fault(WorkerFault::Protocol(error), &fault, &control);
                    break;
                }
            };
            let received_at = Instant::now();
            if matches!(output, WorkerOutput::Progress(_)) {
                if received_at.duration_since(progress_window) >= Duration::from_secs(1) {
                    progress_window = received_at;
                    progress_events = 0;
                }
                progress_events += 1;
                if progress_events > MAX_PROGRESS_EVENTS_PER_SECOND {
                    install_fault(
                        WorkerFault::Protocol(ProtocolError::EventRateExceeded),
                        &fault,
                        &control,
                    );
                    break;
                }
            }
            let validation = {
                let mut tracker = requests.lock().expect("request tracker lock");
                match &output {
                    WorkerOutput::Progress(progress) => tracker.progress(progress),
                    WorkerOutput::Result(result) => tracker.terminal(result),
                }
            };
            if let Err(error) = validation {
                install_fault(WorkerFault::Protocol(error), &fault, &control);
                break;
            }
            match sender.try_send(DispatchItem {
                received_at,
                output,
            }) {
                Ok(()) => {}
                Err(mpsc::TrySendError::Full(_)) => {
                    install_fault(
                        WorkerFault::Protocol(ProtocolError::OutputQueueOverflow),
                        &fault,
                        &control,
                    );
                    break;
                }
                Err(mpsc::TrySendError::Disconnected(_)) => break,
            }
        }
    })
}

fn spawn_stderr_monitor(
    mut stderr: ChildStderr,
    fault: Arc<Mutex<Option<WorkerFaultRecord>>>,
    control: Arc<ProcessControl>,
) -> JoinHandle<()> {
    std::thread::spawn(move || {
        let mut total = 0_usize;
        let mut overflowed = false;
        let mut buffer = [0_u8; 4096];
        loop {
            match stderr.read(&mut buffer) {
                Ok(0) => break,
                Ok(read) => {
                    total = total.saturating_add(read);
                    if total > MAX_STDERR_BYTES && !overflowed {
                        overflowed = true;
                        install_fault(WorkerFault::StderrOverflow, &fault, &control);
                    }
                }
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(_) => {
                    if !control.shutting_down.load(Ordering::SeqCst) {
                        install_fault(WorkerFault::StderrIo, &fault, &control);
                    }
                    break;
                }
            }
        }
    })
}

fn install_fault(
    new_fault: WorkerFault,
    fault: &Arc<Mutex<Option<WorkerFaultRecord>>>,
    control: &Arc<ProcessControl>,
) {
    let installed = {
        let mut slot = fault.lock().expect("worker fault lock");
        if slot.is_some() {
            false
        } else {
            *slot = Some(WorkerFaultRecord {
                fault: new_fault,
                occurred_at: Instant::now(),
            });
            true
        }
    };
    if installed && !control.shutting_down.load(Ordering::SeqCst) {
        control.begin_cleanup(CleanupMode::Abort, Duration::from_millis(500));
    }
}

fn worker_fault_error(fault: WorkerFault) -> SupervisionError {
    match fault {
        WorkerFault::Protocol(error) => SupervisionError::Protocol(error),
        WorkerFault::WorkerExited => SupervisionError::WorkerExited,
        WorkerFault::StdoutIo => SupervisionError::Io(io::Error::other("worker stdout failed")),
        WorkerFault::StderrIo => SupervisionError::Io(io::Error::other("worker stderr failed")),
        WorkerFault::StderrOverflow => SupervisionError::StderrOverflow,
    }
}

fn queued_output_precedes_eof(fault: WorkerFaultRecord, received_at: Instant) -> bool {
    matches!(fault.fault, WorkerFault::WorkerExited) && fault.occurred_at >= received_at
}

fn wait_for_group_exit(
    child: &mut Child,
    process_group_id: i32,
    grace: Duration,
) -> io::Result<bool> {
    let deadline = Instant::now() + grace;
    loop {
        let _ = child.try_wait()?;
        if !process_group_exists(process_group_id)? {
            return Ok(true);
        }
        if Instant::now() >= deadline {
            return Ok(false);
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn finish_child(child: &mut Child) -> io::Result<()> {
    if child.try_wait()?.is_none() {
        child.wait()?;
    }
    Ok(())
}

fn process_group_exists(process_group_id: i32) -> io::Result<bool> {
    let result = unsafe { libc::kill(-process_group_id, 0) };
    if result == 0 {
        return Ok(true);
    }
    let error = io::Error::last_os_error();
    match error.raw_os_error() {
        Some(libc::ESRCH) => Ok(false),
        Some(libc::EPERM) => Ok(true),
        _ => Err(error),
    }
}

fn join_thread(thread: Option<JoinHandle<()>>) -> io::Result<()> {
    if let Some(thread) = thread {
        thread
            .join()
            .map_err(|_| io::Error::other("worker I/O thread panicked"))?;
    }
    Ok(())
}

fn read_bounded_frame(reader: &mut impl BufRead) -> Result<Vec<u8>, SupervisionError> {
    let mut frame = Vec::new();
    loop {
        let available = reader.fill_buf()?;
        if available.is_empty() {
            return Ok(frame);
        }
        let consumed = available
            .iter()
            .position(|byte| *byte == b'\n')
            .map_or(available.len(), |position| position + 1);
        if frame.len() + consumed > MAX_FRAME_BYTES {
            return Err(ProtocolError::FrameTooLarge.into());
        }
        frame.extend_from_slice(&available[..consumed]);
        reader.consume(consumed);
        if frame.last() == Some(&b'\n') {
            return Ok(frame);
        }
    }
}

impl Drop for OwnedChild {
    fn drop(&mut self) {
        let _ = self.abort_and_wait(Duration::from_millis(250));
    }
}

fn create_liveness_pipe() -> io::Result<(RawFd, RawFd)> {
    let mut descriptors = [-1; 2];
    if unsafe { libc::pipe(descriptors.as_mut_ptr()) } != 0 {
        return Err(io::Error::last_os_error());
    }
    let flags = unsafe { libc::fcntl(descriptors[1], libc::F_GETFD) };
    if flags == -1
        || unsafe { libc::fcntl(descriptors[1], libc::F_SETFD, flags | libc::FD_CLOEXEC) } == -1
    {
        unsafe {
            libc::close(descriptors[0]);
            libc::close(descriptors[1]);
        }
        return Err(io::Error::last_os_error());
    }
    Ok((descriptors[0], descriptors[1]))
}

fn signal_group(group: i32, signal: i32) -> io::Result<()> {
    let result = unsafe { libc::kill(-group, signal) };
    if result == 0 {
        return Ok(());
    }
    let error = io::Error::last_os_error();
    if error.raw_os_error() == Some(libc::ESRCH) {
        Ok(())
    } else {
        Err(error)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProcessIdentity {
    pub pid: u32,
    pub start_time_epoch_seconds: u64,
    pub executable_path: PathBuf,
    pub executable_sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OwnershipReceipt {
    pub schema: OwnershipSchema,
    pub process_group_id: i32,
    pub application_build_sha256: String,
    pub worker_build_sha256: String,
    pub tap_build_sha256: String,
    pub children: Vec<ProcessIdentity>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum OwnershipSchema {
    #[serde(rename = "capture-ownership/1")]
    V1,
}

impl OwnershipReceipt {
    pub fn validate(&self) -> bool {
        let mut pids = HashSet::new();
        self.process_group_id > 0
            && valid_sha256(&self.application_build_sha256)
            && valid_sha256(&self.worker_build_sha256)
            && valid_sha256(&self.tap_build_sha256)
            && !self.children.is_empty()
            && self
                .children
                .iter()
                .any(|child| child.pid == self.process_group_id as u32)
            && self.children.iter().all(|child| {
                child.pid > 0
                    && child.start_time_epoch_seconds > 0
                    && pids.insert(child.pid)
                    && child.executable_path.is_absolute()
                    && valid_sha256(&child.executable_sha256)
            })
    }
}

pub trait ProcessInspector {
    fn inspect(&self, pid: u32) -> io::Result<ProcessInspection>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProcessInspection {
    Absent,
    Identity(ProcessIdentity),
    Unavailable,
}

pub trait GroupSignaler {
    fn terminate(&self, process_group_id: i32) -> io::Result<()>;
}

pub struct SystemProcessInspector;

impl ProcessInspector for SystemProcessInspector {
    fn inspect(&self, pid: u32) -> io::Result<ProcessInspection> {
        if probe_process_existence(pid)? == ProcessExistence::Absent {
            return Ok(ProcessInspection::Absent);
        }
        let mut system = System::new();
        let system_pid = Pid::from_u32(pid);
        system.refresh_processes(ProcessesToUpdate::Some(&[system_pid]), true);
        let Some(process) = system.process(system_pid) else {
            return Ok(ProcessInspection::Unavailable);
        };
        let Some(executable) = process.exe() else {
            return Ok(ProcessInspection::Unavailable);
        };
        let executable_sha256 = match sha256_file(executable) {
            Ok(sha256) => sha256,
            Err(_) => return Ok(ProcessInspection::Unavailable),
        };
        Ok(ProcessInspection::Identity(ProcessIdentity {
            pid,
            start_time_epoch_seconds: process.start_time(),
            executable_path: executable.to_path_buf(),
            executable_sha256,
        }))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ProcessExistence {
    Absent,
    Present,
}

fn probe_process_existence(pid: u32) -> io::Result<ProcessExistence> {
    if pid == 0 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "process identifier must be positive",
        ));
    }
    let pid = libc::pid_t::try_from(pid).map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "process identifier exceeds the platform range",
        )
    })?;
    let result = unsafe { libc::kill(pid, 0) };
    let error = (result != 0).then(io::Error::last_os_error);
    classify_process_probe(result, error)
}

fn classify_process_probe(
    result: libc::c_int,
    error: Option<io::Error>,
) -> io::Result<ProcessExistence> {
    if result == 0 {
        return Ok(ProcessExistence::Present);
    }
    let error = error.unwrap_or_else(io::Error::last_os_error);
    match error.raw_os_error() {
        Some(libc::ESRCH) => Ok(ProcessExistence::Absent),
        Some(libc::EPERM) => Ok(ProcessExistence::Present),
        _ => Err(error),
    }
}

pub struct SystemGroupSignaler;

impl GroupSignaler for SystemGroupSignaler {
    fn terminate(&self, process_group_id: i32) -> io::Result<()> {
        signal_group(process_group_id, libc::SIGTERM)
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum RecoveryDecision {
    NoChildrenLive,
    ExactGroupSignalled,
    AmbiguousIdentity,
}

#[derive(Debug, PartialEq, Eq)]
pub enum RecoveryCompletion {
    NoChildrenLive,
    StoppedExactGroup,
    AmbiguousIdentity,
    StillRunning,
}

pub fn recover_owned_group(
    receipt: &OwnershipReceipt,
    inspector: &dyn ProcessInspector,
    signaler: &dyn GroupSignaler,
) -> io::Result<RecoveryDecision> {
    if !receipt.validate() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "capture ownership receipt is malformed",
        ));
    }
    let mut live = 0;
    for expected in &receipt.children {
        match inspector.inspect(expected.pid)? {
            ProcessInspection::Absent => {}
            ProcessInspection::Identity(actual) => {
                live += 1;
                if &actual != expected {
                    return Ok(RecoveryDecision::AmbiguousIdentity);
                }
            }
            ProcessInspection::Unavailable => {
                return Ok(RecoveryDecision::AmbiguousIdentity);
            }
        }
    }
    if live == 0 {
        return Ok(RecoveryDecision::NoChildrenLive);
    }
    signaler.terminate(receipt.process_group_id)?;
    Ok(RecoveryDecision::ExactGroupSignalled)
}

pub fn recover_owned_group_and_wait(
    receipt: &OwnershipReceipt,
    inspector: &dyn ProcessInspector,
    signaler: &dyn GroupSignaler,
    grace: Duration,
) -> io::Result<RecoveryCompletion> {
    match recover_owned_group(receipt, inspector, signaler)? {
        RecoveryDecision::NoChildrenLive => return Ok(RecoveryCompletion::NoChildrenLive),
        RecoveryDecision::AmbiguousIdentity => {
            return Ok(RecoveryCompletion::AmbiguousIdentity);
        }
        RecoveryDecision::ExactGroupSignalled => {}
    }
    let deadline = Instant::now() + grace;
    loop {
        let mut live = false;
        for expected in &receipt.children {
            match inspector.inspect(expected.pid)? {
                ProcessInspection::Absent => {}
                ProcessInspection::Identity(actual) => {
                    live = true;
                    if &actual != expected {
                        return Ok(RecoveryCompletion::AmbiguousIdentity);
                    }
                }
                ProcessInspection::Unavailable => {
                    return Ok(RecoveryCompletion::AmbiguousIdentity);
                }
            }
        }
        if !live {
            return Ok(RecoveryCompletion::StoppedExactGroup);
        }
        if Instant::now() >= deadline {
            return Ok(RecoveryCompletion::StillRunning);
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn valid_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn sha256_file(path: &Path) -> io::Result<String> {
    let bytes = std::fs::read(path)?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

pub fn read_parent_liveness(fd: RawFd) -> io::Result<()> {
    let mut byte = [0_u8; 1];
    loop {
        let read = unsafe { libc::read(fd, byte.as_mut_ptr().cast(), 1) };
        if read == 0 {
            return Ok(());
        }
        if read < 0 {
            let error = io::Error::last_os_error();
            if error.kind() == io::ErrorKind::Interrupted {
                continue;
            }
            return Err(error);
        }
    }
}

pub fn internal_alpha_operations() -> HashSet<Operation> {
    use Operation::*;
    // SittingDerive and TranscriptRestore joined the packaged alpha set on
    // 2026-08-04 with the operator's guided-enrollment and correction-surface
    // registration decisions; the profile family (choices/build/inspect/
    // discard, never adopt) joined 2026-08-05 with the profile-build
    // decision; CorpusEmbed joined 2026-08-08 when `build-alpha` began staging
    // the embedding model.
    //
    // **`parse_ready` pins exact equality, so this list moves only with
    // `worker/main.py`'s — and that sentence was here, correct, and not enough.**
    // On 2026-08-08 `corpus.embed` was added to the worker's set and not to this
    // one, which left the packaged app refusing its own worker at startup with
    // `OperationMismatch`: no transcription, no capture, nothing. Every Rust
    // test passed, because they all build their own fixture sets and none read
    // the worker. `the_alpha_operation_set_is_read_from_the_worker_itself` now
    // does, so the comment is no longer the mechanism.
    [
        CaptureFinalize,
        CaptureInspect,
        TranscriptCreate,
        // TranscriptRetry joined with the source-bound immutable candidate
        // flow. Keep this in exact lockstep with the worker's advertised
        // internal-alpha operations or the packaged app refuses startup.
        TranscriptRetry,
        SittingDerive,
        TranscriptRestore,
        CorpusEmbed,
        ProfileChoices,
        ProfileBuild,
        ProfileInspect,
        ProfileDiscard,
        // NoteCreate joined 2026-08-16 with the generation-invocation
        // decision: the deterministic candidate-point assembler, moving in
        // lockstep with `worker/main.py`'s ALPHA_OPERATIONS.
        NoteCreate,
        // NoteInspect joined the same day, promoted out of the boundary lane
        // once the first real end-to-end run showed the standing worker
        // refusing it under internal-alpha left every published note stuck
        // one step short of the meeting record -- moving in lockstep with
        // `worker/main.py`'s ALPHA_OPERATIONS.
        NoteInspect,
        SpeakerSuggest,
    ]
    .into_iter()
    .collect()
}

pub fn protocol_fixture_operations() -> HashSet<Operation> {
    use Operation::*;
    [
        CaptureStart,
        CaptureFinalize,
        CaptureInspect,
        TranscriptCreate,
    ]
    .into_iter()
    .collect()
}

pub fn bounded_frame(frame: &[u8]) -> Result<&[u8], ProtocolError> {
    if frame.len() > MAX_FRAME_BYTES {
        Err(ProtocolError::FrameTooLarge)
    } else {
        Ok(frame)
    }
}

#[cfg(test)]
mod tests {
    use std::cell::{Cell, RefCell};
    use std::collections::HashMap;
    use std::rc::Rc;

    use super::*;

    /// The list this crate expects and the list the worker advertises are one
    /// contract in two languages, and `parse_ready` compares them for exact
    /// equality — so a difference is not a warning, it is a worker the app
    /// refuses to start.
    ///
    /// This reads `worker/main.py` rather than restating it. A second hand-kept
    /// copy is what drifted on 2026-08-08, and the comment saying "this list
    /// moves only with worker/main.py's" was already there and did not help.
    ///
    /// It follows the live constant on purpose: both sides are current
    /// behaviour, not a frozen artifact, so the day the worker gains an
    /// operation this must fail until Rust gains it too.
    #[test]
    fn the_alpha_operation_set_is_read_from_the_worker_itself() {
        let source = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../worker/main.py"),
        )
        .expect("worker/main.py is committed beside this crate");
        let block = source
            .split_once("ALPHA_OPERATIONS = frozenset(")
            .expect("the worker still names ALPHA_OPERATIONS")
            .1
            .split_once(')')
            .expect("the frozenset literal closes")
            .0;
        // Quoted pieces that look like `family.verb`. The block holds nothing
        // else quoted, and a stray comment string would have to contain a dot
        // and no spaces to be mistaken for one.
        let advertised: HashSet<String> = block
            .split('"')
            .filter(|piece| piece.contains('.') && !piece.contains(char::is_whitespace))
            .map(str::to_owned)
            .collect();
        assert!(
            advertised.len() >= 9,
            "parsed {} operations out of the worker; the literal's shape moved",
            advertised.len()
        );

        let parsed: HashSet<Operation> = advertised
            .iter()
            .map(|name| {
                serde_json::from_value(serde_json::Value::String(name.clone())).unwrap_or_else(
                    |_| panic!("the worker advertises {name}, which this crate cannot name"),
                )
            })
            .collect();
        assert_eq!(
            parsed,
            internal_alpha_operations(),
            "the packaged app would refuse its own worker at startup"
        );
    }

    struct FakeInspector(HashMap<u32, ProcessInspection>);
    impl ProcessInspector for FakeInspector {
        fn inspect(&self, pid: u32) -> io::Result<ProcessInspection> {
            Ok(self
                .0
                .get(&pid)
                .cloned()
                .unwrap_or(ProcessInspection::Absent))
        }
    }
    struct FakeSignaler(Cell<u32>);
    impl GroupSignaler for FakeSignaler {
        fn terminate(&self, _process_group_id: i32) -> io::Result<()> {
            self.0.set(self.0.get() + 1);
            Ok(())
        }
    }

    struct SharedInspector(Rc<RefCell<HashMap<u32, ProcessIdentity>>>);
    impl ProcessInspector for SharedInspector {
        fn inspect(&self, pid: u32) -> io::Result<ProcessInspection> {
            Ok(self
                .0
                .borrow()
                .get(&pid)
                .cloned()
                .map(ProcessInspection::Identity)
                .unwrap_or(ProcessInspection::Absent))
        }
    }

    struct ClearingSignaler {
        calls: Cell<u32>,
        processes: Rc<RefCell<HashMap<u32, ProcessIdentity>>>,
    }
    impl GroupSignaler for ClearingSignaler {
        fn terminate(&self, _process_group_id: i32) -> io::Result<()> {
            self.calls.set(self.calls.get() + 1);
            self.processes.borrow_mut().clear();
            Ok(())
        }
    }

    fn identity(pid: u32, start: u64) -> ProcessIdentity {
        ProcessIdentity {
            pid,
            start_time_epoch_seconds: start,
            executable_path: PathBuf::from("/fixed/worker"),
            executable_sha256: "d".repeat(64),
        }
    }

    #[test]
    fn fresh_launch_signals_only_exact_identity() {
        let expected = identity(44, 10);
        let receipt = OwnershipReceipt {
            schema: OwnershipSchema::V1,
            process_group_id: 44,
            application_build_sha256: "a".repeat(64),
            worker_build_sha256: "b".repeat(64),
            tap_build_sha256: "c".repeat(64),
            children: vec![expected.clone()],
        };
        let signaler = FakeSignaler(Cell::new(0));
        let exact = FakeInspector(HashMap::from([(44, ProcessInspection::Identity(expected))]));
        assert_eq!(
            recover_owned_group(&receipt, &exact, &signaler).unwrap(),
            RecoveryDecision::ExactGroupSignalled
        );
        assert_eq!(signaler.0.get(), 1);

        let reused = FakeInspector(HashMap::from([(
            44,
            ProcessInspection::Identity(identity(44, 11)),
        )]));
        assert_eq!(
            recover_owned_group(&receipt, &reused, &signaler).unwrap(),
            RecoveryDecision::AmbiguousIdentity
        );
        assert_eq!(signaler.0.get(), 1);
    }

    #[test]
    fn absent_process_is_clear_but_unavailable_identity_is_ambiguous() {
        let expected = identity(66, 30);
        let receipt = OwnershipReceipt {
            schema: OwnershipSchema::V1,
            process_group_id: 66,
            application_build_sha256: "a".repeat(64),
            worker_build_sha256: "b".repeat(64),
            tap_build_sha256: "c".repeat(64),
            children: vec![expected],
        };
        let signaler = FakeSignaler(Cell::new(0));

        assert_eq!(
            recover_owned_group(&receipt, &FakeInspector(HashMap::new()), &signaler).unwrap(),
            RecoveryDecision::NoChildrenLive
        );
        assert_eq!(signaler.0.get(), 0);

        let unavailable = FakeInspector(HashMap::from([(66, ProcessInspection::Unavailable)]));
        assert_eq!(
            recover_owned_group(&receipt, &unavailable, &signaler).unwrap(),
            RecoveryDecision::AmbiguousIdentity
        );
        assert_eq!(signaler.0.get(), 0);
    }

    #[test]
    fn process_existence_probe_maps_only_esrch_to_absent() {
        assert_eq!(
            classify_process_probe(0, None).unwrap(),
            ProcessExistence::Present
        );
        assert_eq!(
            classify_process_probe(-1, Some(io::Error::from_raw_os_error(libc::ESRCH))).unwrap(),
            ProcessExistence::Absent
        );
        assert_eq!(
            classify_process_probe(-1, Some(io::Error::from_raw_os_error(libc::EPERM))).unwrap(),
            ProcessExistence::Present
        );
        let error = classify_process_probe(-1, Some(io::Error::from_raw_os_error(libc::EINVAL)))
            .unwrap_err();
        assert_eq!(error.raw_os_error(), Some(libc::EINVAL));
    }

    #[test]
    fn system_inspector_never_classifies_current_process_as_absent() {
        assert_eq!(
            probe_process_existence(std::process::id()).unwrap(),
            ProcessExistence::Present
        );
        assert!(matches!(
            SystemProcessInspector.inspect(std::process::id()).unwrap(),
            ProcessInspection::Identity(_) | ProcessInspection::Unavailable
        ));
    }

    #[test]
    fn exact_recovery_waits_until_the_recorded_child_is_gone() {
        let expected = identity(55, 20);
        let receipt = OwnershipReceipt {
            schema: OwnershipSchema::V1,
            process_group_id: 55,
            application_build_sha256: "a".repeat(64),
            worker_build_sha256: "b".repeat(64),
            tap_build_sha256: "c".repeat(64),
            children: vec![expected.clone()],
        };
        let processes = Rc::new(RefCell::new(HashMap::from([(55, expected)])));
        let inspector = SharedInspector(processes.clone());
        let signaler = ClearingSignaler {
            calls: Cell::new(0),
            processes,
        };
        assert_eq!(
            recover_owned_group_and_wait(
                &receipt,
                &inspector,
                &signaler,
                Duration::from_millis(20),
            )
            .unwrap(),
            RecoveryCompletion::StoppedExactGroup
        );
        assert_eq!(signaler.calls.get(), 1);
    }
    #[test]
    fn only_eof_after_receipt_allows_a_queued_output() {
        let received_at = Instant::now();
        assert!(queued_output_precedes_eof(
            WorkerFaultRecord {
                fault: WorkerFault::WorkerExited,
                occurred_at: received_at + Duration::from_millis(1),
            },
            received_at,
        ));
        assert!(!queued_output_precedes_eof(
            WorkerFaultRecord {
                fault: WorkerFault::WorkerExited,
                occurred_at: received_at - Duration::from_millis(1),
            },
            received_at,
        ));
        assert!(!queued_output_precedes_eof(
            WorkerFaultRecord {
                fault: WorkerFault::Protocol(ProtocolError::Malformed),
                occurred_at: received_at + Duration::from_millis(1),
            },
            received_at,
        ));
        assert!(!queued_output_precedes_eof(
            WorkerFaultRecord {
                fault: WorkerFault::StderrOverflow,
                occurred_at: received_at + Duration::from_millis(1),
            },
            received_at,
        ));
    }

    #[test]
    fn cleanup_failure_overrides_readiness_timeout_and_remains_terminal() {
        let mut command = Command::new("/bin/sh");
        command.args(["-c", "exec sleep 30"]);
        let mut child = OwnedChild::spawn(&mut command).unwrap();
        child
            .control
            .inject_cleanup_failure(&io::Error::other("injected readiness cleanup failure"));

        let ready_error = child
            .wait_ready(Duration::from_millis(50), &protocol_fixture_operations())
            .unwrap_err();
        assert!(matches!(
            ready_error,
            SupervisionError::CleanupFailed(error)
                if error.to_string() == "injected readiness cleanup failure"
        ));

        assert!(matches!(
            child.check_health(),
            Err(SupervisionError::CleanupFailed(error))
                if error.to_string() == "injected readiness cleanup failure"
        ));
    }

    #[test]
    fn cleanup_failure_overrides_request_timeout_and_remains_terminal() {
        let script = r#"printf '%s\n' '{"schema":"worker-event/2","event":"worker.ready","protocol":2,"admission":"boundary-test","build":"test-worker","runtime":{"kind":"bundled","digest":"test-runtime"},"tap":{"build":"test-tap","available":true},"models":[{"id":"test-model","digest":"test-model-digest","available":true}],"operations":["capture.start","capture.finalize","capture.inspect","transcript.create"]}'; IFS= read -r command; exec sleep 30"#;
        let mut command = Command::new("/bin/sh");
        command.args(["-c", script]);
        let mut child = OwnedChild::spawn(&mut command).unwrap();
        child
            .wait_ready(Duration::from_secs(1), &protocol_fixture_operations())
            .unwrap();
        child
            .control
            .inject_cleanup_failure(&io::Error::other("injected cleanup failure"));

        let request = WorkerCommand::new(
            Operation::CaptureInspect,
            serde_json::json!({"meeting_id": "fixture"}),
        );
        let request_error = child
            .request_until(&request, Instant::now() + Duration::from_millis(50), |_| {
                Ok(())
            })
            .unwrap_err();
        assert!(matches!(
            request_error,
            SupervisionError::CleanupFailed(error)
                if error.to_string() == "injected cleanup failure"
        ));

        assert!(matches!(
            child.check_health(),
            Err(SupervisionError::CleanupFailed(error))
                if error.to_string() == "injected cleanup failure"
        ));
    }
}
