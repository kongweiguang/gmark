// @author kongweiguang

use super::*;

impl PieceDocument {
    pub fn accept_external_append(
        &mut self,
        source: FileSource,
        index: LineIndex,
    ) -> Result<(), LargeDocumentError> {
        if !self.undo.is_empty() || !self.redo.is_empty() || self.pieces.piece_count() > 1 {
            return Err(LargeDocumentError::SourceChanged);
        }
        let identity = source.identity()?;
        if identity.len < self.base_identity.len
            || identity.os_file_id != self.base_identity.os_file_id
            || source.sampled_prefix_hash(self.base_identity.len)? != self.base_sample
        {
            return Err(LargeDocumentError::SourceChanged);
        }
        self.len = identity.len;
        self.base_identity = identity;
        self.base_sample = source.sampled_prefix_hash(self.len)?;
        self.base_index = index.clone();
        self.source = Some(source);
        self.pieces = PieceTree::from_iter((self.len > 0).then_some(Piece {
            source: PieceSource::Base,
            range: 0..self.len,
            newlines: index.newline_count(),
        }));
        Ok(())
    }

    pub fn line_count(&self) -> u64 {
        self.pieces.root.summary().newlines + 1
    }

    pub fn line_range(&self, line: u64) -> Option<Range<u64>> {
        if line >= self.line_count() {
            return None;
        }
        let start = if line == 0 {
            0
        } else {
            self.logical_newline_offset(line - 1)?
        };
        let end = self.logical_newline_offset(line).unwrap_or(self.len);
        Some(start..end)
    }

    pub fn replace_text(
        &mut self,
        range: Range<u64>,
        replacement: &str,
    ) -> Result<(), LargeDocumentError> {
        self.replace_text_chunks(range, std::iter::once(replacement))
    }

    /// 以一个撤销事务写入多个 UTF-8 块，恢复超大粘贴时无需先拼成同等大小的临时字符串。
    pub fn replace_text_chunks<'a>(
        &mut self,
        range: Range<u64>,
        chunks: impl IntoIterator<Item = &'a str>,
    ) -> Result<(), LargeDocumentError> {
        if range.start > range.end || range.end > self.len {
            return Err(LargeDocumentError::InvalidRange {
                start: range.start,
                end: range.end,
                len: self.len,
            });
        }
        if !self.is_char_boundary(range.start)? || !self.is_char_boundary(range.end)? {
            return Err(LargeDocumentError::InvalidUtf8Boundary);
        }
        let mut replacement_len = 0u64;
        let mut replacement_pieces = Vec::new();
        for chunk in chunks {
            if chunk.is_empty() {
                continue;
            }
            replacement_len = replacement_len
                .checked_add(chunk.len() as u64)
                .ok_or(LargeDocumentError::RangeTooLarge)?;
            replacement_pieces.push(Piece {
                source: PieceSource::Add,
                range: self.additions.append(chunk.as_bytes())?,
                newlines: chunk.bytes().filter(|byte| *byte == b'\n').count() as u64,
            });
        }

        let mut cursor = PieceCursor::new(self, 0);
        let mut next = cursor.slice(range.start)?;
        cursor.seek_forward(range.end);
        next.append(PieceTree::from_iter(replacement_pieces));
        next.append(cursor.slice(self.len)?);
        drop(cursor);
        self.record_undo_root(self.pieces.clone(), self.len);
        self.redo.clear();
        self.pieces = next;
        self.len = self.len - (range.end - range.start) + replacement_len;
        Ok(())
    }

    /// 将基于同一 Source revision 的多个不相交编辑作为一个撤销事务提交。
    /// 倒序应用可保持所有 range 都在原始字节坐标系中。
    pub fn replace_text_batch(
        &mut self,
        edits: &[(Range<u64>, Arc<str>)],
    ) -> Result<(), LargeDocumentError> {
        if edits.is_empty() {
            return Ok(());
        }
        let mut ordered = edits.to_vec();
        ordered.sort_by_key(|(range, _)| (range.start, range.end));
        for pair in ordered.windows(2) {
            let previous = &pair[0].0;
            let next = &pair[1].0;
            if previous.end > next.start
                || (previous.is_empty() && next.is_empty() && previous.start == next.start)
            {
                return Err(LargeDocumentError::InvalidTransaction(
                    "derived edit ranges overlap or contain ambiguous inserts".into(),
                ));
            }
        }
        for (range, _) in &ordered {
            if range.start > range.end || range.end > self.len {
                return Err(LargeDocumentError::InvalidRange {
                    start: range.start,
                    end: range.end,
                    len: self.len,
                });
            }
            if !self.is_char_boundary(range.start)? || !self.is_char_boundary(range.end)? {
                return Err(LargeDocumentError::InvalidUtf8Boundary);
            }
        }

        let original_pieces = self.pieces.clone();
        let original_len = self.len;
        let original_undo = self.undo.clone();
        let original_redo = self.redo.clone();
        for (range, replacement) in ordered.iter().rev() {
            if let Err(error) = self.replace_text(range.clone(), replacement) {
                self.pieces = original_pieces;
                self.len = original_len;
                self.undo = original_undo;
                self.redo = original_redo;
                return Err(error);
            }
        }
        self.undo = original_undo;
        self.record_undo_root(original_pieces, original_len);
        self.redo.clear();
        Ok(())
    }

    pub fn undo(&mut self) -> bool {
        let Some((pieces, len)) = self.undo.pop() else {
            return false;
        };
        self.redo.push((self.pieces.clone(), self.len));
        self.pieces = pieces;
        self.len = len;
        true
    }

    pub fn redo(&mut self) -> bool {
        let Some((pieces, len)) = self.redo.pop() else {
            return false;
        };
        self.record_undo_root(self.pieces.clone(), self.len);
        self.pieces = pieces;
        self.len = len;
        true
    }

    fn record_undo_root(&mut self, pieces: PieceTree, len: u64) {
        if self.undo.len() == DEFAULT_HISTORY_LIMIT {
            self.undo.remove(0);
        }
        self.undo.push((pieces, len));
    }

    pub fn read_range(&self, range: Range<u64>) -> Result<Vec<u8>, LargeDocumentError> {
        if range.start > range.end || range.end > self.len {
            return Err(LargeDocumentError::InvalidRange {
                start: range.start,
                end: range.end,
                len: self.len,
            });
        }
        let capacity = usize::try_from(range.end - range.start)
            .map_err(|_| LargeDocumentError::RangeTooLarge)?;
        let mut output = Vec::with_capacity(capacity);
        if range.is_empty() {
            return Ok(output);
        }
        let mut cursor = self.pieces.root.cursor::<Bytes>(());
        cursor.seek(&Bytes(range.start), Bias::Right);
        while let Some(piece) = cursor.item() {
            let logical_start = cursor.start().0;
            let logical_end = cursor.end().0;
            let start = range.start.max(logical_start);
            let end = range.end.min(logical_end);
            if start < end {
                let relative = start - logical_start..end - logical_start;
                let bytes = piece.range.start + relative.start..piece.range.start + relative.end;
                match piece.source {
                    PieceSource::Base => {
                        output.extend(self.source()?.read_range(bytes.start, bytes.end)?)
                    }
                    PieceSource::Add => output.extend(self.additions.read(bytes)?),
                }
            }
            if logical_end >= range.end {
                break;
            }
            cursor.next();
        }
        Ok(output)
    }

    /// 分块读取不可变 PieceTree 快照，并在页块之间响应取消。剪贴板任务因此不会在
    /// Tab 已关闭或文件 identity 已变化后继续扫描数十 MiB 的旧文档。
    pub fn read_range_cancellable(
        &self,
        range: Range<u64>,
        cancellation: &SearchCancellation,
    ) -> Result<Vec<u8>, LargeDocumentError> {
        if range.start > range.end || range.end > self.len {
            return Err(LargeDocumentError::InvalidRange {
                start: range.start,
                end: range.end,
                len: self.len,
            });
        }
        if cancellation.is_cancelled() {
            return Err(LargeDocumentError::Cancelled);
        }
        let capacity = usize::try_from(range.end - range.start)
            .map_err(|_| LargeDocumentError::RangeTooLarge)?;
        let mut output = Vec::with_capacity(capacity);
        const COPY_CHUNK: u64 = 1024 * 1024;
        let mut offset = range.start;
        while offset < range.end {
            if cancellation.is_cancelled() {
                return Err(LargeDocumentError::Cancelled);
            }
            let end = offset.saturating_add(COPY_CHUNK).min(range.end);
            output.extend(self.read_range(offset..end)?);
            offset = end;
        }
        Ok(output)
    }

    pub fn search_literal(
        &self,
        needle: &[u8],
        limit: usize,
    ) -> Result<Vec<SearchMatch>, LargeDocumentError> {
        self.search_literal_cancellable(needle, limit, None)
    }

    fn search_literal_cancellable(
        &self,
        needle: &[u8],
        limit: usize,
        cancellation: Option<&SearchCancellation>,
    ) -> Result<Vec<SearchMatch>, LargeDocumentError> {
        if needle.is_empty() || limit == 0 {
            return Ok(Vec::new());
        }
        let mut matches = Vec::new();
        let mut offset = 0u64;
        let mut carry = Vec::new();
        let mut minimum_start = 0u64;
        while offset < self.len && matches.len() < limit {
            if cancellation.is_some_and(SearchCancellation::is_cancelled) {
                return Err(LargeDocumentError::Cancelled);
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
    ) -> Result<Vec<SearchMatch>, LargeDocumentError> {
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
                return Err(LargeDocumentError::Cancelled);
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
    ) -> Result<Vec<SearchMatch>, LargeDocumentError> {
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
            .map_err(|error| LargeDocumentError::InvalidRegex(error.to_string()))?;
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
    ) -> Result<Vec<SearchMatch>, LargeDocumentError> {
        let mut matches = Vec::new();
        let mut search_start = 0u64;
        let mut last_match_end = None;
        let mut forward_cache = expression.forward().create_cache();
        let mut reverse_cache = expression.reverse().create_cache();

        while search_start <= self.len && matches.len() < options.result_limit {
            if cancellation.is_cancelled() {
                return Err(LargeDocumentError::Cancelled);
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
    ) -> Result<Option<u64>, LargeDocumentError> {
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
                return Err(LargeDocumentError::Cancelled);
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
                    return Err(LargeDocumentError::Search(format!(
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
    ) -> Result<u64, LargeDocumentError> {
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
                return Err(LargeDocumentError::Cancelled);
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
                    return Err(LargeDocumentError::Search(format!(
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

    fn has_word_boundaries(&self, range: Range<u64>) -> Result<bool, LargeDocumentError> {
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

    fn char_before(&self, offset: u64) -> Result<Option<char>, LargeDocumentError> {
        if offset == 0 {
            return Ok(None);
        }
        let mut start = offset.saturating_sub(4);
        while start < offset && !self.is_char_boundary(start)? {
            start += 1;
        }
        let bytes = self.read_range(start..offset)?;
        Ok(std::str::from_utf8(&bytes)
            .map_err(|_| LargeDocumentError::InvalidUtf8Boundary)?
            .chars()
            .next_back())
    }

    fn char_after(&self, offset: u64) -> Result<Option<char>, LargeDocumentError> {
        if offset >= self.len {
            return Ok(None);
        }
        let mut end = (offset + 4).min(self.len);
        while end > offset && end < self.len && !self.is_char_boundary(end)? {
            end -= 1;
        }
        let bytes = self.read_range(offset..end)?;
        Ok(std::str::from_utf8(&bytes)
            .map_err(|_| LargeDocumentError::InvalidUtf8Boundary)?
            .chars()
            .next())
    }

    /// 按逻辑 piece 顺序流式输出，不物化完整文档。
    pub fn write_to(&self, mut output: impl Write) -> Result<(), LargeDocumentError> {
        self.for_each_utf8_chunk(8 * 1024 * 1024, |bytes| {
            output
                .write_all(bytes)
                .map_err(|source| LargeDocumentError::Io {
                    path: self.base_identity.path.clone(),
                    source,
                })
        })
    }

    /// 仅在 UTF-8 边界切块，供编码器和搜索器保持跨块状态。
    pub fn for_each_utf8_chunk(
        &self,
        chunk_bytes: u64,
        mut callback: impl FnMut(&[u8]) -> Result<(), LargeDocumentError>,
    ) -> Result<(), LargeDocumentError> {
        let chunk_bytes = chunk_bytes.max(4);
        let mut offset = 0u64;
        while offset < self.len {
            let mut end = (offset + chunk_bytes).min(self.len);
            while end < self.len && end > offset && !self.is_char_boundary(end)? {
                end -= 1;
            }
            if end == offset {
                return Err(LargeDocumentError::InvalidUtf8Boundary);
            }
            let bytes = self.read_range(offset..end)?;
            callback(&bytes)?;
            offset = end;
        }
        Ok(())
    }

    /// 遍历一个已验证的 Source 字节范围，并只在 UTF-8 字符边界切块。
    /// 选区导出借此复用整文档编码器，而不物化超大选区。
    pub fn for_each_utf8_range_chunk(
        &self,
        range: Range<u64>,
        chunk_bytes: u64,
        mut callback: impl FnMut(&[u8]) -> Result<(), LargeDocumentError>,
    ) -> Result<(), LargeDocumentError> {
        if range.start > range.end || range.end > self.len {
            return Err(LargeDocumentError::InvalidRange {
                start: range.start,
                end: range.end,
                len: self.len,
            });
        }
        if !self.is_char_boundary(range.start)? || !self.is_char_boundary(range.end)? {
            return Err(LargeDocumentError::InvalidUtf8Boundary);
        }
        let chunk_bytes = chunk_bytes.max(4);
        let mut offset = range.start;
        while offset < range.end {
            let mut end = offset.saturating_add(chunk_bytes).min(range.end);
            while end < range.end && end > offset && !self.is_char_boundary(end)? {
                end -= 1;
            }
            if end == offset {
                return Err(LargeDocumentError::InvalidUtf8Boundary);
            }
            let bytes = self.read_range(offset..end)?;
            callback(&bytes)?;
            offset = end;
        }
        Ok(())
    }

    fn is_char_boundary(&self, offset: u64) -> Result<bool, LargeDocumentError> {
        if offset == 0 || offset == self.len {
            return Ok(true);
        }
        let byte = self.read_range(offset..offset + 1)?[0];
        Ok(byte & 0b1100_0000 != 0b1000_0000)
    }

    pub fn save_atomic(&mut self, path: impl AsRef<Path>) -> Result<(), LargeDocumentError> {
        self.save_atomic_cancellable(path, &SearchCancellation::default())
    }

    pub fn save_atomic_cancellable(
        &mut self,
        path: impl AsRef<Path>,
        cancellation: &SearchCancellation,
    ) -> Result<(), LargeDocumentError> {
        let path = path.as_ref();
        if path == self.source()?.path() && self.source()?.identity()? != self.base_identity {
            return Err(LargeDocumentError::SourceChanged);
        }
        let parent = path.parent().unwrap_or_else(|| Path::new("."));
        let mut temporary =
            tempfile::NamedTempFile::new_in(parent).map_err(|source| LargeDocumentError::Io {
                path: parent.to_path_buf(),
                source,
            })?;
        const COPY_CHUNK: u64 = 8 * 1024 * 1024;
        let mut offset = 0u64;
        while offset < self.len {
            if cancellation.is_cancelled() {
                return Err(LargeDocumentError::Cancelled);
            }
            let end = (offset + COPY_CHUNK).min(self.len);
            let bytes = self.read_range(offset..end)?;
            temporary
                .write_all(&bytes)
                .map_err(|source| LargeDocumentError::Io {
                    path: temporary.path().to_path_buf(),
                    source,
                })?;
            offset = end;
        }
        temporary
            .as_file()
            .sync_all()
            .map_err(|source| LargeDocumentError::Io {
                path: temporary.path().to_path_buf(),
                source,
            })?;
        if let Ok(metadata) = std::fs::metadata(path) {
            temporary
                .as_file()
                .set_permissions(metadata.permissions())
                .map_err(|source| LargeDocumentError::Io {
                    path: temporary.path().to_path_buf(),
                    source,
                })?;
        }
        // 写临时文件可能持续数分钟；替换前必须再次核验 live identity，不能用
        // 保存开始时的检查覆盖期间发生的外部修改。
        if path == self.source()?.path() && self.source()?.identity()? != self.base_identity {
            return Err(LargeDocumentError::SourceChanged);
        }
        if cancellation.is_cancelled() {
            return Err(LargeDocumentError::Cancelled);
        }
        // Windows 目标被当前进程持有时无法原子替换；所有 base piece 已写完，可安全关闭句柄。
        self.source.take();
        if let Err(error) = crate::source::persist_temporary(temporary, path) {
            self.source = FileSource::open(path).ok();
            return Err(error);
        }
        crate::source::sync_parent_directory(parent)?;

        let source = FileSource::open(path)?;
        let index = LineIndex::build(&source)?;
        self.base_identity = source.identity()?;
        self.base_sample = source.sampled_prefix_hash(self.len)?;
        self.source = Some(source);
        self.base_index = index.clone();
        self.pieces = PieceTree::from_iter((self.len > 0).then_some(Piece {
            source: PieceSource::Base,
            range: 0..self.len,
            newlines: index.newline_count(),
        }));
        self.additions = AppendStore::default();
        self.undo.clear();
        self.redo.clear();
        Ok(())
    }

    /// 将源码选区流式导出到独立文件；不物化完整选区，也不改变文档 pristine/history。
    pub fn save_range_atomic_cancellable(
        &self,
        range: Range<u64>,
        path: impl AsRef<Path>,
        cancellation: &SearchCancellation,
    ) -> Result<(), LargeDocumentError> {
        if range.start > range.end || range.end > self.len {
            return Err(LargeDocumentError::InvalidRange {
                start: range.start,
                end: range.end,
                len: self.len,
            });
        }
        let path = path.as_ref();
        let parent = path.parent().unwrap_or_else(|| Path::new("."));
        let mut temporary =
            tempfile::NamedTempFile::new_in(parent).map_err(|source| LargeDocumentError::Io {
                path: parent.to_path_buf(),
                source,
            })?;
        const COPY_CHUNK: u64 = 8 * 1024 * 1024;
        let mut offset = range.start;
        while offset < range.end {
            if cancellation.is_cancelled() {
                return Err(LargeDocumentError::Cancelled);
            }
            let end = offset.saturating_add(COPY_CHUNK).min(range.end);
            let bytes = self.read_range(offset..end)?;
            temporary
                .write_all(&bytes)
                .map_err(|source| LargeDocumentError::Io {
                    path: temporary.path().to_path_buf(),
                    source,
                })?;
            offset = end;
        }
        temporary
            .as_file()
            .sync_all()
            .map_err(|source| LargeDocumentError::Io {
                path: temporary.path().to_path_buf(),
                source,
            })?;
        crate::source::persist_temporary(temporary, path)?;
        crate::source::sync_parent_directory(parent)
    }

    fn source(&self) -> Result<&FileSource, LargeDocumentError> {
        self.source.as_ref().ok_or_else(|| LargeDocumentError::Io {
            path: self.base_identity.path.clone(),
            source: std::io::Error::other("base source is temporarily unavailable"),
        })
    }

    pub(super) fn slice_piece(
        &self,
        piece: &Piece,
        relative: Range<u64>,
    ) -> Result<Piece, LargeDocumentError> {
        let range = piece.range.start + relative.start..piece.range.start + relative.end;
        let newlines = match piece.source {
            PieceSource::Base => self.base_index.newline_count_in(range.clone()),
            PieceSource::Add => self
                .additions
                .read(range.clone())?
                .iter()
                .filter(|byte| **byte == b'\n')
                .count() as u64,
        };
        Ok(Piece {
            source: piece.source,
            range,
            newlines,
        })
    }

    pub(super) fn logical_newline_offset(&self, newline_index: u64) -> Option<u64> {
        let mut cursor = self.pieces.root.cursor::<Dimensions<Newlines, Bytes>>(());
        // seek 的 bool 只表示目标是否恰落在 item 边界；目标位于一个多换行
        // Piece 内部时会返回 false，但 cursor.item() 仍是所需 Piece。
        cursor.seek(&Newlines(newline_index), Bias::Right);
        let piece = cursor.item()?;
        let remaining = newline_index.checked_sub(cursor.start().0.0)?;
        let logical_start = cursor.start().1.0;
        let source_offset = match piece.source {
            PieceSource::Base => self
                .base_index
                .newline_offset_in(piece.range.clone(), remaining)?,
            PieceSource::Add => {
                let bytes = self.additions.read(piece.range.clone()).ok()?;
                let relative =
                    memchr::memchr_iter(b'\n', &bytes).nth(usize::try_from(remaining).ok()?)?;
                piece.range.start + relative as u64 + 1
            }
        };
        Some(logical_start + source_offset - piece.range.start)
    }
}
