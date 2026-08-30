//! Capability-gated transaction foundation for a future shared settings writer.
//!
//! This is deliberately unavailable to the default product: the only
//! authorization constructor is compiled for crate tests or the non-default
//! `settings-writer-probe` feature, and it accepts only the existing synthetic
//! lease root. The manifest remains `legacy_unsafe`; production
//! `acquire_writer` still rejects Settings.

use std::ffi::{OsStr, OsString};
use std::io::{Read, Write};
use std::path::Path;

use cap_fs_ext::{FollowSymlinks, OpenOptionsFollowExt};
#[cfg(unix)]
use cap_std::fs::OpenOptionsExt;
use cap_std::fs::{Dir, OpenOptions};
use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::shared::settings_document::{
    SettingsDocument, SettingsPatchError, SettingsPatchResult, SettingsThreeWayPatch,
    MAX_SETTINGS_DOCUMENT_BYTES, SETTINGS_RELATIVE_PATH,
};
use crate::storage_contract::{LeaseError, SharedStoreLeaseBatch};
#[cfg(any(test, feature = "settings-writer-probe"))]
use crate::storage_contract::{SharedStoreId, SharedStoreLeaseRole};

/// A live diagnostic lease plus a verified synthetic root. The fields stay
/// private so a default-product caller cannot manufacture this capability.
pub struct SettingsWriterAuthorization {
    /// Stable directory capability acquired once, before callers can rename
    /// the synthetic root path. Every later file operation is relative to it.
    root: Dir,
    #[allow(dead_code)]
    lease: SharedStoreLeaseBatch,
}

impl SettingsWriterAuthorization {
    /// Constructible only in tests or with the explicit diagnostic feature.
    /// `SharedStoreLeaseBatch` independently verifies the `VibeBarLease-*`
    /// temporary-root boundary and obtains the SettingsEditor lock.
    #[cfg(any(test, feature = "settings-writer-probe"))]
    pub fn acquire_synthetic(
        root: &Path,
        client_id: &str,
    ) -> Result<Self, SettingsTransactionError> {
        let root = root
            .canonicalize()
            .map_err(|error| SettingsTransactionError::Io {
                operation: "canonicalize_root",
                error,
            })?;
        #[cfg(unix)]
        let expected_identity = {
            use std::os::unix::fs::MetadataExt;
            let metadata =
                std::fs::symlink_metadata(&root).map_err(|error| SettingsTransactionError::Io {
                    operation: "lstat_authorized_root",
                    error,
                })?;
            if metadata.file_type().is_symlink() || !metadata.is_dir() {
                return Err(SettingsTransactionError::UnsafePath(
                    "synthetic root is not a real directory",
                ));
            }
            (metadata.dev(), metadata.ino())
        };
        let lease = SharedStoreLeaseBatch::acquire_synthetic_probe(
            &root,
            &[SharedStoreId::Settings],
            SharedStoreLeaseRole::SettingsEditor,
            false,
            client_id,
        )?;
        let directory = crate::paths::open_or_create_ambient_dir(&root).map_err(|error| {
            SettingsTransactionError::Io {
                operation: "open_authorized_root_nofollow",
                error,
            }
        })?;
        #[cfg(unix)]
        {
            use cap_std::fs::MetadataExt;
            let metadata =
                directory
                    .dir_metadata()
                    .map_err(|error| SettingsTransactionError::Io {
                        operation: "stat_authorized_root",
                        error,
                    })?;
            if (metadata.dev(), metadata.ino()) != expected_identity {
                return Err(SettingsTransactionError::UnsafePath(
                    "synthetic root changed while acquiring its lease",
                ));
            }
        }
        Ok(Self {
            root: directory,
            lease,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsTransactionStage {
    TempCreated,
    FileSynced,
    BeforeRename,
    Renamed,
    ParentSynced,
}

/// Tests can inject a crash-like failure at an exact durable-write boundary.
pub trait SettingsTransactionSeam {
    fn checkpoint(&self, stage: SettingsTransactionStage) -> Result<(), SettingsTransactionError>;
}

pub struct NoopSettingsTransactionSeam;
impl SettingsTransactionSeam for NoopSettingsTransactionSeam {
    fn checkpoint(&self, _stage: SettingsTransactionStage) -> Result<(), SettingsTransactionError> {
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsTransactionDurability {
    /// macOS parent-directory sync completed after the atomic replacement.
    Durable,
    /// Replacement is atomic within the directory, but Windows/Linux do not
    /// promise a portable parent-directory sync through this capability layer.
    AtomicReplaceOnly,
}

#[derive(Debug)]
pub struct SettingsTransactionOutcome {
    pub patch: SettingsPatchResult,
    pub durability: Option<SettingsTransactionDurability>,
}

#[derive(Debug, Error)]
pub enum SettingsTransactionError {
    #[error(transparent)]
    Lease(#[from] LeaseError),
    #[error(transparent)]
    Patch(#[from] SettingsPatchError),
    #[error("settings path rejected: {0}")]
    UnsafePath(&'static str),
    #[error("settings transaction failed while {operation}: {error}")]
    Io {
        operation: &'static str,
        error: std::io::Error,
    },
    #[error("injected transaction failure at {0:?}")]
    InjectedFailure(SettingsTransactionStage),
    /// Rename succeeded, but the subsequent parent sync did not. Callers must
    /// re-read before retrying; returning success here would be a false claim.
    #[error("settings replacement occurred but parent sync was not confirmed")]
    PostRenameUnconfirmed,
    #[error("settings source changed after the patch was based on it")]
    SourceChangedBeforeCommit,
}

/// Re-read, merge, and atomically replace the synthetic `settings.json` only
/// when the patch actually changes it. This function has no default-product
/// route because the authorization capability cannot be constructed there.
pub fn apply_settings_patch(
    authorization: &SettingsWriterAuthorization,
    patch: &SettingsThreeWayPatch,
) -> Result<SettingsTransactionOutcome, SettingsTransactionError> {
    apply_settings_patch_with_seam(authorization, patch, &NoopSettingsTransactionSeam)
}

pub fn apply_settings_patch_with_seam(
    authorization: &SettingsWriterAuthorization,
    patch: &SettingsThreeWayPatch,
    seam: &dyn SettingsTransactionSeam,
) -> Result<SettingsTransactionOutcome, SettingsTransactionError> {
    let directory =
        authorization
            .root
            .try_clone()
            .map_err(|error| SettingsTransactionError::Io {
                operation: "clone_authorized_root",
                error,
            })?;
    let (current, source) = read_current_document(&directory)?;
    let patch = patch.apply(&current)?;
    if !patch.write_required {
        return Ok(SettingsTransactionOutcome {
            patch,
            durability: None,
        });
    }
    let bytes =
        serde_json::to_vec(&patch.document.to_value()).map_err(SettingsPatchError::InvalidJson)?;
    if bytes.len() > MAX_SETTINGS_DOCUMENT_BYTES {
        return Err(SettingsPatchError::SizeLimit {
            actual: bytes.len(),
            max: MAX_SETTINGS_DOCUMENT_BYTES,
        }
        .into());
    }
    let durability = replace_atomically(&directory, &bytes, source, seam)?;
    Ok(SettingsTransactionOutcome {
        patch,
        durability: Some(durability),
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SourceFingerprint {
    exists: bool,
    len: u64,
    sha256: [u8; 32],
}

fn read_current_document(
    directory: &Dir,
) -> Result<(SettingsDocument, SourceFingerprint), SettingsTransactionError> {
    let bytes = read_source_bytes(directory)?;
    match bytes {
        Some(bytes) => {
            let fingerprint = SourceFingerprint {
                exists: true,
                len: bytes.len() as u64,
                sha256: Sha256::digest(&bytes).into(),
            };
            Ok((SettingsDocument::parse_bytes(&bytes)?, fingerprint))
        }
        None => Ok((
            SettingsDocument::from_value(serde_json::Value::Object(serde_json::Map::new()))?,
            SourceFingerprint {
                exists: false,
                len: 0,
                sha256: Sha256::digest([]).into(),
            },
        )),
    }
}

fn read_source_bytes(directory: &Dir) -> Result<Option<Vec<u8>>, SettingsTransactionError> {
    let name = Path::new(SETTINGS_RELATIVE_PATH);
    match directory.symlink_metadata(name) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => {
            return Err(SettingsTransactionError::UnsafePath(
                "settings source is unsafe",
            ));
        }
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(SettingsTransactionError::Io {
                operation: "lstat_settings",
                error,
            })
        }
    }
    let mut options = OpenOptions::new();
    options.read(true).follow(FollowSymlinks::No);
    let mut file = match directory.open_with(name, &options) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(None);
        }
        Err(_) => {
            return Err(SettingsTransactionError::UnsafePath(
                "settings source refused no-follow open",
            ))
        }
    };
    let metadata = file
        .metadata()
        .map_err(|error| SettingsTransactionError::Io {
            operation: "stat_settings",
            error,
        })?;
    if !metadata.is_file() {
        return Err(SettingsTransactionError::UnsafePath(
            "settings is not a regular file",
        ));
    }
    if metadata.len() > MAX_SETTINGS_DOCUMENT_BYTES as u64 {
        return Err(SettingsPatchError::SizeLimit {
            actual: metadata.len() as usize,
            max: MAX_SETTINGS_DOCUMENT_BYTES,
        }
        .into());
    }
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    (&mut file)
        .take(MAX_SETTINGS_DOCUMENT_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| SettingsTransactionError::Io {
            operation: "read_settings",
            error,
        })?;
    if bytes.len() > MAX_SETTINGS_DOCUMENT_BYTES {
        return Err(SettingsPatchError::SizeLimit {
            actual: bytes.len(),
            max: MAX_SETTINGS_DOCUMENT_BYTES,
        }
        .into());
    }
    let post = file
        .metadata()
        .map_err(|error| SettingsTransactionError::Io {
            operation: "restat_settings",
            error,
        })?;
    if post.len() > MAX_SETTINGS_DOCUMENT_BYTES as u64 {
        return Err(SettingsPatchError::SizeLimit {
            actual: post.len() as usize,
            max: MAX_SETTINGS_DOCUMENT_BYTES,
        }
        .into());
    }
    Ok(Some(bytes))
}

fn replace_atomically(
    directory: &Dir,
    bytes: &[u8],
    source: SourceFingerprint,
    seam: &dyn SettingsTransactionSeam,
) -> Result<SettingsTransactionDurability, SettingsTransactionError> {
    let name = OsStr::new(SETTINGS_RELATIVE_PATH);
    let destination_exists = reject_unsafe_destination(directory, name)?;
    let (temp, mut file) = create_temp_file(directory, name)?;
    let before_rename = (|| -> Result<(), SettingsTransactionError> {
        restrict_file(&file)?;
        seam.checkpoint(SettingsTransactionStage::TempCreated)?;
        file.write_all(bytes)
            .map_err(|error| SettingsTransactionError::Io {
                operation: "write_temp",
                error,
            })?;
        file.sync_all()
            .map_err(|error| SettingsTransactionError::Io {
                operation: "sync_temp",
                error,
            })?;
        seam.checkpoint(SettingsTransactionStage::FileSynced)?;
        seam.checkpoint(SettingsTransactionStage::BeforeRename)?;
        Ok(())
    })();
    drop(file);
    if let Err(error) = before_rename {
        let _ = directory.remove_file(&temp);
        return Err(error);
    }
    let final_source = match source_fingerprint(directory) {
        Ok(fingerprint) => fingerprint,
        Err(error) => {
            let _ = directory.remove_file(&temp);
            return Err(error);
        }
    };
    if final_source != source {
        let _ = directory.remove_file(&temp);
        return Err(SettingsTransactionError::SourceChangedBeforeCommit);
    }
    if let Err(error) = replace_name(directory, &temp, name, destination_exists) {
        let _ = directory.remove_file(&temp);
        return Err(error);
    }
    if seam.checkpoint(SettingsTransactionStage::Renamed).is_err() {
        return Err(SettingsTransactionError::PostRenameUnconfirmed);
    }
    let durability =
        sync_parent(directory).map_err(|_| SettingsTransactionError::PostRenameUnconfirmed)?;
    if seam
        .checkpoint(SettingsTransactionStage::ParentSynced)
        .is_err()
    {
        return Err(SettingsTransactionError::PostRenameUnconfirmed);
    }
    Ok(durability)
}

fn source_fingerprint(directory: &Dir) -> Result<SourceFingerprint, SettingsTransactionError> {
    match read_source_bytes(directory)? {
        Some(bytes) => Ok(SourceFingerprint {
            exists: true,
            len: bytes.len() as u64,
            sha256: Sha256::digest(bytes).into(),
        }),
        None => Ok(SourceFingerprint {
            exists: false,
            len: 0,
            sha256: Sha256::digest([]).into(),
        }),
    }
}

fn reject_unsafe_destination(
    directory: &Dir,
    name: &OsStr,
) -> Result<bool, SettingsTransactionError> {
    let path = Path::new(name);
    let metadata = match directory.symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => {
            return Err(SettingsTransactionError::Io {
                operation: "lstat_settings",
                error,
            });
        }
    };
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(SettingsTransactionError::UnsafePath(
            "settings destination is not a regular file",
        ));
    }
    // Replacement unlinks a hard link rather than writing its inode. It is
    // therefore safe, but importantly never chmods or truncates that target.
    Ok(true)
}

#[cfg(not(windows))]
fn replace_name(
    directory: &Dir,
    temp: &OsStr,
    name: &OsStr,
    _destination_exists: bool,
) -> Result<(), SettingsTransactionError> {
    directory
        .rename(temp, directory, name)
        .map_err(|error| SettingsTransactionError::Io {
            operation: "rename_settings",
            error,
        })
}

#[cfg(windows)]
fn replace_name(
    directory: &Dir,
    temp: &OsStr,
    name: &OsStr,
    destination_exists: bool,
) -> Result<(), SettingsTransactionError> {
    use std::os::windows::ffi::{OsStrExt, OsStringExt};
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Foundation::HANDLE;
    use windows_sys::Win32::Storage::FileSystem::{
        GetFinalPathNameByHandleW, MoveFileExW, ReplaceFileW, FILE_NAME_NORMALIZED,
        MOVEFILE_WRITE_THROUGH,
    };

    let handle = directory
        .try_clone()
        .map_err(|error| SettingsTransactionError::Io {
            operation: "clone_windows_directory",
            error,
        })?
        .into_std_file();
    let raw = handle.as_raw_handle() as HANDLE;
    let needed =
        unsafe { GetFinalPathNameByHandleW(raw, std::ptr::null_mut(), 0, FILE_NAME_NORMALIZED) };
    if needed == 0 {
        return Err(SettingsTransactionError::Io {
            operation: "get_final_directory_path",
            error: std::io::Error::last_os_error(),
        });
    }
    let mut directory_path = vec![0u16; needed as usize + 1];
    let written = unsafe {
        GetFinalPathNameByHandleW(
            raw,
            directory_path.as_mut_ptr(),
            directory_path.len() as u32,
            FILE_NAME_NORMALIZED,
        )
    };
    if written == 0 || written as usize >= directory_path.len() {
        return Err(SettingsTransactionError::Io {
            operation: "get_final_directory_path",
            error: std::io::Error::last_os_error(),
        });
    }
    let directory_path = std::path::PathBuf::from(std::ffi::OsString::from_wide(
        &directory_path[..written as usize],
    ));
    let temp_path = directory_path.join(temp);
    let destination_path = directory_path.join(name);
    let temp_wide: Vec<u16> = temp_path.as_os_str().encode_wide().chain(Some(0)).collect();
    let destination_wide: Vec<u16> = destination_path
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect();
    let ok = unsafe {
        if destination_exists {
            ReplaceFileW(
                destination_wide.as_ptr(),
                temp_wide.as_ptr(),
                std::ptr::null(),
                0,
                std::ptr::null(),
                std::ptr::null(),
            )
        } else {
            // Do not set MOVEFILE_REPLACE_EXISTING here: if a destination
            // appeared after the capability-relative check, fail closed rather
            // than replace a racing writer's new file.
            MoveFileExW(
                temp_wide.as_ptr(),
                destination_wide.as_ptr(),
                MOVEFILE_WRITE_THROUGH,
            )
        }
    };
    if ok == 0 {
        return Err(SettingsTransactionError::Io {
            operation: if destination_exists {
                "replace_file_windows"
            } else {
                "move_file_windows"
            },
            error: std::io::Error::last_os_error(),
        });
    }
    Ok(())
}

fn create_temp_file(
    directory: &Dir,
    name: &OsStr,
) -> Result<(OsString, cap_std::fs::File), SettingsTransactionError> {
    for attempt in 0..16 {
        let temp = temp_sibling(name, attempt);
        let mut options = OpenOptions::new();
        options
            .write(true)
            .create_new(true)
            .follow(FollowSymlinks::No);
        #[cfg(unix)]
        options.mode(0o600);
        match directory.open_with(Path::new(&temp), &options) {
            Ok(file) => return Ok((temp, file)),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => {
                return Err(SettingsTransactionError::Io {
                    operation: "create_temp",
                    error,
                });
            }
        }
    }
    Err(SettingsTransactionError::UnsafePath(
        "unable to allocate unique temporary file",
    ))
}

fn temp_sibling(name: &OsStr, attempt: u32) -> OsString {
    let mut random = [0u8; 8];
    let _ = getrandom::fill(&mut random);
    let suffix = u64::from_le_bytes(random);
    let mut temp = OsString::from(".");
    temp.push(name);
    temp.push(format!(".desktop-settings-{}-{suffix:016x}.tmp", attempt));
    temp
}

#[cfg(unix)]
fn restrict_file(file: &cap_std::fs::File) -> Result<(), SettingsTransactionError> {
    use cap_std::fs::PermissionsExt;
    file.set_permissions(cap_std::fs::Permissions::from_mode(0o600))
        .map_err(|error| SettingsTransactionError::Io {
            operation: "chmod_temp",
            error,
        })
}

#[cfg(not(unix))]
fn restrict_file(_file: &cap_std::fs::File) -> Result<(), SettingsTransactionError> {
    Ok(())
}

#[cfg(target_os = "macos")]
fn sync_parent(directory: &Dir) -> Result<SettingsTransactionDurability, SettingsTransactionError> {
    directory
        .try_clone()
        .and_then(|directory| directory.into_std_file().sync_all())
        .map_err(|error| SettingsTransactionError::Io {
            operation: "sync_parent",
            error,
        })?;
    Ok(SettingsTransactionDurability::Durable)
}

#[cfg(not(target_os = "macos"))]
fn sync_parent(
    _directory: &Dir,
) -> Result<SettingsTransactionDurability, SettingsTransactionError> {
    // Windows has no portable directory fsync through std/cap-std. Linux may
    // also expose an O_PATH descriptor here. Do not claim a durable commit.
    Ok(SettingsTransactionDurability::AtomicReplaceOnly)
}

#[cfg(test)]
mod tests {
    #[cfg(unix)]
    use std::collections::BTreeSet;

    #[cfg(unix)]
    use serde_json::{json, Map, Value};

    use super::*;

    #[cfg(unix)]
    fn root() -> tempfile::TempDir {
        tempfile::Builder::new()
            .prefix("VibeBarLease-")
            .tempdir()
            .unwrap()
    }

    #[cfg(unix)]
    fn fields(value: Value) -> Map<String, Value> {
        value.as_object().unwrap().clone()
    }

    #[cfg(unix)]
    fn patch(base: Value, desired: Value) -> SettingsThreeWayPatch {
        let base = SettingsDocument::from_value(base).unwrap();
        SettingsThreeWayPatch::from_document_and_desired(&base, fields(desired)).unwrap()
    }

    #[cfg(unix)]
    fn authorize(root: &Path) -> SettingsWriterAuthorization {
        SettingsWriterAuthorization::acquire_synthetic(root, "settings-test").unwrap()
    }

    #[cfg(unix)]
    struct FailAt(SettingsTransactionStage);
    #[cfg(unix)]
    impl SettingsTransactionSeam for FailAt {
        fn checkpoint(
            &self,
            stage: SettingsTransactionStage,
        ) -> Result<(), SettingsTransactionError> {
            if stage == self.0 {
                Err(SettingsTransactionError::InjectedFailure(stage))
            } else {
                Ok(())
            }
        }
    }

    #[cfg(unix)]
    struct RewriteBeforeRename {
        path: std::path::PathBuf,
        bytes: Vec<u8>,
    }
    #[cfg(unix)]
    impl SettingsTransactionSeam for RewriteBeforeRename {
        fn checkpoint(
            &self,
            stage: SettingsTransactionStage,
        ) -> Result<(), SettingsTransactionError> {
            if stage == SettingsTransactionStage::BeforeRename {
                std::fs::write(&self.path, &self.bytes).map_err(|error| {
                    SettingsTransactionError::Io {
                        operation: "test_rewrite_source",
                        error,
                    }
                })?;
            }
            Ok(())
        }
    }

    #[cfg(unix)]
    #[test]
    fn legacy_upgrade_preserves_unknown_values_and_uses_0600_atomic_replace() {
        use std::os::unix::fs::PermissionsExt;
        let root = root();
        std::fs::write(
            root.path().join(SETTINGS_RELATIVE_PATH),
            json!({"displayMode":"remaining","future":{"nested":true}}).to_string(),
        )
        .unwrap();
        let authorization = authorize(root.path());
        let result = apply_settings_patch(
            &authorization,
            &patch(
                json!({"displayMode":"remaining","future":{"nested":true}}),
                json!({"displayMode":"used","future":{"nested":true}}),
            ),
        )
        .unwrap();
        assert!(result.patch.write_required);
        assert_eq!(result.patch.document.revision(), 1);
        let value: Value = serde_json::from_slice(
            &std::fs::read(root.path().join(SETTINGS_RELATIVE_PATH)).unwrap(),
        )
        .unwrap();
        assert_eq!(value["schemaVersion"], 1);
        assert_eq!(value["future"]["nested"], true);
        assert_eq!(
            std::fs::metadata(root.path().join(SETTINGS_RELATIVE_PATH))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
    }

    #[cfg(unix)]
    #[test]
    fn conflict_and_idempotency_never_touch_the_file() {
        let root = root();
        let path = root.path().join(SETTINGS_RELATIVE_PATH);
        let original = json!({"schemaVersion":1,"revision":3,"displayMode":"used"}).to_string();
        std::fs::write(&path, &original).unwrap();
        let authorization = authorize(root.path());
        let idempotent = patch(
            json!({"displayMode":"remaining"}),
            json!({"displayMode":"used"}),
        );
        let result = apply_settings_patch(&authorization, &idempotent).unwrap();
        assert!(!result.patch.write_required);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), original);

        let conflict = patch(
            json!({"displayMode":"remaining"}),
            json!({"displayMode":"other"}),
        );
        assert!(matches!(
            apply_settings_patch(&authorization, &conflict),
            Err(SettingsTransactionError::Patch(
                SettingsPatchError::Conflict(_)
            ))
        ));
        assert_eq!(std::fs::read_to_string(&path).unwrap(), original);
    }

    #[cfg(unix)]
    #[test]
    fn pre_rename_failures_clean_temp_and_keep_destination() {
        let root = root();
        let path = root.path().join(SETTINGS_RELATIVE_PATH);
        std::fs::write(&path, json!({"displayMode":"remaining"}).to_string()).unwrap();
        let authorization = authorize(root.path());
        let patch = patch(
            json!({"displayMode":"remaining"}),
            json!({"displayMode":"used"}),
        );
        let error = apply_settings_patch_with_seam(
            &authorization,
            &patch,
            &FailAt(SettingsTransactionStage::FileSynced),
        )
        .unwrap_err();
        assert!(matches!(
            error,
            SettingsTransactionError::InjectedFailure(SettingsTransactionStage::FileSynced)
        ));
        assert_eq!(
            serde_json::from_str::<Value>(&std::fs::read_to_string(&path).unwrap()).unwrap()
                ["displayMode"],
            "remaining"
        );
        let names: BTreeSet<_> = std::fs::read_dir(root.path())
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect();
        assert!(names
            .iter()
            .all(|name| !name.to_string_lossy().contains("desktop-settings")));
    }

    #[cfg(unix)]
    #[test]
    fn same_size_rewrite_before_rename_is_rejected_and_external_value_survives() {
        let root = root();
        let path = root.path().join(SETTINGS_RELATIVE_PATH);
        let initial = br#"{"displayMode":"remaining"}"#;
        let external = br#"{"displayMode":"different"}"#;
        assert_eq!(initial.len(), external.len());
        std::fs::write(&path, initial).unwrap();
        let authorization = authorize(root.path());
        let requested = patch(
            json!({"displayMode":"remaining"}),
            json!({"displayMode":"used"}),
        );
        let error = apply_settings_patch_with_seam(
            &authorization,
            &requested,
            &RewriteBeforeRename {
                path: path.clone(),
                bytes: external.to_vec(),
            },
        )
        .unwrap_err();
        assert!(matches!(
            error,
            SettingsTransactionError::SourceChangedBeforeCommit
        ));
        assert_eq!(std::fs::read(&path).unwrap(), external);
        assert!(std::fs::read_dir(root.path()).unwrap().all(|entry| !entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .contains("desktop-settings")));
    }

    #[cfg(unix)]
    #[test]
    fn destination_created_after_missing_snapshot_is_rejected() {
        let root = root();
        let path = root.path().join(SETTINGS_RELATIVE_PATH);
        let authorization = authorize(root.path());
        let requested = patch(json!({}), json!({"displayMode":"used"}));
        let external = br#"{"displayMode":"remaining"}"#.to_vec();
        let error = apply_settings_patch_with_seam(
            &authorization,
            &requested,
            &RewriteBeforeRename {
                path: path.clone(),
                bytes: external.clone(),
            },
        )
        .unwrap_err();
        assert!(matches!(
            error,
            SettingsTransactionError::SourceChangedBeforeCommit
        ));
        assert_eq!(std::fs::read(&path).unwrap(), external);
    }

    #[cfg(unix)]
    #[test]
    fn final_source_read_failure_cleans_the_temporary_file() {
        let root = root();
        let path = root.path().join(SETTINGS_RELATIVE_PATH);
        std::fs::write(&path, br#"{"displayMode":"remaining"}"#).unwrap();
        let authorization = authorize(root.path());
        let requested = patch(
            json!({"displayMode":"remaining"}),
            json!({"displayMode":"used"}),
        );
        let error = apply_settings_patch_with_seam(
            &authorization,
            &requested,
            &RewriteBeforeRename {
                path,
                bytes: vec![b'x'; MAX_SETTINGS_DOCUMENT_BYTES + 1],
            },
        )
        .unwrap_err();
        assert!(matches!(
            error,
            SettingsTransactionError::Patch(SettingsPatchError::SizeLimit { .. })
        ));
        assert!(std::fs::read_dir(root.path()).unwrap().all(|entry| !entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .contains("desktop-settings")));
    }

    #[cfg(unix)]
    #[test]
    fn post_rename_failure_never_claims_success_or_leaves_a_temp_file() {
        let root = root();
        let path = root.path().join(SETTINGS_RELATIVE_PATH);
        std::fs::write(&path, json!({"displayMode":"remaining"}).to_string()).unwrap();
        let authorization = authorize(root.path());
        let patch = patch(
            json!({"displayMode":"remaining"}),
            json!({"displayMode":"used"}),
        );
        let error = apply_settings_patch_with_seam(
            &authorization,
            &patch,
            &FailAt(SettingsTransactionStage::Renamed),
        )
        .unwrap_err();
        assert!(matches!(
            error,
            SettingsTransactionError::PostRenameUnconfirmed
        ));
        assert_eq!(
            serde_json::from_str::<Value>(&std::fs::read_to_string(&path).unwrap()).unwrap()
                ["displayMode"],
            "used"
        );
        assert!(std::fs::read_dir(root.path()).unwrap().all(|entry| !entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .contains("desktop-settings")));
    }

    #[cfg(unix)]
    #[test]
    fn hard_link_destination_is_replaced_without_mutating_external_inode() {
        let root = root();
        let external = root.path().join("external.json");
        std::fs::write(&external, json!({"displayMode":"remaining"}).to_string()).unwrap();
        std::fs::hard_link(&external, root.path().join(SETTINGS_RELATIVE_PATH)).unwrap();
        let authorization = authorize(root.path());
        let patch = patch(
            json!({"displayMode":"remaining"}),
            json!({"displayMode":"used"}),
        );
        apply_settings_patch(&authorization, &patch).unwrap();
        assert_eq!(
            serde_json::from_str::<Value>(&std::fs::read_to_string(&external).unwrap()).unwrap()
                ["displayMode"],
            "remaining"
        );
    }

    #[cfg(unix)]
    #[test]
    fn rejects_root_and_destination_symlinks() {
        use std::os::unix::fs::symlink;
        let root_dir = root();
        let authorization = authorize(root_dir.path());
        let original = root_dir.path().with_extension("old");
        let redirect = tempfile::tempdir().unwrap();
        std::fs::rename(root_dir.path(), &original).unwrap();
        symlink(redirect.path(), root_dir.path()).unwrap();
        let requested_patch = patch(json!({}), json!({"displayMode":"used"}));
        apply_settings_patch(&authorization, &requested_patch).unwrap();
        assert_eq!(
            serde_json::from_str::<Value>(
                &std::fs::read_to_string(original.join(SETTINGS_RELATIVE_PATH)).unwrap()
            )
            .unwrap()["displayMode"],
            "used"
        );
        assert!(!redirect.path().join(SETTINGS_RELATIVE_PATH).exists());

        let second_root = root();
        let external = tempfile::tempdir().unwrap();
        std::fs::write(external.path().join("outside.json"), "{}").unwrap();
        symlink(
            external.path().join("outside.json"),
            second_root.path().join(SETTINGS_RELATIVE_PATH),
        )
        .unwrap();
        let authorization = authorize(second_root.path());
        assert!(matches!(
            apply_settings_patch(&authorization, &requested_patch),
            Err(SettingsTransactionError::UnsafePath(_))
        ));
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    #[test]
    fn non_macos_reports_atomic_but_not_durable_parent_sync() {
        let root = root();
        let authorization = authorize(root.path());
        let result = apply_settings_patch(
            &authorization,
            &patch(json!({}), json!({"displayMode":"used"})),
        )
        .unwrap();
        assert_eq!(
            result.durability,
            Some(SettingsTransactionDurability::AtomicReplaceOnly)
        );
    }

    #[cfg(windows)]
    #[test]
    fn windows_existing_destination_uses_atomic_replace_without_remove() {
        let root = tempfile::tempdir().unwrap();
        let directory = Dir::open_ambient_dir(root.path(), cap_std::ambient_authority()).unwrap();
        std::fs::write(root.path().join("settings.json"), b"old").unwrap();
        std::fs::write(root.path().join(".settings.json.windows.tmp"), b"new").unwrap();
        replace_name(
            &directory,
            OsStr::new(".settings.json.windows.tmp"),
            OsStr::new("settings.json"),
            true,
        )
        .unwrap();
        assert_eq!(
            std::fs::read(root.path().join("settings.json")).unwrap(),
            b"new"
        );
        assert!(!root.path().join(".settings.json.windows.tmp").exists());
    }
}
