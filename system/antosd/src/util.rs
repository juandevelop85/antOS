//! Small, crate-wide infrastructure helpers (T31.7).
//!
//! Introduced to close out development rule 1 — "cero `unwrap()` o
//! `expect()` en rutas de ejecución de IPC o demonio" — for the two
//! patterns repeated across the largest number of call sites: a `Mutex`
//! that might be poisoned, and a system clock read that might, in
//! principle, fail.

use anyhow::{Context, Result};
use std::ffi::OsString;
use std::path::Path;
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

/// Reads an environment variable, falling back to its `SYSO_*` predecessor
/// with a one-line deprecation notice on `stderr` (T31.11: retiring the
/// `syso` compatibility layer over one soft-transition cycle instead of
/// dropping `SYSO_*` in silence — a script or shell profile that still
/// exports it keeps working, but is told to move on).
pub fn env_with_legacy_fallback(current: &str, legacy: &str) -> Option<OsString> {
    if let Some(v) = std::env::var_os(current) {
        return Some(v);
    }
    let v = std::env::var_os(legacy)?;
    eprintln!("antOS · aviso: {legacy} está obsoleta, usa {current} en su lugar (T31.11)");
    Some(v)
}

/// One-time migration of a legacy on-disk path to its antOS-branded
/// replacement (T31.11: retiring the `syso` compatibility layer).
///
/// A no-op whenever there is nothing useful to do: `new` already exists (the
/// migration already ran, or the user created a fresh `new` themselves —
/// this never overwrites it), or `old` doesn't exist either. Otherwise
/// renames `old` to `new` — creating `new`'s parent directory first if
/// needed — and prints a one-line notice to `stderr` so the migration stays
/// visible instead of silently moving a user's files.
pub fn migrate_legacy_path(old: &Path, new: &Path) -> Result<()> {
    if new.exists() || !old.exists() {
        return Ok(());
    }
    if let Some(parent) = new.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("creating {}", parent.display()))?;
    }
    std::fs::rename(old, new)
        .with_context(|| format!("migrando {} a {}", old.display(), new.display()))?;
    eprintln!(
        "antOS · aviso: se migró automáticamente {} a {} (T31.11)",
        old.display(),
        new.display()
    );
    Ok(())
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
        assert!(
            handle.join().is_err(),
            "the spawned thread must actually have panicked"
        );

        // A plain `.lock().unwrap()` would panic again here. This must not.
        let mut guard = lock_or_recover(&m);
        *guard = 7;
        drop(guard);
        assert_eq!(
            *lock_or_recover(&m),
            7,
            "the lock must remain usable after recovering from poison"
        );
    }

    #[test]
    fn test_unix_now_returns_a_plausible_recent_timestamp() {
        let now = unix_now().unwrap();
        // Any time after this file was written; guards against an
        // accidental epoch-zero or unit mixup (e.g. millis vs. secs).
        assert!(now > 1_700_000_000, "got implausible unix time: {now}");
    }

    #[test]
    fn test_env_with_legacy_fallback_prefers_the_current_variable() {
        std::env::remove_var("T31_11_TEST_LEGACY_A");
        std::env::set_var("T31_11_TEST_CURRENT_A", "nuevo");
        std::env::set_var("T31_11_TEST_LEGACY_A", "viejo");

        let v = env_with_legacy_fallback("T31_11_TEST_CURRENT_A", "T31_11_TEST_LEGACY_A");
        assert_eq!(v, Some(std::ffi::OsString::from("nuevo")));

        std::env::remove_var("T31_11_TEST_CURRENT_A");
        std::env::remove_var("T31_11_TEST_LEGACY_A");
    }

    #[test]
    fn test_env_with_legacy_fallback_falls_back_to_the_legacy_variable() {
        std::env::remove_var("T31_11_TEST_CURRENT_B");
        std::env::set_var("T31_11_TEST_LEGACY_B", "viejo");

        let v = env_with_legacy_fallback("T31_11_TEST_CURRENT_B", "T31_11_TEST_LEGACY_B");
        assert_eq!(v, Some(std::ffi::OsString::from("viejo")));

        std::env::remove_var("T31_11_TEST_LEGACY_B");
    }

    #[test]
    fn test_env_with_legacy_fallback_returns_none_when_neither_is_set() {
        std::env::remove_var("T31_11_TEST_CURRENT_C");
        std::env::remove_var("T31_11_TEST_LEGACY_C");

        assert_eq!(
            env_with_legacy_fallback("T31_11_TEST_CURRENT_C", "T31_11_TEST_LEGACY_C"),
            None
        );
    }

    #[test]
    fn test_migrate_legacy_path_renames_old_into_new() {
        let root = std::env::temp_dir().join(format!(
            "antos-migrate-test-{}-{}",
            std::process::id(),
            unix_now().unwrap()
        ));
        let old = root.join(".syso");
        let new = root.join(".antos");
        std::fs::create_dir_all(&old).unwrap();
        std::fs::write(old.join("marker.txt"), b"config real del usuario").unwrap();

        migrate_legacy_path(&old, &new).unwrap();

        assert!(
            !old.exists(),
            "the legacy path must be gone after migrating"
        );
        assert!(new.is_dir(), "the new path must exist after migrating");
        assert_eq!(
            std::fs::read(new.join("marker.txt")).unwrap(),
            b"config real del usuario",
            "the user's actual file content must survive the migration"
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn test_migrate_legacy_path_never_overwrites_an_existing_new_path() {
        let root = std::env::temp_dir().join(format!(
            "antos-migrate-test-noop-{}-{}",
            std::process::id(),
            unix_now().unwrap()
        ));
        let old = root.join(".syso");
        let new = root.join(".antos");
        std::fs::create_dir_all(&old).unwrap();
        std::fs::write(old.join("marker.txt"), b"config vieja").unwrap();
        std::fs::create_dir_all(&new).unwrap();
        std::fs::write(new.join("marker.txt"), b"config nueva, ya vigente").unwrap();

        migrate_legacy_path(&old, &new).unwrap();

        // Neither side is touched: the new path already "won", and the old
        // one is left alone rather than silently deleted.
        assert!(old.exists());
        assert_eq!(
            std::fs::read(new.join("marker.txt")).unwrap(),
            b"config nueva, ya vigente"
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn test_migrate_legacy_path_is_a_noop_when_neither_path_exists() {
        let root = std::env::temp_dir().join(format!(
            "antos-migrate-test-absent-{}-{}",
            std::process::id(),
            unix_now().unwrap()
        ));
        let old = root.join(".syso");
        let new = root.join(".antos");

        assert!(migrate_legacy_path(&old, &new).is_ok());
        assert!(!old.exists());
        assert!(!new.exists());
    }
}
