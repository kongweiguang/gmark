// @author kongweiguang

//! Background-prepared Markdown projection input.

use std::ops::Range;
use std::sync::Arc;

use gmark_document::{DocumentSnapshot, Revision};

use super::document::{prepare_projection_nodes, scan_projection_regions};
use crate::components::BlockRecord;

/// 不含 GPUI Entity 的可递归块节点，可由后台线程完整构造。
#[derive(Clone, Debug)]
pub(super) struct PreparedBlockNode {
    pub(super) record: BlockRecord,
    pub(super) children: Vec<PreparedBlockNode>,
}

impl PreparedBlockNode {
    pub(super) fn leaf(record: BlockRecord) -> Self {
        Self {
            record,
            children: Vec::new(),
        }
    }
}

/// 后台扫描得到的顶层 Markdown 区域类型；这里只描述边界，不持有 UI 状态。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ProjectionRegionKind {
    Blank,
    Frontmatter,
    FencedCode,
    Comment,
    Html,
    FootnoteDefinition,
    ReferenceDefinition,
    SetextHeading,
    StandaloneImage,
    IndentedCode,
    List,
    Quote,
    AtxHeading,
    Separator,
    RootTableCandidate,
    PipelessTable,
    DisplayMath,
    Paragraph,
}

/// 源码中的连续顶层区域。字节范围使用 UTF-8 偏移且不包含区域后的换行符。
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct ProjectionRegion {
    pub(super) kind: ProjectionRegionKind,
    pub(super) lines: Range<usize>,
    pub(super) bytes: Range<usize>,
}

/// 不包含 GPUI Entity 的区域级投影输入，可安全在后台线程生成和增量复用。
pub(super) struct PreparedSplitProjection {
    pub(super) revision: Revision,
    pub(super) source: String,
    pub(super) lines: Vec<String>,
    pub(super) regions: Vec<ProjectionRegion>,
    /// 与 regions 一一对应；`None` 表示该区域需要在 UI 安装阶段回退构建。
    pub(super) nodes: Vec<Option<Arc<[PreparedBlockNode]>>>,
    /// 从上一 revision 直接复用的顶层区域数量，用于性能诊断和回归测试。
    pub(super) reused_prefix_regions: usize,
}

impl PreparedSplitProjection {
    #[cfg(test)]
    pub(super) fn from_snapshot(snapshot: DocumentSnapshot) -> Self {
        let revision = snapshot.revision();
        let source = snapshot.text();
        let lines = source
            .split('\n')
            .map(ToOwned::to_owned)
            .collect::<Vec<_>>();
        let regions = scan_projection_regions(&lines);
        let nodes = prepare_projection_nodes(&lines, &regions);
        Self {
            revision,
            source,
            lines,
            regions,
            nodes,
            reused_prefix_regions: 0,
        }
    }

    /// 大文档只准备区域边界；离屏语义节点在 viewport 挂载时按区域解析。
    pub(super) fn from_snapshot_adaptive(
        snapshot: DocumentSnapshot,
        virtual_region_threshold: usize,
    ) -> Self {
        let revision = snapshot.revision();
        let source = snapshot.text();
        let lines = source
            .split('\n')
            .map(ToOwned::to_owned)
            .collect::<Vec<_>>();
        let regions = scan_projection_regions(&lines);
        let nodes = if regions.len() >= virtual_region_threshold {
            vec![None; regions.len()]
        } else {
            prepare_projection_nodes(&lines, &regions)
        };
        Self {
            revision,
            source,
            lines,
            regions,
            nodes,
            reused_prefix_regions: 0,
        }
    }

    pub(super) fn from_snapshot_incremental_regions_only(
        snapshot: DocumentSnapshot,
        previous: &Self,
    ) -> Self {
        Self::from_snapshot_incremental_internal(snapshot, previous, false)
    }

    /// 复用未变化的区域前缀，并从受影响区域的前一个边界重新扫描到文末。
    ///
    /// Markdown 块可能受后续闭合标记影响，因此不尝试复用后缀。该策略在保证
    /// 闭合语义正确的前提下，优化长文档后半段的连续编辑。
    pub(super) fn from_snapshot_incremental(snapshot: DocumentSnapshot, previous: &Self) -> Self {
        Self::from_snapshot_incremental_internal(snapshot, previous, true)
    }

    fn from_snapshot_incremental_internal(
        snapshot: DocumentSnapshot,
        previous: &Self,
        prepare_nodes: bool,
    ) -> Self {
        let revision = snapshot.revision();
        let source = snapshot.text();
        let lines = source
            .split('\n')
            .map(ToOwned::to_owned)
            .collect::<Vec<_>>();

        if previous.regions.is_empty() {
            let regions = scan_projection_regions(&lines);
            let nodes = if prepare_nodes {
                prepare_projection_nodes(&lines, &regions)
            } else {
                vec![None; regions.len()]
            };
            return Self {
                revision,
                source,
                lines,
                regions,
                nodes,
                reused_prefix_regions: 0,
            };
        }

        let common_prefix = utf8_common_prefix_len(&previous.source, &source);
        let affected_region = previous
            .regions
            .iter()
            .position(|region| region.bytes.end >= common_prefix)
            .unwrap_or(previous.regions.len() - 1);
        let rescan_region = affected_region.saturating_sub(1);
        let rescan_line = previous.regions[rescan_region].lines.start;
        let rescan_byte = previous.regions[rescan_region].bytes.start;

        // rescan 起点必须处于完全相同的源码前缀，否则退回全量扫描。
        if previous.source.get(..rescan_byte) != source.get(..rescan_byte)
            || rescan_line > lines.len()
        {
            let regions = scan_projection_regions(&lines);
            let nodes = if prepare_nodes {
                prepare_projection_nodes(&lines, &regions)
            } else {
                vec![None; regions.len()]
            };
            return Self {
                revision,
                source,
                lines,
                regions,
                nodes,
                reused_prefix_regions: 0,
            };
        }

        let mut regions = previous.regions[..rescan_region].to_vec();
        let mut nodes = if prepare_nodes {
            previous.nodes[..rescan_region].to_vec()
        } else {
            vec![None; rescan_region]
        };
        let mut rescanned = super::document::scan_projection_regions_from_offset(
            &lines[rescan_line..],
            rescan_line == 0,
        );
        for region in &mut rescanned {
            region.lines.start += rescan_line;
            region.lines.end += rescan_line;
            region.bytes.start += rescan_byte;
            region.bytes.end += rescan_byte;
        }
        let rescanned_nodes = if prepare_nodes {
            prepare_projection_nodes(&lines, &rescanned)
        } else {
            vec![None; rescanned.len()]
        };
        regions.extend(rescanned);
        nodes.extend(rescanned_nodes);

        Self {
            revision,
            source,
            lines,
            regions,
            nodes,
            reused_prefix_regions: rescan_region,
        }
    }
}

fn utf8_common_prefix_len(left: &str, right: &str) -> usize {
    let mut prefix = left
        .as_bytes()
        .iter()
        .zip(right.as_bytes())
        .take_while(|(left, right)| left == right)
        .count();
    while prefix > 0 && (!left.is_char_boundary(prefix) || !right.is_char_boundary(prefix)) {
        prefix -= 1;
    }
    prefix
}
