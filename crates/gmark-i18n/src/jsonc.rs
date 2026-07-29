// @author kongweiguang

use serde_json::Value;

use crate::{I18nError, LanguagePackFormat, Result};

pub(crate) fn parse_jsonc(input: &str) -> Result<Value> {
    let without_comments = strip_comments(input)?;
    serde_json::from_str(&without_comments).map_err(|error| I18nError::InvalidJson {
        format: LanguagePackFormat::Jsonc,
        message: error.to_string(),
    })
}

/// Removes comments without altering quoted content or line positions.
pub(crate) fn strip_comments(input: &str) -> Result<String> {
    let mut output = String::with_capacity(input.len());
    let mut characters = input.chars().peekable();
    let mut in_string = false;
    let mut escaped = false;

    while let Some(character) = characters.next() {
        if in_string {
            output.push(character);
            if escaped {
                escaped = false;
            } else if character == '\\' {
                escaped = true;
            } else if character == '"' {
                in_string = false;
            }
            continue;
        }

        if character == '"' {
            in_string = true;
            output.push(character);
            continue;
        }

        if character == '/' {
            match characters.peek().copied() {
                Some('/') => {
                    let _ = characters.next();
                    for next in characters.by_ref() {
                        if next == '\n' {
                            output.push('\n');
                            break;
                        }
                    }
                    continue;
                }
                Some('*') => {
                    let _ = characters.next();
                    let mut closed = false;
                    let mut previous = '\0';
                    for next in characters.by_ref() {
                        if next == '\n' {
                            output.push('\n');
                        }
                        if previous == '*' && next == '/' {
                            closed = true;
                            break;
                        }
                        previous = next;
                    }
                    if !closed {
                        return Err(I18nError::UnterminatedJsoncComment);
                    }
                    continue;
                }
                _ => {}
            }
        }

        output.push(character);
    }

    Ok(output)
}
