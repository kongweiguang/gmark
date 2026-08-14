// @author kongweiguang

//! Windows single-instance ownership and bounded local file-open forwarding.
//!
//! Windows AF_UNIX endpoints must fit in the 108-byte `sun_path` buffer, including its NUL
//! terminator. We validate that `SUN_LEN` invariant before binding the endpoint under
//! `~/.gmark/runtime`.

use std::collections::{HashSet, VecDeque};
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU64, Ordering},
    mpsc as std_mpsc,
};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use anyhow::{Context as _, bail};
use fs4::fs_std::FileExt as _;
use futures::channel::mpsc;
use gmark_config::{AppDirs, load_or_create_installation_id_with_dirs};
use sha2::{Digest as _, Sha256};
use uds_windows::{UnixListener, UnixStream};

const PROTOCOL_MAGIC: [u8; 8] = *b"GMARKI02";
const ACK: u8 = 0x06;
const NACK: u8 = 0x15;
const MAX_PATHS: usize = 64;
const MAX_PATH_BYTES: usize = 32 * 1024;
const MAX_MESSAGE_BYTES: usize = 1024 * 1024;
const ACQUIRE_TIMEOUT: Duration = Duration::from_secs(3);
const IO_TIMEOUT: Duration = Duration::from_secs(2);
const RETRY_DELAY: Duration = Duration::from_millis(25);
const WINDOWS_AF_UNIX_MAX_PATH_BYTES: usize = 107;
const SOCKET_ID_HASH_BYTES: usize = 12;
const MAX_COMPLETED_REQUEST_IDS: usize = 256;
const UI_ACCEPT_TIMEOUT: Duration = Duration::from_secs(3);

static NEXT_REQUEST_SEQUENCE: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct InstanceMessage {
    pub(crate) request_id: u64,
    pub(crate) paths: Vec<PathBuf>,
}

pub(crate) struct InstanceRequest {
    pub(crate) message: InstanceMessage,
    acknowledgement: std_mpsc::Sender<bool>,
}

impl InstanceRequest {
    /// UI 完成接收后才回传结果，避免 IPC 客户端把“进入队列”误认为“已处理”。
    // 原因：只有 GPUI 回调成功返回，转发方才可以安全结束本次启动请求。
    pub(crate) fn respond(self, accepted: bool) {
        let _ = self.acknowledgement.send(accepted);
    }
}

pub(crate) enum InstanceLaunch {
    Primary {
        guard: InstanceGuard,
        receiver: mpsc::UnboundedReceiver<InstanceRequest>,
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
    // 运行时根目录承载敏感的实例锁；不能回退到配置目录，也不能读取旧验收
    // 环境变量，否则验收实例会与真实进程共享锁语义。
    let dirs = AppDirs::from_system()?;
    dirs.ensure_runtime_root()?;
    let installation_id = load_or_create_installation_id_with_dirs(&dirs)?;
    let socket_path = instance_socket_path(dirs.runtime_root(), installation_id)?;
    acquire_with_paths(&dirs.instance_lock_file(), &socket_path, paths)
}

fn instance_socket_path(
    runtime_root: &Path,
    installation_id: uuid::Uuid,
) -> anyhow::Result<PathBuf> {
    let socket_path = runtime_root.join(instance_socket_file_name(installation_id));
    validate_socket_path(&socket_path).with_context(|| {
        format!(
            "cannot use Gmark runtime root '{}' for the Windows AF_UNIX IPC endpoint",
            runtime_root.display()
        )
    })?;
    Ok(socket_path)
}

fn instance_socket_file_name(installation_id: uuid::Uuid) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"gmark-instance-socket-v1\0");
    hasher.update(installation_id.as_bytes());
    let digest = hasher.finalize();
    format!("gmi-{}.sock", hash_prefix(&digest))
}

fn hash_prefix(digest: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";

    let mut hash = String::with_capacity(SOCKET_ID_HASH_BYTES * 2);
    for byte in digest.iter().take(SOCKET_ID_HASH_BYTES) {
        hash.push(char::from(HEX[usize::from(byte >> 4)]));
        hash.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    hash
}

fn validate_socket_path(path: &Path) -> anyhow::Result<()> {
    let path = path.to_str().ok_or_else(|| {
        anyhow::anyhow!(
            "Windows AF_UNIX IPC endpoint '{}' is not valid UTF-8",
            path.display()
        )
    })?;
    let bytes = path.as_bytes();
    if bytes.len() > WINDOWS_AF_UNIX_MAX_PATH_BYTES {
        bail!(
            "Windows AF_UNIX IPC endpoint '{path}' is {} UTF-8 bytes; SUN_LEN allows at most \
             {WINDOWS_AF_UNIX_MAX_PATH_BYTES}",
            bytes.len()
        );
    }
    if bytes.contains(&0) {
        bail!("Windows AF_UNIX IPC endpoint contains a NUL byte");
    }
    Ok(())
}

fn acquire_with_paths(
    lock_path: &Path,
    socket_path: &Path,
    paths: &[PathBuf],
) -> anyhow::Result<InstanceLaunch> {
    let mut lock_options = OpenOptions::new();
    lock_options
        .read(true)
        .write(true)
        .create(true)
        .truncate(false);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        lock_options.mode(0o600);
    }
    let lock_file = lock_options
        .open(lock_path)
        .with_context(|| format!("failed to open instance lock '{}'", lock_path.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        let mut permissions = lock_file
            .metadata()
            .with_context(|| format!("failed to inspect instance lock '{}'", lock_path.display()))?
            .permissions();
        permissions.set_mode(0o600);
        lock_file.set_permissions(permissions).with_context(|| {
            format!("failed to protect instance lock '{}'", lock_path.display())
        })?;
    }
    let deadline = Instant::now() + ACQUIRE_TIMEOUT;
    let request_id = next_request_id();

    loop {
        if lock_file
            .try_lock_exclusive()
            .with_context(|| format!("failed to lock '{}'", lock_path.display()))?
        {
            return start_primary(lock_file, socket_path);
        }
        match forward_to_primary(socket_path, request_id, paths) {
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
    validate_socket_path(socket_path)?;
    if let Some(parent) = socket_path.parent() {
        std::fs::create_dir_all(parent).with_context(|| {
            format!(
                "failed to prepare instance IPC directory '{}'",
                parent.display()
            )
        })?;
    }
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
    sender: mpsc::UnboundedSender<InstanceRequest>,
    shutdown: Arc<AtomicBool>,
) {
    let mut completed_ids = HashSet::new();
    let mut completed_order = VecDeque::new();
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
        let accepted = match read_message(&mut stream) {
            Ok(message) if completed_ids.contains(&message.request_id) => true,
            Ok(message) => {
                let request_id = message.request_id;
                let (acknowledgement, result) = std_mpsc::channel();
                if sender
                    .unbounded_send(InstanceRequest {
                        message,
                        acknowledgement,
                    })
                    .is_err()
                {
                    false
                } else if result.recv_timeout(UI_ACCEPT_TIMEOUT).unwrap_or(false) {
                    remember_completed_request(
                        &mut completed_ids,
                        &mut completed_order,
                        request_id,
                    );
                    true
                } else {
                    false
                }
            }
            Err(_) => false,
        };
        if accepted {
            let _ = stream.write_all(&[ACK]);
        } else {
            let _ = stream.write_all(&[NACK]);
        }
    }
}

fn forward_to_primary(
    socket_path: &Path,
    request_id: u64,
    paths: &[PathBuf],
) -> anyhow::Result<()> {
    let mut stream = UnixStream::connect(socket_path)
        .with_context(|| format!("failed to connect IPC '{}'", socket_path.display()))?;
    stream.set_read_timeout(Some(IO_TIMEOUT))?;
    stream.set_write_timeout(Some(IO_TIMEOUT))?;
    write_message(
        &mut stream,
        &InstanceMessage {
            request_id,
            paths: paths.to_vec(),
        },
    )?;
    let mut response = [0u8; 1];
    stream
        .read_exact(&mut response)
        .context("primary instance closed IPC before acknowledgement")?;
    if response != [ACK] {
        bail!("primary instance rejected the IPC request");
    }
    Ok(())
}

fn write_message(mut writer: impl Write, message: &InstanceMessage) -> anyhow::Result<()> {
    let paths = &message.paths;
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
        .try_fold(PROTOCOL_MAGIC.len() + 8 + 4, |total, path| {
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
    writer.write_all(&message.request_id.to_le_bytes())?;
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
    let request_id = read_u64(&mut reader)?;
    let count = read_u32(&mut reader)? as usize;
    if count > MAX_PATHS {
        bail!("IPC request exceeds the {MAX_PATHS} path limit");
    }
    let mut total = PROTOCOL_MAGIC.len() + 8 + 4;
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
    Ok(InstanceMessage { request_id, paths })
}

fn read_u64(reader: &mut impl Read) -> std::io::Result<u64> {
    let mut bytes = [0; 8];
    reader.read_exact(&mut bytes)?;
    Ok(u64::from_le_bytes(bytes))
}

/// 为每次二次启动生成稳定于重试周期的 ID，避免 ACK 丢失导致重复派发。
// 原因：同一进程的重试必须复用 ID，而不同进程仍需尽量避免碰撞。
fn next_request_id() -> u64 {
    let sequence = NEXT_REQUEST_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    (u64::from(std::process::id()) << 32) | (sequence & u64::from(u32::MAX))
}

/// 只保留最近一小段已接受请求，既能去重重试又不会让常驻进程无界增长。
// 原因：IPC 请求可能无限到达，去重状态必须有明确的内存上限。
fn remember_completed_request(
    completed_ids: &mut HashSet<u64>,
    completed_order: &mut VecDeque<u64>,
    request_id: u64,
) {
    if !completed_ids.insert(request_id) {
        return;
    }
    completed_order.push_back(request_id);
    while completed_order.len() > MAX_COMPLETED_REQUEST_IDS {
        if let Some(expired) = completed_order.pop_front() {
            completed_ids.remove(&expired);
        }
    }
}

fn read_u32(reader: &mut impl Read) -> std::io::Result<u32> {
    let mut bytes = [0u8; 4];
    reader.read_exact(&mut bytes)?;
    Ok(u32::from_le_bytes(bytes))
}

#[cfg(test)]
#[path = "../../tests/unit/single_instance.rs"]
mod tests;
