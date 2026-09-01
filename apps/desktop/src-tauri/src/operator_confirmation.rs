//! "Is it you?" — macOS Touch ID, with the system's own password fallback.
//!
//! This is the only place in the app that calls LocalAuthentication, and it is
//! deliberately a trait with one real implementation and one fake. The real
//! prompt cannot run in a test: it draws a system panel and waits for a finger
//! or a typed password. So every gate path in `meeting_lock` and `main.rs` is
//! written against [`ConfirmsOperator`], the fake drives all three outcomes in
//! the suite, and **the real Touch ID prompt is live-run evidence, never test
//! evidence.** Nothing in `cargo test` proves a finger was read.
//!
//! # Why LAContext and not a Tauri plugin
//!
//! Checked 2026-08-31 against the official plugin listing (v2.tauri.app/plugin/)
//! and the biometric plugin's own page: `tauri-plugin-biometric`'s supported-
//! platforms table lists android and ios only, with macos blank. There is no
//! first-party desktop route, so this calls the platform API directly through
//! `objc2-local-authentication`, the objc2 family's generated binding for
//! LocalAuthentication.framework.
//!
//! # Policy
//!
//! `LAPolicy::DeviceOwnerAuthentication` — Touch ID first, and macOS falls back
//! to the login password on its own when biometry is unavailable or refused.
//! The narrower `DeviceOwnerAuthenticationWithBiometrics` would fail outright on
//! a Mac without a Touch Bar or Touch ID sensor, which is most desktop Macs.
//!
//! # Threading
//!
//! `evaluatePolicy:localizedReason:reply:` is asynchronous: it returns
//! immediately and calls its reply block on an arbitrary queue once the person
//! has answered. Blocking on that answer is a human-scale wait — seconds, or a
//! minute if they walk away — so the calling thread must never be the main
//! thread and must never be holding a lock anything else needs. Both are the
//! caller's responsibility and both are stated at the call sites: every command
//! that confirms is `#[tauri::command(async)]` (so it runs on the async
//! runtime's pool, not the UI thread) and resolves its meeting, releases the
//! storage sequence, and only then confirms.

#[cfg(target_os = "macos")]
use std::sync::mpsc;

/// What the system said.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Confirmation {
    /// The person proved they are the device owner.
    Confirmed,
    /// They cancelled, failed, or refused. Ordinary, and not an error worth a
    /// diagnostic — the caller returns to the locked state quietly.
    Declined,
    /// This Mac cannot ask: no Touch ID, no password set, LocalAuthentication
    /// unavailable, or the evaluation itself failed to run.
    ///
    /// Distinct from `Declined` because the honest sentence is different.
    /// A decline says "not this time"; this says "not on this Mac, ever",
    /// and the lock stays on either way.
    Unavailable,
}

/// The seam every gate is written against.
pub(crate) trait ConfirmsOperator: Send + Sync {
    /// Asks the person to prove they are the device owner. `reason` is
    /// rendered by macOS inside its own prompt, so it names the act, never a
    /// meeting's title or any of its content.
    fn confirm(&self, reason: &str) -> Confirmation;

    /// Whether this Mac could ask at all, without asking.
    ///
    /// Used before offering to lock a meeting: locking on a Mac that can never
    /// confirm would create a meeting nothing in this app can reopen. It is a
    /// preflight, not a gate — no decision that refuses access depends on it.
    fn available(&self) -> bool;
}

/// The real one: LocalAuthentication on this Mac.
#[derive(Debug, Default)]
pub(crate) struct DeviceOwnerConfirmation;

#[cfg(target_os = "macos")]
impl ConfirmsOperator for DeviceOwnerConfirmation {
    fn confirm(&self, reason: &str) -> Confirmation {
        use block2::RcBlock;
        use objc2::runtime::Bool;
        use objc2_foundation::{NSError, NSString};
        use objc2_local_authentication::{LAContext, LAPolicy};

        // SAFETY: `LAContext::new` is an ordinary Objective-C allocation with
        // no preconditions. The returned `Retained` keeps the context alive
        // for the whole evaluation, which the framework requires — dropping it
        // early cancels the prompt.
        let context = unsafe { LAContext::new() };
        let policy = LAPolicy::DeviceOwnerAuthentication;
        // Preflight. `canEvaluatePolicy` answers without drawing anything, so
        // a Mac that cannot ask says so instead of showing a panel that fails.
        // SAFETY: the context is live and the policy is a framework constant.
        if unsafe { context.canEvaluatePolicy_error(policy) }.is_err() {
            return Confirmation::Unavailable;
        }
        let (sender, receiver) = mpsc::sync_channel::<bool>(1);
        let reply = RcBlock::new(move |success: Bool, _error: *mut NSError| {
            // The error is deliberately not read. Its contents describe why a
            // person failed to authenticate, which is not something this app
            // logs, renders, or branches on: declined is declined. Send is
            // best-effort because a dropped receiver means the caller is gone.
            let _ = sender.send(success.as_bool());
        });
        let localized_reason = NSString::from_str(reason);
        // SAFETY: the reply block is `move` over a `SyncSender`, which is
        // `Send`, satisfying the binding's "reply block must be sendable"
        // requirement. The context outlives the call because it is held on
        // this stack frame until `recv` returns below.
        unsafe {
            context.evaluatePolicy_localizedReason_reply(policy, &localized_reason, &reply);
        }
        // The human-scale wait. This thread is an async command worker, never
        // the UI thread, and holds no storage lock — see the module header.
        match receiver.recv() {
            Ok(true) => Confirmation::Confirmed,
            Ok(false) => Confirmation::Declined,
            // The block was dropped without replying: the evaluation never
            // completed, which is a failure to ask rather than an answer.
            Err(_) => Confirmation::Unavailable,
        }
    }

    fn available(&self) -> bool {
        use objc2_local_authentication::{LAContext, LAPolicy};

        // SAFETY: as in `confirm` — an ordinary allocation, and a preflight
        // that draws nothing.
        let context = unsafe { LAContext::new() };
        unsafe {
            context
                .canEvaluatePolicy_error(LAPolicy::DeviceOwnerAuthentication)
                .is_ok()
        }
    }
}

/// Off macOS there is no device-owner check to run, so every confirmation is
/// unavailable and every lock stays on. The product ships only on macOS; this
/// exists so the crate still compiles for anyone building it elsewhere, and it
/// fails closed rather than waving actions through.
#[cfg(not(target_os = "macos"))]
impl ConfirmsOperator for DeviceOwnerConfirmation {
    fn confirm(&self, _reason: &str) -> Confirmation {
        Confirmation::Unavailable
    }

    fn available(&self) -> bool {
        false
    }
}

#[cfg(test)]
pub(crate) mod fake {
    use std::sync::Mutex;

    use super::{Confirmation, ConfirmsOperator};

    /// A scripted stand-in for the system prompt.
    ///
    /// It records what it was asked, so a test can prove a gate confirmed
    /// before it acted rather than only that the outcome matched.
    #[derive(Debug)]
    pub(crate) struct FakeConfirmation {
        outcome: Confirmation,
        available: bool,
        pub(crate) asked: Mutex<Vec<String>>,
    }

    impl FakeConfirmation {
        pub(crate) fn confirming() -> Self {
            Self::new(Confirmation::Confirmed, true)
        }

        pub(crate) fn declining() -> Self {
            Self::new(Confirmation::Declined, true)
        }

        pub(crate) fn unavailable() -> Self {
            Self::new(Confirmation::Unavailable, false)
        }

        fn new(outcome: Confirmation, available: bool) -> Self {
            Self {
                outcome,
                available,
                asked: Mutex::new(Vec::new()),
            }
        }

        pub(crate) fn times_asked(&self) -> usize {
            self.asked.lock().expect("fake confirmation lock").len()
        }
    }

    impl ConfirmsOperator for FakeConfirmation {
        fn confirm(&self, reason: &str) -> Confirmation {
            self.asked
                .lock()
                .expect("fake confirmation lock")
                .push(reason.to_owned());
            self.outcome
        }

        fn available(&self) -> bool {
            self.available
        }
    }
}

#[cfg(test)]
mod tests {
    use super::fake::FakeConfirmation;
    use super::*;

    #[test]
    fn the_fake_reports_each_scripted_outcome_and_records_the_reason() {
        let confirming = FakeConfirmation::confirming();
        assert_eq!(confirming.confirm("open it"), Confirmation::Confirmed);
        assert!(confirming.available());
        assert_eq!(confirming.times_asked(), 1);
        assert_eq!(
            confirming.asked.lock().unwrap().as_slice(),
            ["open it".to_owned()]
        );

        let declining = FakeConfirmation::declining();
        assert_eq!(declining.confirm("open it"), Confirmation::Declined);
        assert!(declining.available());

        // The Mac that cannot ask. Both halves report it: `available` is what
        // the lock affordance preflights on, `confirm` is what an unlock hits.
        let unavailable = FakeConfirmation::unavailable();
        assert_eq!(unavailable.confirm("open it"), Confirmation::Unavailable);
        assert!(!unavailable.available());
    }

    #[test]
    fn declined_and_unavailable_are_different_answers() {
        // They lead to different sentences on screen and must never collapse
        // into one "could not confirm" state.
        assert_ne!(Confirmation::Declined, Confirmation::Unavailable);
    }
}
