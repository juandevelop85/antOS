//! Small, crate-wide infrastructure helpers (T31.7).
//!
//! Introduced to close out development rule 1 — "cero `unwrap()` o
//! `expect()` en rutas de ejecución de IPC o demonio" — for the two
//! patterns repeated across the largest number of call sites: a `Mutex`
//! that might be poisoned, and a system clock read that might, in
//! principle, fail.

use anyhow::{Context, Result};
use std::sync::{Mutex, MutexGuard};
use std::time::{SystemTime, UNIX_EPOCH};

/// Locks `mutex`, recovering transparently from lock poisoning instead of
/// panicking (T31.7).
///
/// A poisoned lock means some *other* thread panicked while holding it —
/// not that the data inside is corrupt. A panic unwinds Rust's own stack
/// frames cleanly; it cannot leave a `Mutex<T>`'s protected value in a
/// torn, partially-written state the way a crash mid-`memcpy` could in a
/// language without that guarantee. So the data behind the poison is still
/// safe to use — usually still logically consistent, since most panics
/// here happen on error paths *before* a mutation, not mid-mutation.
///
/// Propagating the poison forever (`.lock().unwrap()`, which panics again
/// on a poisoned lock) turns one isolated panic into a permanent, cascading
/// failure of every other call site that shares the same global state, for
/// the rest of the process's life. Recovering is the better default for a
/// long-running daemon; the recovery is still logged, so a poisoned lock
/// stays visible instead of being silently swallowed.
pub fn lock_or_recover<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    match mutex.lock() {
        Ok(guard) => guard,
        Err(poisoned) => {
            eprintln!("antOS · aviso: se recuperó un cerrojo envenenado por un pánico previo en otro hilo (T31.7)");
            poisoned.into_inner()
        }
    }
}

/// Seconds since the Unix epoch, propagating an error instead of panicking
/// if the system clock is ever set before 1970 (T31.7).
pub fn unix_now() -> Result<u64> {
    Ok(SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("system clock is set before the Unix epoch")?
        .as_secs())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    #[test]
    fn test_lock_or_recover_returns_the_value_on_a_healthy_lock() {
        let m = Mutex::new(41);
        {
            let mut guard = lock_or_recover(&m);
            *guard += 1;
        }
        assert_eq!(*lock_or_recover(&m), 42);
    }

    #[test]
    fn test_lock_or_recover_survives_a_poisoned_lock() {
        let m = std::sync::Arc::new(Mutex::new(0));
        let m2 = m.clone();

        // Poison the lock: panic while holding it, on another thread.
        let handle = std::thread::spawn(move || {
            let _guard = m2.lock().unwrap();
            panic!("intentional panic to poison the lock for this test");
        });
        assert!(handle.join().is_err(), "the spawned thread must actually have panicked");

        // A plain `.lock().unwrap()` would panic again here. This must not.
        let mut guard = lock_or_recover(&m);
        *guard = 7;
        drop(guard);
        assert_eq!(*lock_or_recover(&m), 7, "the lock must remain usable after recovering from poison");
    }

    #[test]
    fn test_unix_now_returns_a_plausible_recent_timestamp() {
        let now = unix_now().unwrap();
        // Any time after this file was written; guards against an
        // accidental epoch-zero or unit mixup (e.g. millis vs. secs).
        assert!(now > 1_700_000_000, "got implausible unix time: {now}");
    }
}
