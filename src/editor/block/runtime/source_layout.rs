// @author kongweiguang

use super::*;

/// 大文件 Source 行的稳定布局身份。动态字体、主题、缩放与换行宽度在实际 shape
/// 时补入缓存键；这里保留文档侧不变量，避免 revision 重置或横向窗口变化时复用旧行。
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SourceLayoutIdentity {
    pub(crate) document_epoch: u64,
    pub(crate) document_revision: u64,
    pub(crate) source_range: Range<u64>,
    pub(crate) column_window_start: u64,
    pub(crate) show_line_endings: bool,
}

/// 单个已挂载 Source 行只保留最近一次 shaped layout。整个 Source surface 最多挂载
/// 512 行，因此该缓存天然受 512 行 / 32 MiB 的更严格上限约束。
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct SourceLayoutCacheKey {
    pub(crate) identity: SourceLayoutIdentity,
    pub(crate) text: SharedString,
    pub(crate) marked_range: Option<Range<usize>>,
    pub(crate) theme_identity: usize,
    pub(crate) font: Font,
    pub(crate) font_size_bits: u32,
    pub(crate) line_height_bits: u32,
    pub(crate) scale_bits: u32,
    pub(crate) wrap_width_bits: Option<u32>,
    pub(crate) soft_wrap: bool,
}
