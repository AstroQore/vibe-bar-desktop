#[cfg(unix)]
pub(super) mod lease_platform {
    use super::super::lease::unix_epoch_millis;
    use super::super::{LeaseError, SharedStoreId, SharedStoreLeaseRecord};
    use std::collections::HashMap;
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;
    use std::os::unix::io::RawFd;
    use std::path::Path;
    use std::sync::{Mutex, OnceLock};

    #[derive(Clone, Copy)]
    enum Mode {
        Shared,
        Exclusive,
    }
    #[derive(Default)]
    struct Held {
        shared: usize,
        exclusive: bool,
    }
    #[derive(Clone, Copy)]
    struct Reservation {
        key: (u64, u64),
        mode: Mode,
    }
    struct Lock {
        fd: RawFd,
        record_name: Option<CString>,
        reservation: Reservation,
    }
    pub(crate) struct LeaseBatch {
        stores: Vec<SharedStoreId>,
        run_fd: Option<RawFd>,
        locks: Vec<Lock>,
    }

    static HELD: OnceLock<Mutex<HashMap<(u64, u64), Held>>> = OnceLock::new();
    fn held() -> &'static Mutex<HashMap<(u64, u64), Held>> {
        HELD.get_or_init(|| Mutex::new(HashMap::new()))
    }

    impl LeaseBatch {
        pub(crate) fn acquire(
            root: &Path,
            stores: &[SharedStoreId],
            maintenance: bool,
            record: &SharedStoreLeaseRecord,
        ) -> Result<Self, LeaseError> {
            let root_fd = open_absolute_dir_nofollow(root, "open_data_root")?;
            let result = (|| {
                mkdirat(root_fd, "run", 0o700, "mkdirat_run")?;
                let run_fd = open_child_dir_nofollow(root_fd, "run", "openat_run")?;
                unsafe {
                    if libc::fchmod(run_fd, 0o700) != 0 {
                        let code = errno();
                        libc::close(run_fd);
                        return Err(LeaseError::Io {
                            operation: "chmod_run",
                            code,
                        });
                    }
                }
                let mut locks = Vec::new();
                let acquired = (|| {
                    locks.push(acquire_lock(
                        run_fd,
                        "barrier",
                        if maintenance {
                            Mode::Exclusive
                        } else {
                            Mode::Shared
                        },
                        None,
                    )?);
                    for store in stores {
                        locks.push(acquire_lock(
                            run_fd,
                            store.as_raw(),
                            Mode::Exclusive,
                            Some(record),
                        )?);
                    }
                    Ok::<(), LeaseError>(())
                })();
                if let Err(error) = acquired {
                    release_locks(run_fd, &mut locks);
                    unsafe {
                        libc::close(run_fd);
                    };
                    return Err(error);
                }
                Ok(Self {
                    stores: stores.to_vec(),
                    run_fd: Some(run_fd),
                    locks,
                })
            })();
            unsafe {
                libc::close(root_fd);
            }
            result
        }
        pub(crate) fn release(&mut self) {
            let Some(run_fd) = self.run_fd.take() else {
                return;
            };
            release_locks(run_fd, &mut self.locks);
            unsafe {
                libc::close(run_fd);
            }
        }
        pub(crate) fn stores(&self) -> &[SharedStoreId] {
            &self.stores
        }
    }

    fn release_locks(run_fd: RawFd, locks: &mut Vec<Lock>) {
        for lock in locks.drain(..).rev() {
            if let Some(name) = lock.record_name {
                unsafe {
                    libc::unlinkat(run_fd, name.as_ptr(), 0);
                }
            }
            unsafe {
                libc::flock(lock.fd, libc::LOCK_UN);
                libc::close(lock.fd);
            }
            release_reservation(lock.reservation);
        }
    }

    fn acquire_lock(
        run_fd: RawFd,
        name: &str,
        mode: Mode,
        record: Option<&SharedStoreLeaseRecord>,
    ) -> Result<Lock, LeaseError> {
        let lock_name = format!("{name}.lock");
        reject_symlink(run_fd, &lock_name)?;
        let lock_name_c = cstring(&lock_name, "lock name")?;
        let fd = unsafe {
            libc::openat(
                run_fd,
                lock_name_c.as_ptr(),
                libc::O_RDWR | libc::O_CREAT | libc::O_NOFOLLOW | libc::O_CLOEXEC,
                0o600,
            )
        };
        if fd < 0 {
            return Err(map_open_error("openat_lock"));
        }
        let setup = (|| {
            unsafe {
                if libc::fchmod(fd, 0o600) != 0 {
                    return Err(io("chmod_lock"));
                }
            }
            let mut stat: libc::stat = unsafe { std::mem::zeroed() };
            unsafe {
                if libc::fstat(fd, &mut stat) != 0 {
                    return Err(io("fstat_lock"));
                }
            }
            if (stat.st_mode & libc::S_IFMT) != libc::S_IFREG {
                return Err(LeaseError::Io {
                    operation: "fstat_regular_lock",
                    code: libc::EINVAL,
                });
            }
            let reservation = reserve((stat.st_dev as u64, stat.st_ino as u64), mode)?;
            let flock_mode = match mode {
                Mode::Shared => libc::LOCK_SH,
                Mode::Exclusive => libc::LOCK_EX,
            } | libc::LOCK_NB;
            if unsafe { libc::flock(fd, flock_mode) } != 0 {
                let error = map_flock_error();
                release_reservation(reservation);
                return Err(error);
            }
            let record_name = match record {
                Some(record) => match write_record(record, run_fd, name) {
                    Ok(value) => Some(value),
                    Err(error) => {
                        unsafe {
                            libc::flock(fd, libc::LOCK_UN);
                        };
                        release_reservation(reservation);
                        return Err(error);
                    }
                },
                None => None,
            };
            Ok(Lock {
                fd,
                record_name,
                reservation,
            })
        })();
        if setup.is_err() {
            unsafe {
                libc::close(fd);
            }
        }
        setup
    }

    fn reserve(key: (u64, u64), mode: Mode) -> Result<Reservation, LeaseError> {
        let mut map = held().lock().expect("lease held-lock poisoned");
        let entry = map.entry(key).or_default();
        match mode {
            Mode::Shared if entry.exclusive => return Err(LeaseError::Busy),
            Mode::Exclusive if entry.exclusive || entry.shared != 0 => {
                return Err(LeaseError::Busy)
            }
            Mode::Shared => entry.shared += 1,
            Mode::Exclusive => entry.exclusive = true,
        }
        Ok(Reservation { key, mode })
    }
    fn release_reservation(reservation: Reservation) {
        let mut map = held().lock().expect("lease held-lock poisoned");
        let Some(entry) = map.get_mut(&reservation.key) else {
            return;
        };
        match reservation.mode {
            Mode::Shared => entry.shared = entry.shared.saturating_sub(1),
            Mode::Exclusive => entry.exclusive = false,
        }
        if entry.shared == 0 && !entry.exclusive {
            map.remove(&reservation.key);
        }
    }

    fn write_record(
        record: &SharedStoreLeaseRecord,
        run_fd: RawFd,
        store: &str,
    ) -> Result<CString, LeaseError> {
        let record_name = format!("{store}.record");
        reject_symlink(run_fd, &record_name)?;
        let temp_name = format!(
            ".{record_name}.{}.{}.tmp",
            std::process::id(),
            unix_epoch_millis()
        );
        let temp = cstring(&temp_name, "record temp name")?;
        let fd = unsafe {
            libc::openat(
                run_fd,
                temp.as_ptr(),
                libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL | libc::O_NOFOLLOW | libc::O_CLOEXEC,
                0o600,
            )
        };
        if fd < 0 {
            return Err(map_open_error("openat_record"));
        }
        let result = (|| {
            unsafe {
                if libc::fchmod(fd, 0o600) != 0 {
                    return Err(io("chmod_record"));
                }
            }
            let bytes = record.canonical_json()?;
            write_all(fd, &bytes)?;
            unsafe {
                if libc::fsync(fd) != 0 {
                    return Err(io("fsync_record"));
                }
            }
            let final_name = cstring(&record_name, "record name")?;
            unsafe {
                if libc::renameat(run_fd, temp.as_ptr(), run_fd, final_name.as_ptr()) != 0 {
                    return Err(io("renameat_record"));
                }
            }
            unsafe {
                if libc::fsync(run_fd) != 0 {
                    let _ = libc::unlinkat(run_fd, final_name.as_ptr(), 0);
                    return Err(io("fsync_run"));
                }
            }
            Ok(final_name)
        })();
        unsafe {
            libc::close(fd);
            libc::unlinkat(run_fd, temp.as_ptr(), 0);
        }
        result
    }
    fn write_all(fd: RawFd, bytes: &[u8]) -> Result<(), LeaseError> {
        let mut offset = 0;
        while offset < bytes.len() {
            let result =
                unsafe { libc::write(fd, bytes[offset..].as_ptr().cast(), bytes.len() - offset) };
            if result > 0 {
                offset += result as usize;
                continue;
            }
            if result < 0 && errno() == libc::EINTR {
                continue;
            }
            return Err(io("write_record"));
        }
        Ok(())
    }
    /// Walk an absolute data root from `/`, refusing symlinks at *every*
    /// component. `O_NOFOLLOW` only protects the final component of one
    /// `openat`; the explicit walk matches the native POSIX helper.
    fn open_absolute_dir_nofollow(
        path: &Path,
        operation: &'static str,
    ) -> Result<RawFd, LeaseError> {
        let slash = CString::new("/").expect("slash has no NUL");
        let mut current = unsafe {
            libc::open(
                slash.as_ptr(),
                libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            )
        };
        if current < 0 {
            return Err(map_open_error(operation));
        }
        let mut saw_root = false;
        let mut saw_normal = false;
        for component in path.components() {
            match component {
                std::path::Component::RootDir if !saw_root => saw_root = true,
                std::path::Component::Normal(name) if saw_root => {
                    let value = CString::new(name.as_bytes()).map_err(|_| LeaseError::Io {
                        operation,
                        code: libc::EINVAL,
                    })?;
                    let mut stat: libc::stat = unsafe { std::mem::zeroed() };
                    if unsafe {
                        libc::fstatat(
                            current,
                            value.as_ptr(),
                            &mut stat,
                            libc::AT_SYMLINK_NOFOLLOW,
                        )
                    } == 0
                        && (stat.st_mode & libc::S_IFMT) == libc::S_IFLNK
                    {
                        unsafe { libc::close(current) };
                        return Err(LeaseError::SymlinkDetected);
                    }
                    let next = unsafe {
                        libc::openat(
                            current,
                            value.as_ptr(),
                            libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
                        )
                    };
                    unsafe {
                        libc::close(current);
                    }
                    if next < 0 {
                        return Err(map_open_error(operation));
                    }
                    current = next;
                    saw_normal = true;
                }
                _ => {
                    unsafe {
                        libc::close(current);
                    };
                    return Err(LeaseError::Io {
                        operation,
                        code: libc::EINVAL,
                    });
                }
            }
        }
        if !saw_root || !saw_normal {
            unsafe {
                libc::close(current);
            };
            return Err(LeaseError::Io {
                operation,
                code: libc::EINVAL,
            });
        }
        Ok(current)
    }
    fn open_child_dir_nofollow(
        dirfd: RawFd,
        name: &str,
        operation: &'static str,
    ) -> Result<RawFd, LeaseError> {
        let value = cstring(name, "directory name")?;
        let fd = unsafe {
            libc::openat(
                dirfd,
                value.as_ptr(),
                libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            )
        };
        if fd < 0 {
            Err(map_open_error(operation))
        } else {
            Ok(fd)
        }
    }
    fn mkdirat(
        dirfd: RawFd,
        name: &str,
        mode: libc::mode_t,
        operation: &'static str,
    ) -> Result<(), LeaseError> {
        let name = cstring(name, "directory name")?;
        let result = unsafe { libc::mkdirat(dirfd, name.as_ptr(), mode) };
        if result == 0 || errno() == libc::EEXIST {
            Ok(())
        } else {
            Err(io(operation))
        }
    }
    fn reject_symlink(dirfd: RawFd, name: &str) -> Result<(), LeaseError> {
        let name = cstring(name, "leaf name")?;
        let mut stat: libc::stat = unsafe { std::mem::zeroed() };
        if unsafe { libc::fstatat(dirfd, name.as_ptr(), &mut stat, libc::AT_SYMLINK_NOFOLLOW) } == 0
        {
            if (stat.st_mode & libc::S_IFMT) == libc::S_IFLNK {
                return Err(LeaseError::SymlinkDetected);
            }
            return Ok(());
        }
        if errno() == libc::ENOENT {
            Ok(())
        } else {
            Err(io("fstatat"))
        }
    }
    fn cstring(value: &str, _what: &'static str) -> Result<CString, LeaseError> {
        CString::new(value).map_err(|_| LeaseError::Io {
            operation: "invalid_cstring",
            code: libc::EINVAL,
        })
    }
    fn errno() -> i32 {
        std::io::Error::last_os_error()
            .raw_os_error()
            .unwrap_or(libc::EIO)
    }
    fn io(operation: &'static str) -> LeaseError {
        LeaseError::Io {
            operation,
            code: errno(),
        }
    }
    fn map_open_error(operation: &'static str) -> LeaseError {
        if errno() == libc::ELOOP {
            LeaseError::SymlinkDetected
        } else {
            io(operation)
        }
    }
    fn map_flock_error() -> LeaseError {
        let code = errno();
        if code == libc::EWOULDBLOCK {
            LeaseError::Busy
        } else {
            LeaseError::Io {
                operation: "flock",
                code,
            }
        }
    }
}

#[cfg(not(unix))]
pub(super) mod lease_platform {
    use super::super::{LeaseError, SharedStoreId, SharedStoreLeaseRecord};
    use std::path::Path;
    /// Windows deliberately has no fake `flock` implementation. A future
    /// Windows contract must name its sole-writer primitive explicitly.
    pub(crate) struct LeaseBatch {
        stores: Vec<SharedStoreId>,
    }
    impl LeaseBatch {
        pub(crate) fn acquire(
            _root: &Path,
            _stores: &[SharedStoreId],
            _maintenance: bool,
            _record: &SharedStoreLeaseRecord,
        ) -> Result<Self, LeaseError> {
            Err(LeaseError::UnsupportedPlatform)
        }
        pub(crate) fn release(&mut self) {}
        pub(crate) fn stores(&self) -> &[SharedStoreId] {
            &self.stores
        }
    }
}
