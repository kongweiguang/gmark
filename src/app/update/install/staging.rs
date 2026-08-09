// @author kongweiguang

//! Guarded staging and launch verification for the helper and feedback agent.

use sha2::{Digest as _, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read as _, Write as _},
    path::{Path, PathBuf},
};
use uuid::Uuid;

const MAX_STAGED_HELPER_BYTES: u64 = 128 * 1024 * 1024;

#[derive(Clone)]
pub(crate) struct StagedHelper {
    pub(crate) path: PathBuf,
    pub(crate) length: u64,
    pub(crate) digest: [u8; 32],
}

/// The standalone feedback agent uses the same immutable staging and hash
/// verification rules as the helper executable.
pub(crate) type StagedAgent = StagedHelper;

/// On Windows the open directory and image handles deny replacement until the
/// process has resolved the staged executable.  Unix rehashes the file before
/// spawn as the equivalent second verification boundary.
pub(crate) struct StagedHelperLaunchGuard {
    #[cfg(windows)]
    _directory: File,
    #[cfg(windows)]
    _file: File,
}

pub(crate) fn stage_update_helper(
    transaction_dir: &Path,
    installed_helper: &Path,
) -> Result<StagedHelper, String> {
    harden_transaction_directory(transaction_dir)?;
    let helper_name = if cfg!(windows) {
        format!("gmark-update-helper-copy-{}.exe", Uuid::new_v4())
    } else {
        format!("gmark-update-helper-copy-{}", Uuid::new_v4())
    };
    let path = transaction_dir.join(helper_name);
    let (length, digest) = copy_helper_exclusive(installed_helper, &path)?;
    harden_staged_helper(&path)?;
    let helper = StagedHelper {
        path,
        length,
        digest,
    };
    verify_staged_helper_for_launch(&helper).map(|_| helper)
}

pub(crate) fn stage_update_agent(
    transaction_dir: &Path,
    installed_agent: &Path,
) -> Result<StagedAgent, String> {
    harden_transaction_directory(transaction_dir)?;
    let agent_name = if cfg!(windows) {
        "gmark-update-agent.exe"
    } else {
        "gmark-update-agent"
    };
    let path = transaction_dir.join(agent_name);
    let (length, digest) = copy_helper_exclusive(installed_agent, &path)?;
    harden_staged_helper(&path)?;
    let agent = StagedAgent {
        path,
        length,
        digest,
    };
    verify_staged_helper_for_launch(&agent).map(|_| agent)
}

pub(crate) fn verify_staged_helper_for_launch(
    helper: &StagedHelper,
) -> Result<StagedHelperLaunchGuard, String> {
    let metadata = fs::symlink_metadata(&helper.path)
        .map_err(|error| format!("failed to inspect staged helper: {error}"))?;
    if !is_real_regular_file(&metadata) {
        return Err("staged helper is not a regular file".to_owned());
    }
    if metadata.len() != helper.length {
        return Err("staged helper changed after verification".to_owned());
    }

    #[cfg(windows)]
    let directory = {
        use std::os::windows::fs::OpenOptionsExt as _;

        let transaction_dir = helper
            .path
            .parent()
            .ok_or_else(|| "staged helper has no transaction directory".to_owned())?;
        let directory_metadata = fs::symlink_metadata(transaction_dir)
            .map_err(|error| format!("failed to inspect staged helper directory: {error}"))?;
        if !is_real_directory(&directory_metadata) {
            return Err("staged helper directory is not a real directory".to_owned());
        }
        const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;
        const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
        const FILE_SHARE_READ: u32 = 0x0000_0001;
        let directory = OpenOptions::new()
            .read(true)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
            .share_mode(FILE_SHARE_READ)
            .open(transaction_dir)
            .map_err(|error| format!("failed to lock staged helper directory: {error}"))?;
        if !is_real_directory(
            &directory
                .metadata()
                .map_err(|error| format!("failed to verify staged helper directory: {error}"))?,
        ) {
            return Err("opened staged helper directory is not a real directory".to_owned());
        }
        directory
    };
    #[cfg(windows)]
    let file = {
        use std::os::windows::fs::OpenOptionsExt as _;

        const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
        const FILE_SHARE_READ: u32 = 0x0000_0001;
        let file = OpenOptions::new()
            .read(true)
            .share_mode(FILE_SHARE_READ)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
            .open(&helper.path)
            .map_err(|error| format!("failed to lock staged helper for launch: {error}"))?;
        let opened = file
            .metadata()
            .map_err(|error| format!("failed to verify staged helper handle: {error}"))?;
        if !is_real_regular_file(&opened) || opened.len() != helper.length {
            return Err("opened staged helper is not the verified regular file".to_owned());
        }
        file
    };
    #[cfg(not(windows))]
    let file = File::open(&helper.path)
        .map_err(|error| format!("failed to open staged helper for launch: {error}"))?;

    #[cfg(windows)]
    let mut hash_file = file
        .try_clone()
        .map_err(|error| format!("failed to clone staged helper handle: {error}"))?;
    #[cfg(not(windows))]
    let mut hash_file = file;
    if hash_file_exact(&mut hash_file, helper.length, "staged helper")? != helper.digest {
        return Err("staged helper changed after verification".to_owned());
    }
    Ok(StagedHelperLaunchGuard {
        #[cfg(windows)]
        _directory: directory,
        #[cfg(windows)]
        _file: file,
    })
}

pub(crate) fn copy_verified_artifact(
    source: &Path,
    destination: &Path,
    expected_length: u64,
    expected_digest: &str,
) -> Result<(), String> {
    let metadata = fs::symlink_metadata(source)
        .map_err(|error| format!("failed to inspect verified update artifact: {error}"))?;
    if !is_real_regular_file(&metadata) {
        return Err("verified update artifact is not a regular non-link file".to_owned());
    }
    let (length, digest) = copy_helper_exclusive(source, destination)?;
    if length != expected_length {
        let _ = fs::remove_file(destination);
        return Err("verified update artifact changed while preparing apply attempt".to_owned());
    }
    let actual = digest
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    if !actual.eq_ignore_ascii_case(expected_digest) {
        let _ = fs::remove_file(destination);
        return Err(
            "verified update artifact digest changed while preparing apply attempt".to_owned(),
        );
    }
    Ok(())
}

pub(crate) fn copy_regular_file(source: &Path, destination: &Path) -> Result<(), String> {
    let metadata = fs::symlink_metadata(source)
        .map_err(|error| format!("failed to inspect staged update manifest: {error}"))?;
    if !is_real_regular_file(&metadata) {
        return Err("staged update manifest is not a regular file".to_owned());
    }
    let mut input = File::open(source)
        .map_err(|error| format!("failed to open staged update manifest: {error}"))?;
    let mut output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(destination)
        .map_err(|error| format!("failed to create staged update manifest: {error}"))?;
    match std::io::copy(&mut input, &mut output).and_then(|_| output.sync_all()) {
        Ok(()) => Ok(()),
        Err(error) => {
            drop(output);
            let _ = fs::remove_file(destination);
            Err(format!("failed to persist staged update manifest: {error}"))
        }
    }
}

fn harden_transaction_directory(transaction_dir: &Path) -> Result<(), String> {
    let metadata = fs::symlink_metadata(transaction_dir)
        .map_err(|error| format!("failed to inspect update transaction directory: {error}"))?;
    if !is_real_directory(&metadata) {
        return Err("update transaction directory is not a real directory".to_owned());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        fs::set_permissions(transaction_dir, fs::Permissions::from_mode(0o700))
            .map_err(|error| format!("failed to secure update transaction directory: {error}"))?;
    }
    Ok(())
}

fn copy_helper_exclusive(source: &Path, destination: &Path) -> Result<(u64, [u8; 32]), String> {
    let source_link_metadata = fs::symlink_metadata(source).map_err(|error| {
        format!(
            "failed to inspect installed update helper '{}': {error}",
            source.display()
        )
    })?;
    if !is_real_regular_file(&source_link_metadata) {
        return Err("installed update helper is not a regular non-link file".to_owned());
    }
    let mut source_file = File::open(source).map_err(|error| {
        format!(
            "failed to open installed update helper '{}': {error}",
            source.display()
        )
    })?;
    let source_metadata = source_file
        .metadata()
        .map_err(|error| format!("failed to inspect installed update helper: {error}"))?;
    let expected_length = source_metadata.len();
    if !source_metadata.is_file()
        || expected_length == 0
        || expected_length > MAX_STAGED_HELPER_BYTES
    {
        return Err("installed update helper is not a bounded regular file".to_owned());
    }
    let mut destination_file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(destination)
        .map_err(|error| format!("failed to create staged update helper: {error}"))?;
    let mut hasher = Sha256::new();
    let mut total = 0_u64;
    let mut buffer = [0_u8; 64 * 1024];
    let copy_result = (|| -> Result<(), String> {
        loop {
            let read = source_file
                .read(&mut buffer)
                .map_err(|error| format!("failed to read installed update helper: {error}"))?;
            if read == 0 {
                break;
            }
            total = total
                .checked_add(read as u64)
                .ok_or_else(|| "installed update helper is too large".to_owned())?;
            if total > expected_length || total > MAX_STAGED_HELPER_BYTES {
                return Err("installed update helper changed while staging".to_owned());
            }
            destination_file
                .write_all(&buffer[..read])
                .map_err(|error| format!("failed to stage update helper: {error}"))?;
            hasher.update(&buffer[..read]);
        }
        if total != expected_length {
            return Err("installed update helper changed while staging".to_owned());
        }
        destination_file
            .sync_all()
            .map_err(|error| format!("failed to persist staged update helper: {error}"))
    })();
    if let Err(error) = copy_result {
        drop(destination_file);
        let _ = fs::remove_file(destination);
        return Err(error);
    }
    Ok((total, hasher.finalize().into()))
}

fn harden_staged_helper(path: &Path) -> Result<(), String> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| format!("failed to inspect staged helper: {error}"))?;
    if !is_real_regular_file(&metadata) {
        return Err("staged helper is not a regular file".to_owned());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        fs::set_permissions(path, fs::Permissions::from_mode(0o500))
            .map_err(|error| format!("failed to secure staged helper: {error}"))?;
    }
    Ok(())
}

pub(crate) fn is_real_regular_file(metadata: &fs::Metadata) -> bool {
    metadata.file_type().is_file()
        && !metadata.file_type().is_symlink()
        && !is_windows_reparse_point(metadata)
}

pub(crate) fn is_real_directory(metadata: &fs::Metadata) -> bool {
    metadata.file_type().is_dir()
        && !metadata.file_type().is_symlink()
        && !is_windows_reparse_point(metadata)
}

fn is_windows_reparse_point(metadata: &fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt as _;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0000_0400;
        metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
    }
    #[cfg(not(windows))]
    {
        let _ = metadata;
        false
    }
}

fn hash_file_exact(file: &mut File, expected_length: u64, label: &str) -> Result<[u8; 32], String> {
    let mut hasher = Sha256::new();
    let mut total = 0_u64;
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|error| format!("failed to read {label}: {error}"))?;
        if read == 0 {
            break;
        }
        total = total
            .checked_add(read as u64)
            .ok_or_else(|| format!("{label} is too large"))?;
        if total > expected_length || total > MAX_STAGED_HELPER_BYTES {
            return Err(format!("{label} changed after verification"));
        }
        hasher.update(&buffer[..read]);
    }
    if total != expected_length {
        return Err(format!("{label} changed after verification"));
    }
    Ok(hasher.finalize().into())
}

pub(crate) fn installed_helper_path() -> Result<PathBuf, String> {
    let current = std::env::current_exe()
        .map_err(|error| format!("failed to locate current executable: {error}"))?;
    let parent = current
        .parent()
        .ok_or_else(|| "current executable has no parent directory".to_owned())?;
    let local = parent.join(if cfg!(windows) {
        "gmark-update-helper.exe"
    } else {
        "gmark-update-helper"
    });
    if local.is_file() {
        return Ok(local);
    }
    #[cfg(target_os = "macos")]
    {
        let bundled = parent.join("../Helpers/gmark-update-helper");
        if bundled.is_file() {
            return Ok(bundled);
        }
    }
    #[cfg(target_os = "linux")]
    if let Some(app_dir) = std::env::var_os("APPDIR") {
        let bundled = PathBuf::from(app_dir).join("usr/lib/gmark/gmark-update-helper");
        if bundled.is_file() {
            return Ok(bundled);
        }
    }
    Err("this installation does not include gmark-update-helper".to_owned())
}

pub(crate) fn installed_agent_path() -> Result<PathBuf, String> {
    let current = std::env::current_exe()
        .map_err(|error| format!("failed to locate current executable: {error}"))?;
    let parent = current
        .parent()
        .ok_or_else(|| "current executable has no parent directory".to_owned())?;
    let local = parent.join(if cfg!(windows) {
        "gmark-update-agent.exe"
    } else {
        "gmark-update-agent"
    });
    if local.is_file() {
        return Ok(local);
    }
    #[cfg(target_os = "macos")]
    {
        let bundled = parent.join("../Helpers/gmark-update-agent");
        if bundled.is_file() {
            return Ok(bundled);
        }
    }
    #[cfg(target_os = "linux")]
    if let Some(app_dir) = std::env::var_os("APPDIR") {
        let bundled = PathBuf::from(app_dir).join("usr/lib/gmark/gmark-update-agent");
        if bundled.is_file() {
            return Ok(bundled);
        }
    }
    Err("this installation does not include gmark-update-agent".to_owned())
}
