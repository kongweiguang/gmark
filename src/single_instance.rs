// @author kongweiguang

//! Windows single-instance ownership and bounded local file-open forwarding.

use std::fs::{File, OpenOptions};
use std::hash::{Hash, Hasher};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use anyhow::{Context as _, bail};
use fs4::fs_std::FileExt as _;
use futures::channel::mpsc;
use uds_windows::{UnixListener, UnixStream};

const PROTOCOL_MAGIC: [u8; 8] = *b"GMARKI01";
const ACK: u8 = 0x06;
const NACK: u8 = 0x15;
const MAX_PATHS: usize = 64;
const MAX_PATH_BYTES: usize = 32 * 1024;
const MAX_MESSAGE_BYTES: usize = 1024 * 1024;
const ACQUIRE_TIMEOUT: Duration = Duration::from_secs(3);
const IO_TIMEOUT: Duration = Duration::from_secs(2);
const RETRY_DELAY: Duration = Duration::from_millis(25);

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct InstanceMessage {
    pub(crate) paths: Vec<PathBuf>,
}

pub(crate) enum InstanceLaunch {
    Primary {
        guard: InstanceGuard,
        receiver: mpsc::UnboundedReceiver<InstanceMessage>,
    },
    Forwarded,
}

pub(crate) struct InstanceGuard {
    _lock_file: File,
    socket_path: PathBuf,
    shutdown: Arc<AtomicBool>,
    listener_thread: Option<JoinHandle<()>>,
}

impl Drop for InstanceGuard {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::Release);
        if let Some(thread) = self.listener_thread.take() {
            let _ = thread.join();
        }
        let _ = std::fs::remove_file(&self.socket_path);
    }
}

pub(crate) fn acquire(paths: &[PathBuf]) -> anyhow::Result<InstanceLaunch> {
    let dirs = crate::config::GmarkConfigDirs::from_system()?;
    let installation_id = crate::config::load_or_create_installation_id()?;
    let socket_path = instance_socket_path(
        installation_id,
        std::env::var_os("GMARK_UI_CHECK_CONFIG_ROOT"),
    );
    acquire_with_paths(&dirs.instance_lock_file(), &socket_path, paths)
}

fn instance_socket_path(
    installation_id: uuid::Uuid,
    ui_check_root: Option<std::ffi::OsString>,
) -> PathBuf {
    // Windows AF_UNIX sockaddr has a short fixed path budget; keep the endpoint flat in %TEMP%.
    // UI screenshot processes must never forward files to a user's real instance, even when a
    // pre-existing installation identifier was copied into the temporary configuration root.
    let suffix = ui_check_root
        .filter(|root| !root.is_empty())
        .map(|root| {
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            root.hash(&mut hasher);
            format!("-{:016x}", hasher.finish())
        })
        .unwrap_or_default();
    std::env::temp_dir().join(format!("gmi-{}{suffix}.sock", installation_id.simple()))
}

fn acquire_with_paths(
    lock_path: &Path,
    socket_path: &Path,
    paths: &[PathBuf],
) -> anyhow::Result<InstanceLaunch> {
    if let Some(parent) = lock_path.parent() {
        std::fs::create_dir_all(parent).with_context(|| {
            format!("failed to create instance directory '{}'", parent.display())
        })?;
    }
    let lock_file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(lock_path)
        .with_context(|| format!("failed to open instance lock '{}'", lock_path.display()))?;
    let deadline = Instant::now() + ACQUIRE_TIMEOUT;

    loop {
        if lock_file
            .try_lock_exclusive()
            .with_context(|| format!("failed to lock '{}'", lock_path.display()))?
        {
            return start_primary(lock_file, socket_path);
        }
        match forward_to_primary(socket_path, paths) {
            Ok(()) => return Ok(InstanceLaunch::Forwarded),
            Err(error) if Instant::now() < deadline => {
                let _ = error;
                std::thread::sleep(RETRY_DELAY);
            }
            Err(error) => {
                return Err(error).with_context(|| {
                    format!(
                        "the primary gmark instance did not accept IPC at '{}'",
                        socket_path.display()
                    )
                });
            }
        }
    }
}

fn start_primary(lock_file: File, socket_path: &Path) -> anyhow::Result<InstanceLaunch> {
    match std::fs::remove_file(socket_path) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(error).with_context(|| {
                format!("failed to remove stale IPC '{}'", socket_path.display())
            });
        }
    }
    let listener = UnixListener::bind(socket_path)
        .with_context(|| format!("failed to bind IPC '{}'", socket_path.display()))?;
    listener
        .set_nonblocking(true)
        .context("failed to configure nonblocking instance IPC")?;
    let (sender, receiver) = mpsc::unbounded();
    let shutdown = Arc::new(AtomicBool::new(false));
    let thread_shutdown = shutdown.clone();
    let listener_thread = std::thread::Builder::new()
        .name("gmark-instance-ipc".to_owned())
        .spawn(move || run_listener(listener, sender, thread_shutdown))
        .context("failed to spawn instance IPC listener")?;
    Ok(InstanceLaunch::Primary {
        guard: InstanceGuard {
            _lock_file: lock_file,
            socket_path: socket_path.to_path_buf(),
            shutdown,
            listener_thread: Some(listener_thread),
        },
        receiver,
    })
}

fn run_listener(
    listener: UnixListener,
    sender: mpsc::UnboundedSender<InstanceMessage>,
    shutdown: Arc<AtomicBool>,
) {
    while !shutdown.load(Ordering::Acquire) {
        let mut stream = match listener.accept() {
            Ok((stream, _address)) => stream,
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(RETRY_DELAY);
                continue;
            }
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(_) => {
                std::thread::sleep(RETRY_DELAY);
                continue;
            }
        };
        if shutdown.load(Ordering::Acquire) {
            break;
        }
        let _ = stream.set_read_timeout(Some(IO_TIMEOUT));
        let _ = stream.set_write_timeout(Some(IO_TIMEOUT));
        let accepted = read_message(&mut stream)
            .ok()
            .is_some_and(|message| sender.unbounded_send(message).is_ok());
        if accepted {
            let _ = stream.write_all(&[ACK]);
        } else {
            let _ = stream.write_all(&[NACK]);
        }
    }
}

fn forward_to_primary(socket_path: &Path, paths: &[PathBuf]) -> anyhow::Result<()> {
    let mut stream = UnixStream::connect(socket_path)
        .with_context(|| format!("failed to connect IPC '{}'", socket_path.display()))?;
    stream.set_read_timeout(Some(IO_TIMEOUT))?;
    stream.set_write_timeout(Some(IO_TIMEOUT))?;
    write_message(&mut stream, paths)?;
    let mut response = [0u8; 1];
    stream
        .read_exact(&mut response)
        .context("primary instance closed IPC before acknowledgement")?;
    if response != [ACK] {
        bail!("primary instance rejected the IPC request");
    }
    Ok(())
}

fn write_message(mut writer: impl Write, paths: &[PathBuf]) -> anyhow::Result<()> {
    if paths.len() > MAX_PATHS {
        bail!("IPC request exceeds the {MAX_PATHS} path limit");
    }
    let encoded = paths
        .iter()
        .map(|path| {
            path.to_str().map(str::as_bytes).ok_or_else(|| {
                anyhow::anyhow!("IPC path is not valid Unicode: '{}'", path.display())
            })
        })
        .collect::<anyhow::Result<Vec<_>>>()?;
    let total = encoded
        .iter()
        .try_fold(PROTOCOL_MAGIC.len() + 4, |total, path| {
            if path.len() > MAX_PATH_BYTES {
                bail!("IPC path exceeds the {MAX_PATH_BYTES} byte limit");
            }
            total
                .checked_add(4 + path.len())
                .ok_or_else(|| anyhow::anyhow!("IPC message size overflow"))
        })?;
    if total > MAX_MESSAGE_BYTES {
        bail!("IPC request exceeds the {MAX_MESSAGE_BYTES} byte limit");
    }

    writer.write_all(&PROTOCOL_MAGIC)?;
    writer.write_all(&(encoded.len() as u32).to_le_bytes())?;
    for path in encoded {
        writer.write_all(&(path.len() as u32).to_le_bytes())?;
        writer.write_all(path)?;
    }
    writer.flush()?;
    Ok(())
}

fn read_message(mut reader: impl Read) -> anyhow::Result<InstanceMessage> {
    let mut magic = [0u8; PROTOCOL_MAGIC.len()];
    reader.read_exact(&mut magic)?;
    if magic != PROTOCOL_MAGIC {
        bail!("unsupported IPC protocol");
    }
    let count = read_u32(&mut reader)? as usize;
    if count > MAX_PATHS {
        bail!("IPC request exceeds the {MAX_PATHS} path limit");
    }
    let mut total = PROTOCOL_MAGIC.len() + 4;
    let mut paths = Vec::with_capacity(count);
    for _ in 0..count {
        let len = read_u32(&mut reader)? as usize;
        if len > MAX_PATH_BYTES {
            bail!("IPC path exceeds the {MAX_PATH_BYTES} byte limit");
        }
        total = total
            .checked_add(4 + len)
            .ok_or_else(|| anyhow::anyhow!("IPC message size overflow"))?;
        if total > MAX_MESSAGE_BYTES {
            bail!("IPC request exceeds the {MAX_MESSAGE_BYTES} byte limit");
        }
        let mut bytes = vec![0; len];
        reader.read_exact(&mut bytes)?;
        let path = String::from_utf8(bytes).context("IPC path is not valid UTF-8")?;
        paths.push(PathBuf::from(path));
    }
    Ok(InstanceMessage { paths })
}

fn read_u32(reader: &mut impl Read) -> std::io::Result<u32> {
    let mut bytes = [0u8; 4];
    reader.read_exact(&mut bytes)?;
    Ok(u32::from_le_bytes(bytes))
}

#[cfg(test)]
#[path = "../tests/unit/single_instance.rs"]
mod tests;
