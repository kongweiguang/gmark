// @author kongweiguang

use std::fmt;

/// 与 UI 框架无关的半开源码 byte range `[start, end)`。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ByteRange {
    start: u64,
    end: u64,
}

impl ByteRange {
    /// 创建一个有序 byte range。UTF-8 边界需要通过 [`Self::validate_for`] 验证。
    pub fn new(start: u64, end: u64) -> Result<Self, ByteRangeError> {
        if start > end {
            return Err(ByteRangeError::Reversed { start, end });
        }
        Ok(Self { start, end })
    }

    /// 以已知源码的 byte offset 创建并验证 range。
    pub fn from_source_offsets(
        source: &str,
        start: usize,
        end: usize,
    ) -> Result<Self, ByteRangeError> {
        let start = u64::try_from(start).map_err(|_| ByteRangeError::PlatformLimit)?;
        let end = u64::try_from(end).map_err(|_| ByteRangeError::PlatformLimit)?;
        let range = Self::new(start, end)?;
        range.validate_for(source)?;
        Ok(range)
    }

    /// 起始 byte offset。
    pub const fn start(self) -> u64 {
        self.start
    }

    /// 结束 byte offset（不包含）。
    pub const fn end(self) -> u64 {
        self.end
    }

    /// range 是否为空。
    pub const fn is_empty(self) -> bool {
        self.start == self.end
    }

    /// 检查 range 是否落在源码内且两端都处于 UTF-8 字符边界。
    pub fn validate_for(self, source: &str) -> Result<(), ByteRangeError> {
        let source_len = u64::try_from(source.len()).map_err(|_| ByteRangeError::PlatformLimit)?;
        if self.end > source_len {
            return Err(ByteRangeError::OutsideSource {
                start: self.start,
                end: self.end,
                source_len,
            });
        }

        let start = usize::try_from(self.start).map_err(|_| ByteRangeError::PlatformLimit)?;
        let end = usize::try_from(self.end).map_err(|_| ByteRangeError::PlatformLimit)?;
        if !source.is_char_boundary(start) {
            return Err(ByteRangeError::InvalidUtf8Boundary { offset: self.start });
        }
        if !source.is_char_boundary(end) {
            return Err(ByteRangeError::InvalidUtf8Boundary { offset: self.end });
        }
        Ok(())
    }

    /// 从已验证范围取得源码片段。
    pub fn slice(self, source: &str) -> Result<&str, ByteRangeError> {
        self.validate_for(source)?;
        let start = usize::try_from(self.start).map_err(|_| ByteRangeError::PlatformLimit)?;
        let end = usize::try_from(self.end).map_err(|_| ByteRangeError::PlatformLimit)?;
        source
            .get(start..end)
            .ok_or(ByteRangeError::InvalidUtf8Boundary { offset: self.start })
    }
}

/// 无法把外部 byte offset 安全投影到 UTF-8 源码时的错误。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ByteRangeError {
    Reversed {
        start: u64,
        end: u64,
    },
    OutsideSource {
        start: u64,
        end: u64,
        source_len: u64,
    },
    InvalidUtf8Boundary {
        offset: u64,
    },
    PlatformLimit,
}

impl fmt::Display for ByteRangeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Reversed { start, end } => write!(formatter, "byte range 无序：{start}..{end}"),
            Self::OutsideSource {
                start,
                end,
                source_len,
            } => write!(
                formatter,
                "byte range 超出源码：{start}..{end}，源码长度为 {source_len}"
            ),
            Self::InvalidUtf8Boundary { offset } => {
                write!(formatter, "byte offset {offset} 不在 UTF-8 字符边界")
            }
            Self::PlatformLimit => formatter.write_str("当前平台无法表示该 byte range"),
        }
    }
}

impl std::error::Error for ByteRangeError {}
