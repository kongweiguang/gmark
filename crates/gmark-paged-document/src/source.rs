// @author kongweiguang

use std::fs::{File, OpenOptions};
use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use lru::LruCache;
use thiserror::Error;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileIdentity {
    pub path: PathBuf,
    pub len: u64,
    pub modified_nanos: Option<u128>,
    pub os_file_id: Option<file_id::FileId>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FileCacheStats {
    pub page_bytes: u64,
    pub max_pages: usize,
    pub resident_pages: usize,
}

#[derive(Debug, Error)]
pub enum PagedDocumentError {
    #[error("failed to access '{path}': {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("invalid byte range {start}..{end} for a {len}-byte file")]
    InvalidRange { start: u64, end: u64, len: u64 },
    #[error("byte range length does not fit this platform")]
    RangeTooLarge,
    #[error("edit range is not on a UTF-8 character boundary")]
    InvalidUtf8Boundary,
    #[error("file is binary or uses an unsupported encoding")]
    Binary,
    #[error("unsupported text encoding '{0}'")]
    UnsupportedEncoding(String),
    #[error("text contains characters that cannot be represented in '{encoding}'")]
    UnrepresentableEncoding { encoding: String },
    #[error("operation was cancelled")]
    Cancelled,
    #[error("invalid JSON near byte {offset}: {message}")]
    InvalidJson { offset: u64, message: String },
    #[error("invalid delimited record near byte {offset}: {message}")]
    InvalidDelimited { offset: u64, message: String },
    #[error("invalid regular expression: {0}")]
    InvalidRegex(String),
    #[error("large-document search failed: {0}")]
    Search(String),
    #[error("invalid source transaction: {0}")]
    InvalidTransaction(String),
    #[error("the source file changed on disk")]
    SourceChanged,
    #[error("failed to atomically replace '{path}': {source}")]
    Persist {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("large-document recovery failed: {0}")]
    Recovery(String),
}

const CACHE_PAGE_BYTES: u64 = 256 * 1024;
const CACHE_PAGE_COUNT: usize = 256;

#[derive(Clone)]
pub struct FileSource {
    path: PathBuf,
    file: Arc<File>,
    opened_len: u64,
    cache: Arc<Mutex<PageCache>>,
}

struct PageCache {
    pages: LruCache<u64, Arc<[u8]>>,
}

impl Default for PageCache {
    fn default() -> Self {
        Self::with_capacity(CACHE_PAGE_COUNT)
    }
}

impl PageCache {
    fn with_capacity(capacity: usize) -> Self {
        let capacity = NonZeroUsize::new(capacity).expect("page cache capacity must be non-zero");
        Self {
            pages: LruCache::new(capacity),
        }
    }

    fn get(&mut self, page_number: u64) -> Option<Arc<[u8]>> {
        self.pages.get(&page_number).cloned()
    }

    fn insert(&mut self, page_number: u64, page: Arc<[u8]>) {
        self.pages.put(page_number, page);
    }

    fn len(&self) -> usize {
        self.pages.len()
    }
}

impl FileSource {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, PagedDocumentError> {
        let path = path.as_ref().to_path_buf();
        let file = open_shared(&path).map_err(|source| PagedDocumentError::Io {
            path: path.clone(),
            source,
        })?;
        let metadata = file.metadata().map_err(|source| PagedDocumentError::Io {
            path: path.clone(),
            source,
        })?;
        if !metadata.is_file() {
            return Err(PagedDocumentError::Io {
                path,
                source: std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    "path is not a regular file",
                ),
            });
        }
        Ok(Self {
            path,
            file: Arc::new(file),
            opened_len: metadata.len(),
            cache: Arc::new(Mutex::new(PageCache::default())),
        })
    }

    pub fn identity(&self) -> Result<FileIdentity, PagedDocumentError> {
        let metadata = std::fs::metadata(&self.path).map_err(|source| PagedDocumentError::Io {
            path: self.path.clone(),
            source,
        })?;
        let modified_nanos = metadata
            .modified()
            .ok()
            .and_then(|value| value.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|value| value.as_nanos());
        let os_file_id = file_id::get_file_id(&self.path).ok();
        // sidecar 与保存冲突判断必须把相对路径、符号链接等别名归一到同一文件身份；
        // 正文路径仍保留用户打开的写法，用于界面和 Save As。
        let canonical_path =
            std::fs::canonicalize(&self.path).unwrap_or_else(|_| self.path.clone());
        Ok(FileIdentity {
            path: canonical_path,
            len: metadata.len(),
            modified_nanos,
            os_file_id,
        })
    }

    pub fn read_range(&self, start: u64, end: u64) -> Result<Vec<u8>, PagedDocumentError> {
        // 读取绑定打开时的稳定句柄与长度；路径 identity 可能已经指向替换后的新文件，
        // 外部变化由专门监控处理，不能让每个视口小读都重复查询 metadata/file-id。
        if start > end || end > self.opened_len {
            return Err(PagedDocumentError::InvalidRange {
                start,
                end,
                len: self.opened_len,
            });
        }
        let len = usize::try_from(end - start).map_err(|_| PagedDocumentError::RangeTooLarge)?;
        let mut output = Vec::with_capacity(len);
        let mut cursor = start;
        while cursor < end {
            let page_number = cursor / CACHE_PAGE_BYTES;
            let page_start = page_number * CACHE_PAGE_BYTES;
            let page = self.page(page_number, page_start, self.opened_len)?;
            let relative_start = usize::try_from(cursor - page_start)
                .map_err(|_| PagedDocumentError::RangeTooLarge)?;
            let take =
                usize::try_from((end - cursor).min(page.len() as u64 - relative_start as u64))
                    .map_err(|_| PagedDocumentError::RangeTooLarge)?;
            output.extend_from_slice(&page[relative_start..relative_start + take]);
            cursor += take as u64;
        }
        Ok(output)
    }

    pub fn read_exact_at(
        &self,
        mut offset: u64,
        mut buffer: &mut [u8],
    ) -> Result<(), PagedDocumentError> {
        while !buffer.is_empty() {
            let read =
                read_at(&self.file, buffer, offset).map_err(|source| PagedDocumentError::Io {
                    path: self.path.clone(),
                    source,
                })?;
            if read == 0 {
                return Err(PagedDocumentError::Io {
                    path: self.path.clone(),
                    source: std::io::Error::new(
                        std::io::ErrorKind::UnexpectedEof,
                        "file changed while it was being read",
                    ),
                });
            }
            offset += read as u64;
            buffer = &mut buffer[read..];
        }
        Ok(())
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn cache_stats(&self) -> FileCacheStats {
        let cache = self
            .cache
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        FileCacheStats {
            page_bytes: CACHE_PAGE_BYTES,
            max_pages: CACHE_PAGE_COUNT,
            resident_pages: cache.len(),
        }
    }

    /// 对指定历史长度的头/中/尾做内容抽样；append 判定必须证明旧前缀仍是同一基线，
    /// 不能只依赖 file id 与“长度变大”。
    pub fn sampled_prefix_hash(&self, prefix_len: u64) -> Result<u32, PagedDocumentError> {
        const SAMPLE_BYTES: u64 = 64 * 1024;
        let current_len = self.identity()?.len;
        if prefix_len > current_len {
            return Err(PagedDocumentError::SourceChanged);
        }
        let mut hasher = crc32fast::Hasher::new();
        for start in [
            0,
            prefix_len.saturating_sub(SAMPLE_BYTES) / 2,
            prefix_len.saturating_sub(SAMPLE_BYTES),
        ] {
            let end = start.saturating_add(SAMPLE_BYTES).min(prefix_len);
            if start < end {
                hasher.update(&self.read_range(start, end)?);
            }
        }
        Ok(hasher.finalize())
    }

    fn page(
        &self,
        page_number: u64,
        page_start: u64,
        file_len: u64,
    ) -> Result<Arc<[u8]>, PagedDocumentError> {
        {
            let mut cache = self
                .cache
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            if let Some(page) = cache.get(page_number) {
                return Ok(page);
            }
        }

        let page_len = usize::try_from((file_len - page_start).min(CACHE_PAGE_BYTES))
            .map_err(|_| PagedDocumentError::RangeTooLarge)?;
        let mut bytes = vec![0; page_len];
        self.read_exact_at(page_start, &mut bytes)?;
        let page: Arc<[u8]> = bytes.into();
        let mut cache = self
            .cache
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        cache.insert(page_number, Arc::clone(&page));
        Ok(page)
    }
}

#[cfg(windows)]
fn open_shared(path: &Path) -> std::io::Result<File> {
    use std::os::windows::fs::OpenOptionsExt;

    // 允许日志写入方追加、轮转或替换文件；当前句柄仍稳定指向打开时的文件对象。
    const FILE_SHARE_READ_WRITE_DELETE: u32 = 0x0000_0001 | 0x0000_0002 | 0x0000_0004;
    OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ_WRITE_DELETE)
        .open(path)
}

#[cfg(not(windows))]
fn open_shared(path: &Path) -> std::io::Result<File> {
    OpenOptions::new().read(true).open(path)
}

#[cfg(unix)]
pub(crate) fn sync_parent_directory(parent: &Path) -> Result<(), PagedDocumentError> {
    File::open(parent)
        .and_then(|directory| directory.sync_all())
        .map_err(|source| PagedDocumentError::Io {
            path: parent.to_path_buf(),
            source,
        })
}

#[cfg(not(unix))]
pub(crate) fn sync_parent_directory(_parent: &Path) -> Result<(), PagedDocumentError> {
    // Windows 没有稳定的目录 fsync 契约；ReplaceFile/rename 与目标句柄 sync 是可用边界。
    Ok(())
}

/// 把同目录、已 fsync 的临时文件原子替换为目标文件。
///
/// Windows 的通用 rename/persist 在目标仍被另一个编辑快照持有时会返回 AccessDenied；
/// MoveFileExW 明确支持 replace-existing + write-through；通过安全封装调用，避免业务
/// crate 引入 unsafe 边界。
#[cfg(windows)]
pub(crate) fn persist_temporary(
    temporary: tempfile::NamedTempFile,
    path: &Path,
) -> Result<(), PagedDocumentError> {
    let temporary_path = temporary.into_temp_path();
    atomicwrites::replace_atomic(&temporary_path, path).map_err(|source| {
        PagedDocumentError::Persist {
            path: path.to_path_buf(),
            source,
        }
    })
}

#[cfg(not(windows))]
pub(crate) fn persist_temporary(
    temporary: tempfile::NamedTempFile,
    path: &Path,
) -> Result<(), PagedDocumentError> {
    let persisted = temporary
        .persist(path)
        .map_err(|error| PagedDocumentError::Persist {
            path: path.to_path_buf(),
            source: error.error,
        })?;
    persisted
        .sync_all()
        .map_err(|source| PagedDocumentError::Io {
            path: path.to_path_buf(),
            source,
        })
}

#[cfg(unix)]
fn read_at(file: &File, buffer: &mut [u8], offset: u64) -> std::io::Result<usize> {
    use std::os::unix::fs::FileExt;
    file.read_at(buffer, offset)
}

#[cfg(test)]
#[path = "../tests/unit/source.rs"]
mod tests;

#[cfg(windows)]
fn read_at(file: &File, buffer: &mut [u8], offset: u64) -> std::io::Result<usize> {
    use std::os::windows::fs::FileExt;
    file.seek_read(buffer, offset)
}
