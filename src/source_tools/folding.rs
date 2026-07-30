// @author kongweiguang

//! Source view 的折叠投影兼容层；折叠发现和增量语法树属于 `gmark-source-tools`。

use std::collections::{BTreeSet, HashSet};
use std::ops::Range;

use gmark_source_tools::{FoldRange, IncrementalFoldParser, fold_ranges_in_window};

use super::SourceLanguageId;

/// 一个可折叠结构使用真实源码坐标；UI 只能派生可见行，不能改写这些坐标。
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct FoldRegion {
    pub(crate) id: u64,
    pub(crate) kind: &'static str,
    pub(crate) byte_range: Range<u64>,
    pub(crate) start_line: usize,
    pub(crate) end_line: usize,
    pub(crate) depth: usize,
    /// 格式化后用于恢复相同结构的折叠状态；这是 Source UI 的投影信息。
    pub(crate) structure_path: Vec<u32>,
    pub(crate) closing: Option<char>,
}

impl FoldRegion {
    pub(crate) fn hidden_line_count(&self) -> usize {
        self.end_line.saturating_sub(self.start_line)
    }
}

#[derive(Clone, Debug)]
struct EffectiveFold {
    start: usize,
    end: usize,
    hidden_before: usize,
    visible_start: usize,
}

/// 折叠状态与行投影。嵌套折叠状态会保留，但只有最外层折叠参与行数映射。
#[derive(Clone, Debug, Default)]
pub(crate) struct FoldProjectionIndex {
    real_line_count: usize,
    regions: Vec<FoldRegion>,
    collapsed: BTreeSet<u64>,
    effective: Vec<EffectiveFold>,
    hidden_total: usize,
}

/// `DocumentHost` 的历史 API 兼容层；领域解析器不依赖窗口或 GPUI。
#[derive(Default)]
pub(crate) struct ResidentFoldParser {
    parser: IncrementalFoldParser,
}

impl ResidentFoldParser {
    pub(crate) fn parse(
        &mut self,
        document_epoch: u64,
        language: SourceLanguageId,
        source: &str,
    ) -> Vec<FoldRegion> {
        fold_regions(self.parser.parse(document_epoch, language, source))
    }

    #[cfg(test)]
    fn last_parse_was_incremental(&self) -> bool {
        self.parser.last_parse_was_incremental()
    }
}

impl FoldProjectionIndex {
    pub(crate) fn set_real_line_count(&mut self, real_line_count: usize) {
        if self.real_line_count != real_line_count {
            self.real_line_count = real_line_count;
            self.rebuild();
        }
    }

    pub(crate) fn set_regions(&mut self, real_line_count: usize, mut regions: Vec<FoldRegion>) {
        let collapsed_structure = self
            .regions
            .iter()
            .filter(|region| self.collapsed.contains(&region.id))
            .map(|region| (region.kind, region.structure_path.clone()))
            .collect::<HashSet<_>>();
        regions.sort_by_key(|region| (region.start_line, std::cmp::Reverse(region.end_line)));
        let current = regions
            .iter()
            .map(|region| region.id)
            .collect::<HashSet<_>>();
        let mut next_collapsed = self
            .collapsed
            .iter()
            .filter(|id| current.contains(id))
            .copied()
            .collect::<BTreeSet<_>>();
        for region in &regions {
            if collapsed_structure.contains(&(region.kind, region.structure_path.clone())) {
                next_collapsed.insert(region.id);
            }
        }
        self.collapsed = next_collapsed;
        self.real_line_count = real_line_count;
        self.regions = regions;
        self.rebuild();
    }

    pub(crate) fn replace_window_regions(
        &mut self,
        real_line_count: usize,
        window: Range<usize>,
        regions: Vec<FoldRegion>,
    ) {
        let mut merged = self
            .regions
            .iter()
            .filter(|region| region.end_line < window.start || region.start_line >= window.end)
            .cloned()
            .collect::<Vec<_>>();
        merged.extend(regions);
        self.set_regions(real_line_count, merged);
    }

    pub(crate) fn regions(&self) -> &[FoldRegion] {
        &self.regions
    }

    pub(crate) fn region_starting(&self, line: usize) -> Option<&FoldRegion> {
        self.regions
            .iter()
            .filter(|region| region.start_line == line)
            .max_by_key(|region| region.end_line)
    }

    pub(crate) fn is_collapsed(&self, id: u64) -> bool {
        self.collapsed.contains(&id)
    }

    pub(crate) fn toggle(&mut self, id: u64) -> bool {
        if !self.regions.iter().any(|region| region.id == id) {
            return false;
        }
        if !self.collapsed.remove(&id) {
            self.collapsed.insert(id);
        }
        self.rebuild();
        true
    }

    pub(crate) fn set_collapsed(&mut self, id: u64, collapsed: bool) -> bool {
        if !self.regions.iter().any(|region| region.id == id) {
            return false;
        }
        let changed = if collapsed {
            self.collapsed.insert(id)
        } else {
            self.collapsed.remove(&id)
        };
        if changed {
            self.rebuild();
        }
        changed
    }

    pub(crate) fn collapse_all(&mut self) {
        self.collapsed = self.regions.iter().map(|region| region.id).collect();
        self.rebuild();
    }

    pub(crate) fn expand_all(&mut self) {
        self.collapsed.clear();
        self.rebuild();
    }

    /// 展开包含目标真实行的所有折叠祖先，保证导航不会停在不可见行。
    pub(crate) fn ensure_line_visible(&mut self, line: usize) -> bool {
        let ids = self
            .regions
            .iter()
            .filter(|region| {
                self.collapsed.contains(&region.id)
                    && line > region.start_line
                    && line <= region.end_line
            })
            .map(|region| region.id)
            .collect::<Vec<_>>();
        if ids.is_empty() {
            return false;
        }
        for id in ids {
            self.collapsed.remove(&id);
        }
        self.rebuild();
        true
    }

    /// 普通编辑命中结构时立即展开；未命中的后续区域按 byte/行差量平移。
    pub(crate) fn apply_source_edit(
        &mut self,
        range: Range<u64>,
        start_line: usize,
        end_line: usize,
        replacement: &str,
    ) {
        let removed_lines = end_line.saturating_sub(start_line);
        let inserted_lines = replacement.bytes().filter(|byte| *byte == b'\n').count();
        let byte_delta = replacement.len() as i128 - range.end.saturating_sub(range.start) as i128;
        let line_delta = inserted_lines as i128 - removed_lines as i128;
        let insertion = range.is_empty();
        let mut removed = BTreeSet::new();

        self.regions.retain_mut(|region| {
            let touched = if insertion {
                range.start >= region.byte_range.start && range.start <= region.byte_range.end
            } else {
                range.start < region.byte_range.end && range.end > region.byte_range.start
            };
            if touched {
                removed.insert(region.id);
                return false;
            }
            if region.byte_range.start >= range.end {
                region.byte_range.start = shift_u64(region.byte_range.start, byte_delta);
                region.byte_range.end = shift_u64(region.byte_range.end, byte_delta);
                region.start_line = shift_usize(region.start_line, line_delta);
                region.end_line = shift_usize(region.end_line, line_delta);
            }
            true
        });
        self.collapsed.retain(|id| !removed.contains(id));
        self.real_line_count = shift_usize(self.real_line_count, line_delta).max(1);
        self.rebuild();
    }

    pub(crate) fn visible_line_count(&self) -> usize {
        self.real_line_count.saturating_sub(self.hidden_total)
    }

    pub(crate) fn real_line_for_visible(&self, visible: usize) -> usize {
        let visible = visible.min(self.visible_line_count().saturating_sub(1));
        let count = self
            .effective
            .partition_point(|fold| fold.visible_start < visible);
        let hidden = count.checked_sub(1).map_or(0, |index| {
            let fold = &self.effective[index];
            fold.hidden_before + fold.end - fold.start
        });
        visible
            .saturating_add(hidden)
            .min(self.real_line_count.saturating_sub(1))
    }

    pub(crate) fn visible_line_for_real(&self, real: usize) -> usize {
        let real = real.min(self.real_line_count.saturating_sub(1));
        if let Some(fold) = self
            .effective
            .iter()
            .find(|fold| real > fold.start && real <= fold.end)
        {
            return fold.visible_start;
        }
        let count = self.effective.partition_point(|fold| fold.end < real);
        let hidden = count.checked_sub(1).map_or(0, |index| {
            let fold = &self.effective[index];
            fold.hidden_before + fold.end - fold.start
        });
        real.saturating_sub(hidden)
    }

    fn rebuild(&mut self) {
        self.effective.clear();
        let mut outer_end = None;
        let mut hidden_before = 0usize;
        for region in &self.regions {
            if !self.collapsed.contains(&region.id) || region.hidden_line_count() == 0 {
                continue;
            }
            if outer_end.is_some_and(|end| region.end_line <= end)
                || outer_end.is_some_and(|end| region.start_line <= end)
            {
                continue;
            }
            self.effective.push(EffectiveFold {
                start: region.start_line,
                end: region.end_line,
                hidden_before,
                visible_start: region.start_line.saturating_sub(hidden_before),
            });
            hidden_before = hidden_before.saturating_add(region.hidden_line_count());
            outer_end = Some(region.end_line);
        }
        self.hidden_total = hidden_before.min(self.real_line_count.saturating_sub(1));
    }
}

/// 解析完整或有界源码窗口。范围必须使用窗口内的真实 byte/行基址。
pub(crate) fn discover_fold_regions(
    language: SourceLanguageId,
    source: &str,
    byte_base: u64,
    line_base: usize,
) -> Vec<FoldRegion> {
    fold_regions(fold_ranges_in_window(
        language, source, byte_base, line_base,
    ))
}

fn fold_regions(ranges: Vec<FoldRange>) -> Vec<FoldRegion> {
    let mut regions = ranges
        .into_iter()
        .map(|range| FoldRegion {
            id: range.id,
            kind: range.kind.stable_name(),
            byte_range: range.byte_range.start()..range.byte_range.end(),
            start_line: range.start_line,
            end_line: range.end_line,
            depth: range.depth,
            structure_path: Vec::new(),
            closing: range.closing,
        })
        .collect::<Vec<_>>();
    normalize_regions(&mut regions);
    regions
}

fn normalize_regions(regions: &mut Vec<FoldRegion>) {
    regions.retain(|region| region.end_line > region.start_line);
    regions.sort_by_key(|region| {
        (
            region.start_line,
            std::cmp::Reverse(region.end_line),
            region.byte_range.start,
        )
    });
    regions.dedup_by(|right, left| {
        right.start_line == left.start_line
            && right.end_line == left.end_line
            && right.kind == left.kind
    });
    let mut stack = Vec::<(usize, Vec<u32>, u32)>::new();
    let mut root_ordinal = 0_u32;
    for region in regions {
        while stack
            .last()
            .is_some_and(|(end, _, _)| *end < region.end_line)
        {
            stack.pop();
        }
        let path = if let Some((_, parent_path, next_child)) = stack.last_mut() {
            let mut path = parent_path.clone();
            path.push(*next_child);
            *next_child = next_child.saturating_add(1);
            path
        } else {
            let path = vec![root_ordinal];
            root_ordinal = root_ordinal.saturating_add(1);
            path
        };
        region.depth = path.len().saturating_sub(1);
        region.structure_path = path.clone();
        region.id = stable_region_id(region.kind, region.byte_range.start, region.depth);
        stack.push((region.end_line, path, 0));
    }
}

fn stable_region_id(kind: &str, start: u64, depth: usize) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for byte in kind
        .bytes()
        .chain(start.to_le_bytes())
        .chain(u64::try_from(depth).unwrap_or(u64::MAX).to_le_bytes())
    {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

fn shift_u64(value: u64, delta: i128) -> u64 {
    u64::try_from(value as i128 + delta).unwrap_or(if delta.is_negative() { 0 } else { u64::MAX })
}

fn shift_usize(value: usize, delta: i128) -> usize {
    usize::try_from(value as i128 + delta).unwrap_or(if delta.is_negative() {
        0
    } else {
        usize::MAX
    })
}

#[cfg(test)]
#[path = "../../tests/unit/source_tools/folding.rs"]
mod tests;
