// @author kongweiguang

use super::model::{StringToken, Token};
use super::*;
use crate::{JsonGraphError, JsonValueKind};

impl<'a> GraphParser<'a> {
    pub(super) fn next_token(&mut self) -> Result<Token, JsonGraphError> {
        self.cursor.skip_whitespace()?;
        let start = self.cursor.position();
        let Some(byte) = self.cursor.bump()? else {
            return Ok(Token::Eof(start));
        };
        match byte {
            b'{' => Ok(Token::ObjectStart(start)),
            b'}' => Ok(Token::ObjectEnd(self.cursor.position())),
            b'[' => Ok(Token::ArrayStart(start)),
            b']' => Ok(Token::ArrayEnd(self.cursor.position())),
            b':' => Ok(Token::Colon(start)),
            b',' => Ok(Token::Comma(start)),
            b'"' => self.read_string(start).map(Token::String),
            b't' => self.read_literal(start, b"rue", "true", JsonValueKind::Boolean),
            b'f' => self.read_literal(start, b"alse", "false", JsonValueKind::Boolean),
            b'n' => self.read_literal(start, b"ull", "null", JsonValueKind::Null),
            b'-' | b'0'..=b'9' => self.read_number(start, byte),
            _ => Err(self.invalid(start, "invalid JSON token")),
        }
    }

    fn read_literal(
        &mut self,
        start: u64,
        tail: &[u8],
        display: &str,
        kind: JsonValueKind,
    ) -> Result<Token, JsonGraphError> {
        for expected in tail {
            if self.cursor.bump()? != Some(*expected) {
                return Err(self.invalid(self.cursor.position(), "invalid JSON literal"));
            }
        }
        Ok(Token::Scalar {
            start,
            end: self.cursor.position(),
            display: display.to_owned(),
            kind,
        })
    }

    fn read_string(&mut self, start: u64) -> Result<StringToken, JsonGraphError> {
        let mut raw = vec![b'"'];
        let mut escaped = false;
        loop {
            let Some(byte) = self.cursor.bump()? else {
                return Err(self.invalid(start, "unterminated string"));
            };
            if raw.len() <= DISPLAY_TEXT_BYTES * 4 {
                raw.push(byte);
            }
            if escaped {
                if byte == b'u' {
                    for _ in 0..4 {
                        let Some(hex) = self.cursor.bump()? else {
                            return Err(self.invalid(start, "unterminated unicode escape"));
                        };
                        if !hex.is_ascii_hexdigit() {
                            return Err(
                                self.invalid(self.cursor.position() - 1, "invalid unicode escape")
                            );
                        }
                        if raw.len() <= DISPLAY_TEXT_BYTES * 4 {
                            raw.push(hex);
                        }
                    }
                } else if !matches!(byte, b'"' | b'\\' | b'/' | b'b' | b'f' | b'n' | b'r' | b't') {
                    return Err(self.invalid(self.cursor.position() - 1, "invalid string escape"));
                }
                escaped = false;
                continue;
            }
            match byte {
                b'\\' => escaped = true,
                b'"' => break,
                0x00..=0x1f => {
                    return Err(
                        self.invalid(self.cursor.position() - 1, "control character in string")
                    );
                }
                _ => {}
            }
        }
        let display = decode_bounded_string_prefix(raw)
            .ok_or_else(|| self.invalid(start, "invalid Unicode string escape"))?;
        Ok(StringToken {
            start,
            end: self.cursor.position(),
            display: truncate_display(display),
        })
    }

    fn read_number(&mut self, start: u64, first: u8) -> Result<Token, JsonGraphError> {
        let mut bytes = vec![first];
        let mut push = |byte| {
            if bytes.len() < DISPLAY_TEXT_BYTES {
                bytes.push(byte);
            }
        };
        let first_digit = if first == b'-' {
            let Some(digit) = self.cursor.bump()? else {
                return Err(self.invalid(start, "number is missing an integer part"));
            };
            if !digit.is_ascii_digit() {
                return Err(self.invalid(self.cursor.position() - 1, "invalid number"));
            }
            push(digit);
            digit
        } else {
            first
        };
        if first_digit == b'0'
            && self
                .cursor
                .peek()?
                .is_some_and(|byte| byte.is_ascii_digit())
        {
            return Err(self.invalid(self.cursor.position(), "leading zero in number"));
        }
        while self
            .cursor
            .peek()?
            .is_some_and(|byte| byte.is_ascii_digit())
        {
            push(self.bump_required(start, "number ended unexpectedly")?);
        }
        if self.cursor.peek()? == Some(b'.') {
            push(self.bump_required(start, "number ended after decimal point")?);
            if !self
                .cursor
                .peek()?
                .is_some_and(|byte| byte.is_ascii_digit())
            {
                return Err(self.invalid(self.cursor.position(), "fraction is missing digits"));
            }
            while self
                .cursor
                .peek()?
                .is_some_and(|byte| byte.is_ascii_digit())
            {
                push(self.bump_required(start, "fraction ended unexpectedly")?);
            }
        }
        if self
            .cursor
            .peek()?
            .is_some_and(|byte| matches!(byte, b'e' | b'E'))
        {
            push(self.bump_required(start, "exponent ended unexpectedly")?);
            if self
                .cursor
                .peek()?
                .is_some_and(|byte| matches!(byte, b'+' | b'-'))
            {
                push(self.bump_required(start, "exponent sign has no digits")?);
            }
            if !self
                .cursor
                .peek()?
                .is_some_and(|byte| byte.is_ascii_digit())
            {
                return Err(self.invalid(self.cursor.position(), "exponent is missing digits"));
            }
            while self
                .cursor
                .peek()?
                .is_some_and(|byte| byte.is_ascii_digit())
            {
                push(self.bump_required(start, "exponent ended unexpectedly")?);
            }
        }
        let display = String::from_utf8_lossy(&bytes).into_owned();
        Ok(Token::Scalar {
            start,
            end: self.cursor.position(),
            display,
            kind: JsonValueKind::Number,
        })
    }

    pub(super) fn invalid(&self, offset: u64, message: impl Into<String>) -> JsonGraphError {
        JsonGraphError::InvalidJson {
            offset,
            message: message.into(),
        }
    }

    fn bump_required(&mut self, offset: u64, message: &'static str) -> Result<u8, JsonGraphError> {
        self.cursor
            .bump()?
            .ok_or_else(|| JsonGraphError::InvalidJson {
                offset,
                message: message.to_owned(),
            })
    }
}

pub(super) fn token_offset(token: &Token) -> u64 {
    match token {
        Token::ObjectStart(offset)
        | Token::ArrayStart(offset)
        | Token::Colon(offset)
        | Token::Comma(offset)
        | Token::Eof(offset) => *offset,
        Token::ObjectEnd(offset) | Token::ArrayEnd(offset) => offset.saturating_sub(1),
        Token::String(value) => value.start,
        Token::Scalar { start, .. } => *start,
    }
}

pub(super) fn escape_pointer_segment(value: &str) -> String {
    value.replace('~', "~0").replace('/', "~1")
}

fn truncate_display(mut value: String) -> String {
    if value.chars().count() <= DISPLAY_TEXT_BYTES {
        return value;
    }
    value = value.chars().take(DISPLAY_TEXT_BYTES).collect();
    value.push('…');
    value
}

/// 长字符串不物化完整值；给有界 JSON 前缀补上引号，并退到最后一个完整的
/// UTF-8/escape 边界后再解码，避免把半个 `\uXXXX` 或多字节字符显示成乱码。
fn decode_bounded_string_prefix(mut raw: Vec<u8>) -> Option<String> {
    let truncated = raw.last() != Some(&b'"');
    if !truncated {
        return serde_json::from_slice::<String>(&raw)
            .ok()
            .map(truncate_display);
    }
    raw.push(b'"');
    loop {
        if let Ok(mut decoded) = serde_json::from_slice::<String>(&raw) {
            decoded.push('…');
            return Some(truncate_display(decoded));
        }
        if raw.len() <= 2 {
            return Some("…".to_owned());
        }
        raw.remove(raw.len() - 2);
    }
}
