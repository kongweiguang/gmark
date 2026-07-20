// @author kongweiguang

use crate::TextEdit;
use zed_rope::Rope;

/// 源文件中单个换行的字节表示。
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum LineEnding {
    /// Unix 换行。
    #[default]
    Lf,
    /// Windows 换行。
    CrLf,
    /// 旧式 Mac 换行。
    Cr,
}

/// 当前文档换行样式摘要。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LineEndingStatus {
    /// 文档没有换行符。
    None,
    /// 所有换行使用同一种样式。
    Uniform(LineEnding),
    /// 文档包含两种或以上换行样式。
    Mixed,
}

impl LineEnding {
    pub(crate) fn bytes(self) -> &'static [u8] {
        match self {
            Self::Lf => b"\n",
            Self::CrLf => b"\r\n",
            Self::Cr => b"\r",
        }
    }
}

/// 可持久化的源码字节格式快照。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceFormatSnapshot {
    /// 原文件是否带 UTF-8 BOM。
    pub utf8_bom: bool,
    /// 与规范化源码中每个 `\n` 一一对应的原始换行样式。
    pub endings: Vec<LineEnding>,
    /// 新插入换行没有邻居时使用的样式。
    pub dominant: LineEnding,
}

/// 不复制逐行换行映射的 O(1) 源码格式摘要，供高频 UI 路径读取。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SourceFormatSummary {
    /// 原文件是否带 UTF-8 BOM。
    pub utf8_bom: bool,
    /// 新插入换行没有邻居时使用的样式。
    pub dominant: LineEnding,
    /// 当前逐行换行分布。
    pub line_endings: LineEndingStatus,
}

impl SourceFormatSnapshot {
    /// 返回供状态栏和命令判定使用的换行摘要。
    pub fn line_ending_status(&self) -> LineEndingStatus {
        let Some(first) = self.endings.first().copied() else {
            return LineEndingStatus::None;
        };
        if self.endings.iter().all(|ending| *ending == first) {
            LineEndingStatus::Uniform(first)
        } else {
            LineEndingStatus::Mixed
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct SourceFormat {
    utf8_bom: bool,
    endings: Vec<LineEnding>,
    /// 与 `endings` 同步维护，避免每次按键为主换行符重扫整篇长文档。
    ending_counts: [usize; 3],
    dominant: LineEnding,
}

#[derive(Clone, Debug)]
pub(crate) struct FormatPatch {
    operations: Vec<FormatOperation>,
    dominant_override: Option<(LineEnding, LineEnding)>,
}

#[derive(Clone, Debug)]
struct FormatOperation {
    newline_index: usize,
    removed: Vec<LineEnding>,
    inserted: Vec<LineEnding>,
}

impl SourceFormat {
    pub(crate) fn parse(text: &str) -> (String, Self) {
        let (utf8_bom, text) = text
            .strip_prefix('\u{feff}')
            .map_or((false, text), |text| (true, text));
        let mut normalized = String::with_capacity(text.len());
        let mut endings = Vec::new();
        let bytes = text.as_bytes();
        let mut cursor = 0usize;
        let mut segment_start = 0usize;
        while cursor < bytes.len() {
            let ending = match bytes[cursor] {
                b'\r' if bytes.get(cursor + 1) == Some(&b'\n') => Some((LineEnding::CrLf, 2)),
                b'\r' => Some((LineEnding::Cr, 1)),
                b'\n' => Some((LineEnding::Lf, 1)),
                _ => None,
            };
            if let Some((ending, width)) = ending {
                normalized.push_str(&text[segment_start..cursor]);
                normalized.push('\n');
                endings.push(ending);
                cursor += width;
                segment_start = cursor;
            } else {
                cursor += 1;
            }
        }
        normalized.push_str(&text[segment_start..]);
        let ending_counts = count_endings(&endings);
        let dominant = dominant_ending(ending_counts);
        (
            normalized,
            Self {
                utf8_bom,
                endings,
                ending_counts,
                dominant,
            },
        )
    }

    pub(crate) fn from_normalized(text: &str, snapshot: SourceFormatSnapshot) -> Option<Self> {
        (text.bytes().filter(|byte| *byte == b'\n').count() == snapshot.endings.len()).then(|| {
            let ending_counts = count_endings(&snapshot.endings);
            Self {
                utf8_bom: snapshot.utf8_bom,
                endings: snapshot.endings,
                ending_counts,
                dominant: snapshot.dominant,
            }
        })
    }

    pub(crate) fn snapshot(&self) -> SourceFormatSnapshot {
        SourceFormatSnapshot {
            utf8_bom: self.utf8_bom,
            endings: self.endings.clone(),
            dominant: self.dominant,
        }
    }

    pub(crate) fn summary(&self) -> SourceFormatSummary {
        let line_endings = match self.ending_counts {
            [0, 0, 0] => LineEndingStatus::None,
            [lf, 0, 0] if lf > 0 => LineEndingStatus::Uniform(LineEnding::Lf),
            [0, crlf, 0] if crlf > 0 => LineEndingStatus::Uniform(LineEnding::CrLf),
            [0, 0, cr] if cr > 0 => LineEndingStatus::Uniform(LineEnding::Cr),
            _ => LineEndingStatus::Mixed,
        };
        SourceFormatSummary {
            utf8_bom: self.utf8_bom,
            dominant: self.dominant,
            line_endings,
        }
    }

    pub(crate) fn ending_count(&self) -> usize {
        self.endings.len()
    }

    pub(crate) fn serialized_bytes(&self, normalized: &str) -> Vec<u8> {
        let extra = self
            .endings
            .iter()
            .filter(|ending| matches!(ending, LineEnding::CrLf))
            .count();
        let mut output =
            Vec::with_capacity(normalized.len() + extra + if self.utf8_bom { 3 } else { 0 });
        if self.utf8_bom {
            output.extend_from_slice(&[0xef, 0xbb, 0xbf]);
        }
        let mut ending_index = 0usize;
        let mut segment_start = 0usize;
        for (offset, byte) in normalized.bytes().enumerate() {
            if byte != b'\n' {
                continue;
            }
            output.extend_from_slice(&normalized.as_bytes()[segment_start..offset]);
            let ending = self
                .endings
                .get(ending_index)
                .copied()
                .unwrap_or(self.dominant);
            output.extend_from_slice(ending.bytes());
            ending_index += 1;
            segment_start = offset + 1;
        }
        output.extend_from_slice(&normalized.as_bytes()[segment_start..]);
        output
    }

    pub(crate) fn build_patch(&self, source: &Rope, edits: &[TextEdit]) -> FormatPatch {
        let mut operations = Vec::with_capacity(edits.len());
        for edit in edits {
            if edit.range().is_empty() && !edit.replacement().contains('\n') {
                continue;
            }
            let start_row = source.offset_to_point(edit.range().start).row as usize;
            let end_row = source.offset_to_point(edit.range().end).row as usize;
            let newline_index = start_row;
            let removed_count = end_row - start_row;
            let inserted_count = edit
                .replacement()
                .bytes()
                .filter(|byte| *byte == b'\n')
                .count();
            let inserted_ending = newline_index
                .checked_sub(1)
                .and_then(|index| self.endings.get(index))
                .copied()
                .or_else(|| self.endings.get(newline_index + removed_count).copied())
                .unwrap_or(self.dominant);
            operations.push(FormatOperation {
                newline_index,
                removed: self.endings[newline_index..newline_index.saturating_add(removed_count)]
                    .to_vec(),
                inserted: vec![inserted_ending; inserted_count],
            });
        }
        FormatPatch {
            operations,
            dominant_override: None,
        }
    }

    pub(crate) fn normalization_patch(&self, ending: LineEnding) -> Option<FormatPatch> {
        if self.endings.iter().all(|current| *current == ending) && self.dominant == ending {
            return None;
        }
        Some(FormatPatch {
            operations: vec![FormatOperation {
                newline_index: 0,
                removed: self.endings.clone(),
                inserted: vec![ending; self.endings.len()],
            }],
            dominant_override: Some((self.dominant, ending)),
        })
    }

    pub(crate) fn apply(&mut self, patch: &FormatPatch) {
        for operation in patch.operations.iter().rev() {
            apply_ending_count_delta(
                &mut self.ending_counts,
                &operation.removed,
                &operation.inserted,
            );
            replace_endings(
                &mut self.endings,
                operation.newline_index,
                operation.removed.len(),
                &operation.inserted,
            );
        }
        if let Some((_, after)) = patch.dominant_override {
            self.dominant = after;
        } else if !self.endings.is_empty() {
            self.dominant = dominant_ending(self.ending_counts);
        }
    }

    pub(crate) fn apply_inverse(&mut self, patch: &FormatPatch) {
        // 正向按源码坐标从后向前执行；逆向从前向后才能恢复后续操作的原坐标。
        for operation in &patch.operations {
            apply_ending_count_delta(
                &mut self.ending_counts,
                &operation.inserted,
                &operation.removed,
            );
            replace_endings(
                &mut self.endings,
                operation.newline_index,
                operation.inserted.len(),
                &operation.removed,
            );
        }
        if let Some((before, _)) = patch.dominant_override {
            self.dominant = before;
        } else if !self.endings.is_empty() {
            self.dominant = dominant_ending(self.ending_counts);
        }
    }
}

fn ending_index(ending: LineEnding) -> usize {
    match ending {
        LineEnding::Lf => 0,
        LineEnding::CrLf => 1,
        LineEnding::Cr => 2,
    }
}

fn count_endings(endings: &[LineEnding]) -> [usize; 3] {
    let mut counts = [0usize; 3];
    for ending in endings {
        counts[ending_index(*ending)] += 1;
    }
    counts
}

fn apply_ending_count_delta(
    counts: &mut [usize; 3],
    removed: &[LineEnding],
    inserted: &[LineEnding],
) {
    let removed_counts = count_endings(removed);
    let inserted_counts = count_endings(inserted);
    for index in 0..counts.len() {
        debug_assert!(counts[index] >= removed_counts[index]);
        counts[index] = counts[index] - removed_counts[index] + inserted_counts[index];
    }
}

fn replace_endings(
    endings: &mut Vec<LineEnding>,
    index: usize,
    removed_len: usize,
    replacement: &[LineEnding],
) {
    endings.splice(index..index + removed_len, replacement.iter().copied());
}

fn dominant_ending(counts: [usize; 3]) -> LineEnding {
    let index = counts
        .iter()
        .enumerate()
        .max_by_key(|(index, count)| (**count, std::cmp::Reverse(*index)))
        .map_or(0, |(index, _)| index);
    [LineEnding::Lf, LineEnding::CrLf, LineEnding::Cr][index]
}
