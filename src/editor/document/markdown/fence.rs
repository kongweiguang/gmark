// @author kongweiguang

//! CommonMark fenced-code helpers shared by editor parsers.

/// Returns whether `line` terminates a fence opened by `opener_ch` and `opener_len`.
///
/// Callers handle container prefixes and permitted indentation before invoking
/// this predicate. CommonMark permits a closing fence with the same marker
/// whose run length is greater than or equal to the opening fence; only
/// whitespace may follow that run.
pub(crate) fn is_closing_fence(line: &str, opener_ch: char, opener_len: usize) -> bool {
    let trimmed = line.trim_end();
    if !trimmed.starts_with(opener_ch) {
        return false;
    }

    let run_len = trimmed
        .chars()
        .take_while(|current| *current == opener_ch)
        .count();
    run_len >= opener_len && trimmed[opener_ch.len_utf8() * run_len..].trim().is_empty()
}
