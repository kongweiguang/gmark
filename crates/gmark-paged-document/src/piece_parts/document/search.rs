// @author kongweiguang

use std::ops::Range;

use regex_automata::hybrid::{dfa::DFA, regex::Regex as StreamingRegex};
use regex_automata::{Anchored, Input};

use super::super::*;

impl PieceDocument {
    pub fn search_literal(
        &self,
        needle: &[u8],
        limit: usize,
    ) -> Result<Vec<SearchMatch>, PagedDocumentError> {
        self.search_literal_cancellable(needle, limit, None)
    }

    fn search_literal_cancellable(
        &self,
        needle: &[u8],
        limit: usize,
        cancellation: Option<&SearchCancellation>,
    ) -> Result<Vec<SearchMatch>, PagedDocumentError> {
        if needle.is_empty() || limit == 0 {
            return Ok(Vec::new());
        }
        let mut matches = Vec::new();
        let mut offset = 0u64;
        let mut carry = Vec::new();
        let mut minimum_start = 0u64;
        while offset < self.len && matches.len() < limit {
            if cancellation.is_some_and(SearchCancellation::is_cancelled) {
                return Err(PagedDocumentError::Cancelled);
            }
            let end = (offset + SEARCH_CHUNK_BYTES).min(self.len);
            let chunk = self.read_range(offset..end)?;
            let combined_start = offset.saturating_sub(carry.len() as u64);
            carry.extend_from_slice(&chunk);
            for relative in memchr::memmem::find_iter(&carry, needle) {
                let start = combined_start + relative as u64;
                if start < minimum_start {
                    continue;
                }
                matches.push(SearchMatch::new(start..start + needle.len() as u64));
                if matches.len() == limit {
                    break;
                }
            }
            minimum_start = end.saturating_sub(needle.len().saturating_sub(1) as u64);
            let keep = needle.len().saturating_sub(1).min(carry.len());
            carry.drain(..carry.len() - keep);
            offset = end;
        }
        Ok(matches)
    }

    fn search_ascii_case_insensitive_literal(
        &self,
        needle: &[u8],
        limit: usize,
        cancellation: &SearchCancellation,
    ) -> Result<Vec<SearchMatch>, PagedDocumentError> {
        let folded_needle = needle
            .iter()
            .map(u8::to_ascii_lowercase)
            .collect::<Vec<_>>();
        let mut matches = Vec::new();
        let mut offset = 0u64;
        let mut carry = Vec::new();
        let mut minimum_start = 0u64;
        while offset < self.len && matches.len() < limit {
            if cancellation.is_cancelled() {
                return Err(PagedDocumentError::Cancelled);
            }
            let end = (offset + SEARCH_CHUNK_BYTES).min(self.len);
            let chunk = self.read_range(offset..end)?;
            let combined_start = offset.saturating_sub(carry.len() as u64);
            carry.extend_from_slice(&chunk);
            let folded = carry.iter().map(u8::to_ascii_lowercase).collect::<Vec<_>>();
            for relative in memchr::memmem::find_iter(&folded, &folded_needle) {
                let start = combined_start + relative as u64;
                if start < minimum_start {
                    continue;
                }
                matches.push(SearchMatch::new(start..start + needle.len() as u64));
                if matches.len() == limit {
                    break;
                }
            }
            minimum_start = end.saturating_sub(needle.len().saturating_sub(1) as u64);
            let keep = needle.len().saturating_sub(1).min(carry.len());
            carry.drain(..carry.len() - keep);
            offset = end;
        }
        Ok(matches)
    }

    pub fn search(
        &self,
        query: &str,
        options: SearchOptions,
        cancellation: &SearchCancellation,
    ) -> Result<Vec<SearchMatch>, PagedDocumentError> {
        if query.is_empty() || options.result_limit == 0 {
            return Ok(Vec::new());
        }
        if options.case_sensitive && !options.regex && !options.whole_word {
            return self.search_literal_cancellable(
                query.as_bytes(),
                options.result_limit,
                Some(cancellation),
            );
        }
        if !options.case_sensitive && !options.regex && !options.whole_word && query.is_ascii() {
            return self.search_ascii_case_insensitive_literal(
                query.as_bytes(),
                options.result_limit,
                cancellation,
            );
        }
        let pattern = if options.regex {
            query.to_owned()
        } else {
            regex::escape(query)
        };
        let expression = StreamingRegex::builder()
            .syntax(
                regex_automata::util::syntax::Config::new()
                    .case_insensitive(!options.case_sensitive),
            )
            .build(&pattern)
            .map_err(|error| PagedDocumentError::InvalidRegex(error.to_string()))?;
        self.search_streaming_regex(&expression, options, cancellation)
    }

    /// 保持 lazy DFA 状态跨越磁盘块，匹配长度不受读取窗口限制。正向 DFA 只给出
    /// 结束位置，因此每次命中后再以 anchored 反向 DFA 流式定位开始位置；两次扫描
    /// 都只持有一个固定大小块，避免超长匹配迫使内存随匹配长度增长。
    fn search_streaming_regex(
        &self,
        expression: &StreamingRegex,
        options: SearchOptions,
        cancellation: &SearchCancellation,
    ) -> Result<Vec<SearchMatch>, PagedDocumentError> {
        let mut matches = Vec::new();
        let mut search_start = 0u64;
        let mut last_match_end = None;
        let mut forward_cache = expression.forward().create_cache();
        let mut reverse_cache = expression.reverse().create_cache();

        while search_start <= self.len && matches.len() < options.result_limit {
            if cancellation.is_cancelled() {
                return Err(PagedDocumentError::Cancelled);
            }
            let Some(end) = self.find_streaming_regex_end(
                expression.forward(),
                &mut forward_cache,
                search_start,
                cancellation,
            )?
            else {
                break;
            };
            let start = self.find_streaming_regex_start(
                expression.reverse(),
                &mut reverse_cache,
                search_start,
                end,
                cancellation,
            )?;
            let range = start..end;

            // 与通用 regex 迭代器一致：空匹配若与上一命中的结束位置重叠，丢弃它并
            // 前进到下一个 UTF-8 边界。流式 start state 只拿到一个 look-behind 字节，
            // 不能像完整 Input 那样自行识别“搜索起点位于码点内部”。
            if range.is_empty() && last_match_end == Some(range.end) {
                if search_start == self.len {
                    break;
                }
                search_start = search_start.saturating_add(1);
                while search_start < self.len && !self.is_char_boundary(search_start)? {
                    search_start += 1;
                }
                continue;
            }

            search_start = range.end;
            last_match_end = Some(range.end);
            if !options.whole_word || self.has_word_boundaries(range.clone())? {
                matches.push(SearchMatch::new(range));
            }
        }
        Ok(matches)
    }

    fn find_streaming_regex_end(
        &self,
        dfa: &DFA,
        cache: &mut regex_automata::hybrid::dfa::Cache,
        search_start: u64,
        cancellation: &SearchCancellation,
    ) -> Result<Option<u64>, PagedDocumentError> {
        let look_behind = if search_start == 0 {
            Vec::new()
        } else {
            self.read_range(search_start - 1..search_start)?
        };
        let input = Input::new(&look_behind).span(look_behind.len()..look_behind.len());
        let mut state = dfa
            .start_state_forward(cache, &input)
            .map_err(search_failure)?;
        let mut last_end = None;
        let mut offset = search_start;

        while offset < self.len {
            if cancellation.is_cancelled() {
                return Err(PagedDocumentError::Cancelled);
            }
            let chunk_end = (offset + SEARCH_CHUNK_BYTES).min(self.len);
            let bytes = self.read_range(offset..chunk_end)?;
            for (relative, byte) in bytes.into_iter().enumerate() {
                let position = offset + relative as u64;
                state = dfa.next_state(cache, state, byte).map_err(search_failure)?;
                if state.is_match() {
                    // lazy DFA 的匹配状态延迟一个字节，因此当前位置就是 exclusive end。
                    last_end = Some(position);
                } else if state.is_dead() {
                    return Ok(last_end);
                } else if state.is_quit() {
                    return Err(PagedDocumentError::Search(format!(
                        "regex engine quit at byte {position}"
                    )));
                }
            }
            offset = chunk_end;
        }

        state = dfa.next_eoi_state(cache, state).map_err(search_failure)?;
        if state.is_match() {
            last_end = Some(self.len);
        }
        Ok(last_end)
    }

    fn find_streaming_regex_start(
        &self,
        dfa: &DFA,
        cache: &mut regex_automata::hybrid::dfa::Cache,
        lower_bound: u64,
        match_end: u64,
        cancellation: &SearchCancellation,
    ) -> Result<u64, PagedDocumentError> {
        let look_ahead = if match_end < self.len {
            self.read_range(match_end..match_end + 1)?
        } else {
            Vec::new()
        };
        let input = Input::new(&look_ahead)
            .span(0..0)
            .anchored(Anchored::Yes)
            .earliest(false);
        let mut state = dfa
            .start_state_reverse(cache, &input)
            .map_err(search_failure)?;
        let mut last_start = None;
        let mut chunk_end = match_end;

        while chunk_end > lower_bound {
            if cancellation.is_cancelled() {
                return Err(PagedDocumentError::Cancelled);
            }
            let chunk_start = chunk_end
                .saturating_sub(SEARCH_CHUNK_BYTES)
                .max(lower_bound);
            let bytes = self.read_range(chunk_start..chunk_end)?;
            for (relative, byte) in bytes.into_iter().enumerate().rev() {
                let position = chunk_start + relative as u64;
                state = dfa.next_state(cache, state, byte).map_err(search_failure)?;
                if state.is_match() {
                    // 反向 DFA 的匹配状态同样延迟一个字节，开始位置因此是 position + 1。
                    last_start = Some(position + 1);
                } else if state.is_dead() {
                    return last_start.ok_or_else(missing_regex_start);
                } else if state.is_quit() {
                    return Err(PagedDocumentError::Search(format!(
                        "reverse regex engine quit at byte {position}"
                    )));
                }
            }
            chunk_end = chunk_start;
        }

        state = if lower_bound > 0 {
            let look_behind = self.read_range(lower_bound - 1..lower_bound)?[0];
            dfa.next_state(cache, state, look_behind)
                .map_err(search_failure)?
        } else {
            dfa.next_eoi_state(cache, state).map_err(search_failure)?
        };
        if state.is_match() {
            last_start = Some(lower_bound);
        }
        last_start.ok_or_else(missing_regex_start)
    }

    fn has_word_boundaries(&self, range: Range<u64>) -> Result<bool, PagedDocumentError> {
        if range.is_empty() {
            return Ok(false);
        }
        let left = self.char_before(range.start)?;
        let start = self.char_after(range.start)?;
        let end = self.char_before(range.end)?;
        let right = self.char_after(range.end)?;
        let is_word =
            |value: Option<char>| value.is_some_and(|ch| ch.is_alphanumeric() || ch == '_');
        Ok((!is_word(left) || !is_word(start)) && (!is_word(right) || !is_word(end)))
    }

    fn char_before(&self, offset: u64) -> Result<Option<char>, PagedDocumentError> {
        if offset == 0 {
            return Ok(None);
        }
        let mut start = offset.saturating_sub(4);
        while start < offset && !self.is_char_boundary(start)? {
            start += 1;
        }
        let bytes = self.read_range(start..offset)?;
        Ok(std::str::from_utf8(&bytes)
            .map_err(|_| PagedDocumentError::InvalidUtf8Boundary)?
            .chars()
            .next_back())
    }

    fn char_after(&self, offset: u64) -> Result<Option<char>, PagedDocumentError> {
        if offset >= self.len {
            return Ok(None);
        }
        let mut end = (offset + 4).min(self.len);
        while end > offset && end < self.len && !self.is_char_boundary(end)? {
            end -= 1;
        }
        let bytes = self.read_range(offset..end)?;
        Ok(std::str::from_utf8(&bytes)
            .map_err(|_| PagedDocumentError::InvalidUtf8Boundary)?
            .chars()
            .next())
    }
}
