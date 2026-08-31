//! An advisory lock over one shared file, held across a read-modify-write.
//!
//! `settings.json` has two writers in separate processes. Each re-reads the
//! file before writing so the other's keys survive, but re-read and rename are
//! two steps: two writers can interleave between them and the second one's
//! merge is then based on a file that no longer exists. The window is small
//! and the loss is silent, which is the combination worth a lock.
//!
//! `flock(2)`, the same call the native app makes on the same path. The kernel
//! releases it when the descriptor closes, process death included, so there is
//! no stale lock to break and no liveness check to get wrong.

use std::fs::{self, File, OpenOptions};
use std::io;
use std::path::Path;

/// Run `body` while holding the lock named `name` under `directory`.
///
/// Failing to take the lock is not failing to write: a read-only or unusual
/// filesystem degrades to the unlocked behaviour that came before, rather than
/// to losing the user's settings. The narrow race is worth closing; it is not
/// worth a new way to fail.
pub fn with_lock<T>(name: &str, directory: &Path, body: impl FnOnce() -> T) -> T {
    let held = acquire(name, directory);
    let value = body();
    drop(held);
    value
}

/// Holding the descriptor holds the lock: closing it is what releases, which
/// covers an unwind out of `body` as well as an ordinary return.
struct Held(#[allow(dead_code)] File);

fn acquire(name: &str, directory: &Path) -> Option<Held> {
    let run = directory.join("run");
    fs::create_dir_all(&run).ok()?;
    restrict_directory(&run);
    let file = OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .truncate(false)
        .open(run.join(format!("{name}.lock")))
        .ok()?;
    lock_exclusive(&file).ok()?;
    Some(Held(file))
}

#[cfg(unix)]
fn lock_exclusive(file: &File) -> io::Result<()> {
    use std::os::unix::io::AsRawFd;
    // Blocking: the other holder has a read, a merge and a rename to do, and
    // waiting for that is the entire point.
    let result = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX) };
    if result == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

/// No Windows client writes shared settings yet — the native app it would be
/// sharing with is macOS-only. Returning an error here means the fallback
/// above applies, which is the same unlocked write both clients did before,
/// rather than a lock that silently is not one.
#[cfg(not(unix))]
fn lock_exclusive(_file: &File) -> io::Result<()> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "advisory file locking is implemented for unix only",
    ))
}

#[cfg(unix)]
fn restrict_directory(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    let _ = fs::set_permissions(path, fs::Permissions::from_mode(0o700));
}

#[cfg(not(unix))]
fn restrict_directory(_path: &Path) {}
