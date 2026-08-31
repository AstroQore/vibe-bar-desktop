//! The writer this lock excludes is the native macOS app, a separate process
//! in another language. `flock(2)` is the same primitive from either side, but
//! that is worth checking rather than assuming.

use std::path::Path;
#[cfg(unix)]
use std::process::Command;

use vibebar_desktop_core::shared::file_lock;

/// Ask a process that is not this one to take the same lock, without blocking.
///
/// Unix only, like its callers: the lock has no Windows implementation, since
/// the writer it would exclude is the macOS app.
#[cfg(unix)]
fn foreign_attempt(lock_path: &Path) -> String {
    let output = Command::new("python3")
        .arg("-c")
        .arg(
            r#"
import fcntl, os, sys
fd = os.open(sys.argv[1], os.O_CREAT | os.O_RDWR, 0o600)
try:
    fcntl.flock(fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
    print("took")
except OSError:
    print("blocked")
"#,
        )
        .arg(lock_path)
        .output()
        .expect("python3 runs");
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

#[test]
#[cfg(unix)]
fn a_foreign_process_is_excluded_while_this_one_holds_the_lock() {
    let directory = tempfile::tempdir().expect("temp dir");
    let lock_path = directory.path().join("run/settings.lock");

    let while_held = file_lock::with_lock("settings", directory.path(), || {
        foreign_attempt(&lock_path)
    });
    assert_eq!(while_held, "blocked", "another process took the lock while we held it");
    assert_eq!(foreign_attempt(&lock_path), "took", "the lock was not released");
}

#[test]
#[cfg(unix)]
fn releases_the_lock_when_the_body_panics() {
    let directory = tempfile::tempdir().expect("temp dir");
    let lock_path = directory.path().join("run/settings.lock");

    let result = std::panic::catch_unwind(|| {
        file_lock::with_lock("settings", directory.path(), || panic!("while holding"));
    });
    assert!(result.is_err());
    assert_eq!(
        foreign_attempt(&lock_path),
        "took",
        "a panic inside the body left the lock held"
    );
}

/// Settings that cannot be saved is a worse outcome than a race that was the
/// shipped behaviour until now.
#[test]
fn runs_the_body_even_when_the_lock_cannot_be_taken() {
    let mut ran = false;
    file_lock::with_lock("settings", Path::new("/dev/null/nowhere"), || ran = true);
    assert!(ran);
}

#[test]
fn gives_back_what_the_body_returns() {
    let directory = tempfile::tempdir().expect("temp dir");
    assert_eq!(file_lock::with_lock("value", directory.path(), || 7), 7);
}

/// The shared write contract puts the lock file at 0600, like everything else
/// under the data root. `OpenOptions` defaults to 0666, which the usual umask
/// leaves at 0644.
#[test]
#[cfg(unix)]
fn creates_its_lock_file_private_to_the_user() {
    use std::os::unix::fs::PermissionsExt;

    let directory = tempfile::tempdir().expect("temp dir");
    file_lock::with_lock("settings", directory.path(), || {});

    let mode = std::fs::metadata(directory.path().join("run/settings.lock"))
        .expect("the lock file exists")
        .permissions()
        .mode()
        & 0o777;
    assert_eq!(mode, 0o600, "the lock file is readable by other users");
}
