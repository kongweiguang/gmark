// @author kongweiguang

use std::fmt;

use crate::SourceLanguage;

/// 纯领域格式化入口的结果；调用方据此决定是否创建编辑事务。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FormatResult {
    pub language: SourceLanguage,
    pub text: String,
    pub changed: bool,
}

/// 格式化失败时不返回候选正文，因此调用方可以安全保留当前 revision。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FormatterError {
    InvalidJson {
        line: usize,
        column: usize,
        message: String,
    },
    InvalidJsonLine {
        record: usize,
        column: usize,
        message: String,
    },
    Unavailable {
        language: SourceLanguage,
    },
}

impl fmt::Display for FormatterError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidJson {
                line,
                column,
                message,
            } => write!(formatter, "JSON 第 {line} 行第 {column} 列无效：{message}"),
            Self::InvalidJsonLine {
                record,
                column,
                message,
            } => write!(
                formatter,
                "JSONL 第 {record} 条记录第 {column} 列无效：{message}"
            ),
            Self::Unavailable { language } => write!(
                formatter,
                "未给 {} 提供内置格式化器；请由适配器解析外部格式化器配置",
                language.canonical_name()
            ),
        }
    }
}

impl std::error::Error for FormatterError {}

/// 运行内置格式化器。外部命令、文件配置与进程生命周期由 Wave 2 适配器负责。
pub fn format_source(
    language: SourceLanguage,
    source: &str,
) -> Result<FormatResult, FormatterError> {
    let text = match language {
        SourceLanguage::Json => format_json(source)?,
        SourceLanguage::JsonLines => format_json_lines(source)?,
        _ => return Err(FormatterError::Unavailable { language }),
    };
    let changed = text != source;
    Ok(FormatResult {
        language,
        text,
        changed,
    })
}

/// 格式化完整 JSON，但保持 key 顺序、数字文本和字符串转义的原始词法。
pub fn format_json(source: &str) -> Result<String, FormatterError> {
    let tokens = parse_json(source).map_err(|error| json_error(source, error))?;
    Ok(render_tokens(source, &tokens, false))
}

/// 格式化 JSON Lines：每条非空记录保持一行，空行不被吞并。
pub fn format_json_lines(source: &str) -> Result<String, FormatterError> {
    let trailing_newline = source.ends_with('\n');
    let mut output = String::new();
    for (index, line) in source.lines().enumerate() {
        if index > 0 {
            output.push('\n');
        }
        if line.trim().is_empty() {
            continue;
        }
        let tokens = parse_json(line).map_err(|error| FormatterError::InvalidJsonLine {
            record: index + 1,
            column: line_and_column(line, error.offset).1,
            message: error.message.to_owned(),
        })?;
        output.push_str(&render_tokens(line, &tokens, true));
    }
    if trailing_newline && !output.ends_with('\n') {
        output.push('\n');
    }
    Ok(output)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum JsonToken {
    OpenObject,
    CloseObject,
    OpenArray,
    CloseArray,
    Comma,
    Colon,
    Value { start: usize, end: usize },
}

#[derive(Clone, Debug)]
struct JsonSyntaxError {
    offset: usize,
    message: &'static str,
}

struct JsonParser<'source> {
    source: &'source str,
    position: usize,
    tokens: Vec<JsonToken>,
}

impl<'source> JsonParser<'source> {
    fn parse(mut self) -> Result<Vec<JsonToken>, JsonSyntaxError> {
        self.skip_whitespace();
        if self.position == self.source.len() {
            return Err(self.error("缺少 JSON 值"));
        }
        self.parse_value()?;
        self.skip_whitespace();
        if self.position != self.source.len() {
            return Err(self.error("JSON 值后存在额外内容"));
        }
        Ok(self.tokens)
    }

    fn parse_value(&mut self) -> Result<(), JsonSyntaxError> {
        match self.peek_byte() {
            Some(b'{') => self.parse_object(),
            Some(b'[') => self.parse_array(),
            Some(b'"') => self.parse_string(),
            Some(b'-' | b'0'..=b'9') => self.parse_number(),
            Some(b't') => self.parse_literal(b"true"),
            Some(b'f') => self.parse_literal(b"false"),
            Some(b'n') => self.parse_literal(b"null"),
            _ => Err(self.error("此处需要 JSON 值")),
        }
    }

    fn parse_object(&mut self) -> Result<(), JsonSyntaxError> {
        self.position += 1;
        self.tokens.push(JsonToken::OpenObject);
        self.skip_whitespace();
        if self.consume(b'}') {
            self.tokens.push(JsonToken::CloseObject);
            return Ok(());
        }

        loop {
            if self.peek_byte() != Some(b'"') {
                return Err(self.error("对象 key 必须是字符串"));
            }
            self.parse_string()?;
            self.skip_whitespace();
            self.require(b':', "对象 key 后缺少 ':'")?;
            self.tokens.push(JsonToken::Colon);
            self.skip_whitespace();
            self.parse_value()?;
            self.skip_whitespace();
            if self.consume(b',') {
                self.tokens.push(JsonToken::Comma);
                self.skip_whitespace();
                continue;
            }
            if self.consume(b'}') {
                self.tokens.push(JsonToken::CloseObject);
                return Ok(());
            }
            return Err(self.error("对象成员后需要 ',' 或 '}'"));
        }
    }

    fn parse_array(&mut self) -> Result<(), JsonSyntaxError> {
        self.position += 1;
        self.tokens.push(JsonToken::OpenArray);
        self.skip_whitespace();
        if self.consume(b']') {
            self.tokens.push(JsonToken::CloseArray);
            return Ok(());
        }

        loop {
            self.parse_value()?;
            self.skip_whitespace();
            if self.consume(b',') {
                self.tokens.push(JsonToken::Comma);
                self.skip_whitespace();
                continue;
            }
            if self.consume(b']') {
                self.tokens.push(JsonToken::CloseArray);
                return Ok(());
            }
            return Err(self.error("数组成员后需要 ',' 或 ']'"));
        }
    }

    fn parse_string(&mut self) -> Result<(), JsonSyntaxError> {
        let start = self.position;
        self.position += 1;
        while self.position < self.source.len() {
            let Some(character) = self
                .source
                .get(self.position..)
                .and_then(|tail| tail.chars().next())
            else {
                return Err(self.error("字符串不是有效 UTF-8"));
            };
            if character == '"' {
                self.position += character.len_utf8();
                self.tokens.push(JsonToken::Value {
                    start,
                    end: self.position,
                });
                return Ok(());
            }
            if character == '\\' {
                self.position += 1;
                self.parse_escape()?;
                continue;
            }
            if character <= '\u{001f}' {
                return Err(self.error("字符串包含未转义控制字符"));
            }
            self.position += character.len_utf8();
        }
        Err(self.error("字符串没有闭合引号"))
    }

    fn parse_escape(&mut self) -> Result<(), JsonSyntaxError> {
        let Some(escape) = self
            .source
            .get(self.position..)
            .and_then(|tail| tail.chars().next())
        else {
            return Err(self.error("字符串转义不完整"));
        };
        if escape == 'u' {
            self.position += 1;
            let Some(end) = self.position.checked_add(4) else {
                return Err(self.error("Unicode 转义长度溢出"));
            };
            let Some(digits) = self.source.as_bytes().get(self.position..end) else {
                return Err(self.error("Unicode 转义不完整"));
            };
            if !digits.iter().all(u8::is_ascii_hexdigit) {
                return Err(self.error("Unicode 转义必须包含四个十六进制数字"));
            }
            self.position = end;
            return Ok(());
        }
        if matches!(escape, '"' | '\\' | '/' | 'b' | 'f' | 'n' | 'r' | 't') {
            self.position += escape.len_utf8();
            return Ok(());
        }
        Err(self.error("字符串转义无效"))
    }

    fn parse_number(&mut self) -> Result<(), JsonSyntaxError> {
        let start = self.position;
        self.consume(b'-');
        match self.peek_byte() {
            Some(b'0') => {
                self.position += 1;
                if self.peek_byte().is_some_and(|byte| byte.is_ascii_digit()) {
                    return Err(self.error("数字不能有前导零"));
                }
            }
            Some(b'1'..=b'9') => {
                self.consume_digits();
            }
            _ => return Err(self.error("数字整数部分无效")),
        }
        if self.consume(b'.') && !self.consume_digits() {
            return Err(self.error("小数点后缺少数字"));
        }
        if self
            .peek_byte()
            .is_some_and(|byte| matches!(byte, b'e' | b'E'))
        {
            self.position += 1;
            if self
                .peek_byte()
                .is_some_and(|byte| matches!(byte, b'+' | b'-'))
            {
                self.position += 1;
            }
            if !self.consume_digits() {
                return Err(self.error("指数部分缺少数字"));
            }
        }
        self.tokens.push(JsonToken::Value {
            start,
            end: self.position,
        });
        Ok(())
    }

    fn parse_literal(&mut self, literal: &[u8]) -> Result<(), JsonSyntaxError> {
        let start = self.position;
        let Some(remaining) = self.source.as_bytes().get(self.position..) else {
            return Err(self.error("字面量不完整"));
        };
        if !remaining.starts_with(literal) {
            return Err(self.error("JSON 字面量无效"));
        }
        let Some(end) = self.position.checked_add(literal.len()) else {
            return Err(self.error("字面量长度溢出"));
        };
        self.position = end;
        self.tokens.push(JsonToken::Value { start, end });
        Ok(())
    }

    fn skip_whitespace(&mut self) {
        while self.peek_byte().is_some_and(is_json_whitespace) {
            self.position += 1;
        }
    }

    fn require(&mut self, byte: u8, message: &'static str) -> Result<(), JsonSyntaxError> {
        if self.consume(byte) {
            Ok(())
        } else {
            Err(self.error(message))
        }
    }

    fn consume(&mut self, byte: u8) -> bool {
        if self.peek_byte() == Some(byte) {
            self.position += 1;
            true
        } else {
            false
        }
    }

    fn consume_digits(&mut self) -> bool {
        let start = self.position;
        while self.peek_byte().is_some_and(|byte| byte.is_ascii_digit()) {
            self.position += 1;
        }
        self.position > start
    }

    fn peek_byte(&self) -> Option<u8> {
        self.source.as_bytes().get(self.position).copied()
    }

    fn error(&self, message: &'static str) -> JsonSyntaxError {
        JsonSyntaxError {
            offset: self.position,
            message,
        }
    }
}

fn parse_json(source: &str) -> Result<Vec<JsonToken>, JsonSyntaxError> {
    JsonParser {
        source,
        position: 0,
        tokens: Vec::new(),
    }
    .parse()
}

fn is_json_whitespace(byte: u8) -> bool {
    matches!(byte, b' ' | b'\n' | b'\r' | b'\t')
}

fn json_error(source: &str, error: JsonSyntaxError) -> FormatterError {
    let (line, column) = line_and_column(source, error.offset);
    FormatterError::InvalidJson {
        line,
        column,
        message: error.message.to_owned(),
    }
}

fn line_and_column(source: &str, offset: usize) -> (usize, usize) {
    let prefix = source.get(..offset.min(source.len())).unwrap_or_default();
    let line = prefix.bytes().filter(|byte| *byte == b'\n').count() + 1;
    let column = prefix
        .rsplit('\n')
        .next()
        .map_or(1, |line| line.chars().count() + 1);
    (line, column)
}

fn render_tokens(source: &str, tokens: &[JsonToken], compact: bool) -> String {
    let mut output = String::with_capacity(source.len().saturating_add(source.len() / 8));
    let mut depth = 0;
    for (index, token) in tokens.iter().enumerate() {
        match *token {
            JsonToken::OpenObject => {
                output.push('{');
                if !matches!(tokens.get(index + 1), Some(JsonToken::CloseObject)) {
                    depth += 1;
                    separator(&mut output, compact, depth);
                }
            }
            JsonToken::OpenArray => {
                output.push('[');
                if !matches!(tokens.get(index + 1), Some(JsonToken::CloseArray)) {
                    depth += 1;
                    separator(&mut output, compact, depth);
                }
            }
            JsonToken::CloseObject => close_token(&mut output, compact, &mut depth, '{', '}'),
            JsonToken::CloseArray => close_token(&mut output, compact, &mut depth, '[', ']'),
            JsonToken::Comma => {
                output.push(',');
                separator(&mut output, compact, depth);
            }
            JsonToken::Colon => {
                output.push(':');
                if !compact {
                    output.push(' ');
                }
            }
            JsonToken::Value { start, end } => {
                if let Some(value) = source.get(start..end) {
                    output.push_str(value);
                }
            }
        }
    }
    if source.ends_with('\n') && !output.ends_with('\n') {
        output.push('\n');
    }
    output
}

fn close_token(
    output: &mut String,
    compact: bool,
    depth: &mut usize,
    opening: char,
    closing: char,
) {
    if output.as_bytes().last().copied() != Some(opening as u8) {
        *depth = depth.saturating_sub(1);
        separator(output, compact, *depth);
    }
    output.push(closing);
}

fn separator(output: &mut String, compact: bool, depth: usize) {
    if compact {
        return;
    }
    output.push('\n');
    output.extend(std::iter::repeat_n(' ', depth.saturating_mul(2)));
}
