// @author kongweiguang

use super::{MathDelimiterPair, MathNode};

pub(super) fn parse(source: &str) -> MathNode {
    parse_environment(source).unwrap_or_else(|| Parser::new(source).parse_sequence(None))
}

struct Parser<'a> {
    source: &'a str,
    offset: usize,
}

impl<'a> Parser<'a> {
    fn new(source: &'a str) -> Self {
        Self { source, offset: 0 }
    }

    fn parse_sequence(&mut self, terminator: Option<char>) -> MathNode {
        let mut children = Vec::new();
        let mut text = String::new();
        while let Some(character) = self.peek() {
            if Some(character) == terminator {
                break;
            }
            match character {
                '{' => {
                    flush_text(&mut children, &mut text);
                    children.push(self.parse_group_or_opaque());
                }
                '}' if terminator.is_none() => {
                    flush_text(&mut children, &mut text);
                    children.push(MathNode::Opaque(
                        self.take_char().unwrap_or_default().to_string(),
                    ));
                }
                '\\' => {
                    flush_text(&mut children, &mut text);
                    children.push(self.parse_command());
                }
                '^' | '_' => {
                    flush_text(&mut children, &mut text);
                    children.push(self.parse_script());
                }
                _ => text.push(self.take_char().unwrap_or_default()),
            }
        }
        flush_text(&mut children, &mut text);
        MathNode::Sequence(children)
    }

    fn parse_group_or_opaque(&mut self) -> MathNode {
        let start = self.offset;
        self.take_char();
        let content = self.parse_sequence(Some('}'));
        if self.peek() == Some('}') {
            self.take_char();
            MathNode::Group(Box::new(content))
        } else {
            MathNode::Opaque(self.source[start..self.offset].to_owned())
        }
    }

    fn parse_command(&mut self) -> MathNode {
        let start = self.offset;
        self.take_char();
        let name = self.take_command_name();
        if name.is_empty() {
            return MathNode::Opaque(self.source[start..self.offset].to_owned());
        }
        match name.as_str() {
            "begin" => self.parse_environment_command(start, name),
            "left" => self.parse_delimited(start, name),
            "frac" => self.parse_fraction(start, name),
            "sqrt" => self.parse_square_root(start, name),
            "text" => self.parse_text_mode(start, name),
            name if accent_command(name) => self.parse_accent(start, name),
            name if symbol_command(name) => MathNode::Symbol {
                name: name.to_owned(),
            },
            name if big_operator_command(name) => MathNode::BigOperator {
                name: name.to_owned(),
            },
            _ => MathNode::Command { name },
        }
    }

    fn parse_text_mode(&mut self, start: usize, name: String) -> MathNode {
        if self.peek() != Some('{') {
            return MathNode::Command { name };
        }
        match self.parse_group_or_opaque() {
            MathNode::Group(content) => MathNode::TextMode(content),
            MathNode::Opaque(_) => MathNode::Opaque(self.source[start..self.offset].to_owned()),
            other => other,
        }
    }

    fn parse_accent(&mut self, start: usize, name: &str) -> MathNode {
        let value = match self.peek() {
            Some('{') => match self.parse_group_or_opaque() {
                MathNode::Group(content) => *content,
                MathNode::Opaque(_) => {
                    return MathNode::Opaque(self.source[start..self.offset].to_owned());
                }
                other => other,
            },
            // Keep unbraced accents in their original token form.  A
            // canonical `{...}` insertion would otherwise make a parse/write
            // round trip observable before the user has edited anything.
            Some(_) => {
                return MathNode::Command {
                    name: name.to_owned(),
                };
            }
            None => {
                return MathNode::Command {
                    name: name.to_owned(),
                };
            }
        };
        MathNode::Accent {
            name: name.to_owned(),
            value: Box::new(value),
        }
    }

    fn parse_fraction(&mut self, start: usize, name: String) -> MathNode {
        if self.peek() != Some('{') {
            return MathNode::Command { name };
        }
        let numerator = match self.parse_group_or_opaque() {
            MathNode::Group(content) => *content,
            MathNode::Opaque(_) => {
                return MathNode::Opaque(self.source[start..self.offset].to_owned());
            }
            other => return other,
        };
        if self.peek() != Some('{') {
            return MathNode::Opaque(self.source[start..self.offset].to_owned());
        }
        let denominator = match self.parse_group_or_opaque() {
            MathNode::Group(content) => *content,
            MathNode::Opaque(_) => {
                return MathNode::Opaque(self.source[start..self.offset].to_owned());
            }
            other => return other,
        };
        MathNode::Fraction {
            numerator: Box::new(numerator),
            denominator: Box::new(denominator),
        }
    }

    fn parse_square_root(&mut self, start: usize, name: String) -> MathNode {
        let has_index = self.peek() == Some('[');
        let index = if has_index {
            self.parse_bracket_group().map(Box::new)
        } else {
            None
        };
        if has_index && index.is_none() {
            return MathNode::Opaque(self.source[start..self.offset].to_owned());
        }
        if self.peek() != Some('{') {
            return if index.is_some() {
                MathNode::Opaque(self.source[start..self.offset].to_owned())
            } else {
                MathNode::Command { name }
            };
        }
        let radicand = match self.parse_group_or_opaque() {
            MathNode::Group(content) => *content,
            MathNode::Opaque(_) => {
                return MathNode::Opaque(self.source[start..self.offset].to_owned());
            }
            other => return other,
        };
        MathNode::SquareRoot {
            index,
            radicand: Box::new(radicand),
        }
    }

    fn parse_environment_command(&mut self, start: usize, name: String) -> MathNode {
        let Some(end) = environment_end(self.source, start) else {
            return MathNode::Command { name };
        };
        let source = self.source[start..end].to_owned();
        let Some(open_end) = source.find('}') else {
            return MathNode::Opaque(source);
        };
        let name_end = &source[7..open_end];
        self.offset = end;
        MathNode::Environment {
            name: name_end.to_owned(),
            raw: source,
        }
    }

    fn parse_delimited(&mut self, start: usize, name: String) -> MathNode {
        let Some(open) = self.parse_delimiter_token() else {
            return MathNode::Command { name };
        };
        let Some((right_start, right_end, close)) = find_right(self.source, self.offset) else {
            self.offset = self.source.len();
            return MathNode::Opaque(self.source[start..].to_owned());
        };
        let Some(pair) = MathDelimiterPair::from_tokens(&open, &close) else {
            self.offset = right_end;
            return MathNode::Opaque(self.source[start..right_end].to_owned());
        };
        let body_source = &self.source[self.offset..right_start];
        let body = parse(body_source);
        self.offset = right_end;
        MathNode::Delimited {
            pair,
            body: Box::new(body),
        }
    }

    fn parse_delimiter_token(&mut self) -> Option<String> {
        let character = self.peek()?;
        if character == '\\' {
            self.take_char();
            let name = self.take_command_name();
            return (!name.is_empty()).then(|| format!("\\{name}"));
        }
        self.take_char().map(|character| character.to_string())
    }

    fn parse_bracket_group(&mut self) -> Option<MathNode> {
        self.take_char();
        let content = self.parse_sequence(Some(']'));
        (self.peek() == Some(']')).then(|| {
            self.take_char();
            content
        })
    }

    fn parse_script(&mut self) -> MathNode {
        let marker = self.take_char().unwrap_or_default();
        let value = match self.peek() {
            Some('{') => self.parse_group_or_opaque(),
            Some(_) => self
                .take_char()
                .map(|character| MathNode::Text(character.to_string()))
                .unwrap_or_else(|| MathNode::Opaque(marker.to_string())),
            None => return MathNode::Opaque(marker.to_string()),
        };
        match marker {
            '^' => MathNode::Superscript(Box::new(value)),
            '_' => MathNode::Subscript(Box::new(value)),
            _ => MathNode::Opaque(marker.to_string()),
        }
    }

    fn take_command_name(&mut self) -> String {
        let mut name = String::new();
        let Some(first) = self.peek() else {
            return name;
        };
        if first.is_ascii_alphabetic() {
            while self
                .peek()
                .is_some_and(|character| character.is_ascii_alphabetic())
            {
                name.push(self.take_char().unwrap_or_default());
            }
        } else {
            name.push(self.take_char().unwrap_or_default());
        }
        name
    }

    fn peek(&self) -> Option<char> {
        self.source[self.offset..].chars().next()
    }

    fn take_char(&mut self) -> Option<char> {
        let character = self.peek()?;
        self.offset += character.len_utf8();
        Some(character)
    }
}

fn flush_text(children: &mut Vec<MathNode>, text: &mut String) {
    if !text.is_empty() {
        children.push(MathNode::Text(std::mem::take(text)));
    }
}

fn parse_environment(source: &str) -> Option<MathNode> {
    let open_prefix = "\\begin{";
    let close_prefix = "\\end{";
    let rest = source.strip_prefix(open_prefix)?;
    let close_brace = rest.find('}')?;
    let name = &rest[..close_brace];
    if name.is_empty() {
        return None;
    }
    let open = format!("{open_prefix}{name}}}");
    let close = format!("{close_prefix}{name}}}");
    (source.starts_with(&open)
        && source.ends_with(&close)
        && source.len() >= open.len() + close.len())
    .then(|| MathNode::Environment {
        name: name.to_owned(),
        raw: source.to_owned(),
    })
}

/// Return the end of the environment beginning at `start`, including its
/// matching `\\end{...}`.  Nested environments are counted so a matrix cell
/// containing another matrix remains one editable outer cell.
fn environment_end(source: &str, start: usize) -> Option<usize> {
    let open = source.get(start..)?.strip_prefix("\\begin{")?;
    let close_brace = open.find('}')?;
    let name = &open[..close_brace];
    if name.is_empty() {
        return None;
    }
    let mut depth = 1usize;
    let mut offset = start + 7 + close_brace + 1;
    while offset < source.len() {
        if source[offset..].starts_with("\\begin{") {
            let end = source[offset + 7..].find('}')?;
            depth = depth.saturating_add(1);
            offset += 7 + end + 1;
            continue;
        }
        if source[offset..].starts_with("\\end{") {
            let end = source[offset + 5..].find('}')?;
            let candidate = &source[offset + 5..offset + 5 + end];
            offset += 5 + end + 1;
            if candidate == name {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return Some(offset);
                }
            }
            continue;
        }
        offset += source[offset..].chars().next()?.len_utf8();
    }
    None
}

/// Locate the first top-level `\\right...` matching a `\\left...` body.  The
/// scanner deliberately treats nested left/right pairs structurally while
/// leaving all body text untouched for the recursive parser.
fn find_right(source: &str, mut offset: usize) -> Option<(usize, usize, String)> {
    let mut nested = 0usize;
    while offset < source.len() {
        if source[offset..].starts_with("\\left") {
            nested = nested.saturating_add(1);
            offset += 5;
            continue;
        }
        if source[offset..].starts_with("\\right") {
            let start = offset;
            offset += 6;
            let character = source[offset..].chars().next()?;
            let token = if character == '\\' {
                offset += character.len_utf8();
                let name_start = offset;
                if source[offset..]
                    .chars()
                    .next()
                    .is_some_and(|value| value.is_ascii_alphabetic())
                {
                    while source[offset..]
                        .chars()
                        .next()
                        .is_some_and(|value| value.is_ascii_alphabetic())
                    {
                        offset += source[offset..].chars().next()?.len_utf8();
                    }
                    format!("\\{}", &source[name_start..offset])
                } else {
                    let width = source[offset..].chars().next()?.len_utf8();
                    offset += width;
                    format!("\\{}", &source[name_start..offset])
                }
            } else {
                offset += character.len_utf8();
                source[offset - character.len_utf8()..offset].to_owned()
            };
            if nested == 0 {
                return Some((start, offset, token));
            }
            nested = nested.saturating_sub(1);
            continue;
        }
        offset += source[offset..].chars().next()?.len_utf8();
    }
    None
}

pub(super) fn supported_environment(name: &str) -> bool {
    matches!(
        name,
        "matrix" | "pmatrix" | "bmatrix" | "cases" | "aligned" | "array"
    )
}

fn accent_command(name: &str) -> bool {
    matches!(
        name,
        "hat"
            | "bar"
            | "vec"
            | "dot"
            | "ddot"
            | "tilde"
            | "overline"
            | "underline"
            | "widehat"
            | "widetilde"
            | "breve"
            | "check"
            | "acute"
            | "grave"
    )
}

fn symbol_command(name: &str) -> bool {
    matches!(
        name,
        "alpha"
            | "beta"
            | "gamma"
            | "Delta"
            | "delta"
            | "epsilon"
            | "varepsilon"
            | "theta"
            | "lambda"
            | "mu"
            | "pi"
            | "sigma"
            | "phi"
            | "omega"
            | "infty"
            | "div"
            | "partial"
            | "nabla"
            | "in"
            | "angle"
            | "cdot"
            | "times"
            | "pm"
            | "leq"
            | "geq"
            | "neq"
            | "approx"
            | "to"
            | "rightarrow"
            | "leftarrow"
            | "ldots"
    )
}

fn big_operator_command(name: &str) -> bool {
    matches!(
        name,
        "sum"
            | "prod"
            | "coprod"
            | "int"
            | "iint"
            | "iiint"
            | "oint"
            | "lim"
            | "min"
            | "max"
            | "sup"
            | "inf"
    )
}

pub(super) fn supported_command(name: &str) -> bool {
    matches!(
        name,
        "frac"
            | "sqrt"
            | "text"
            | "hat"
            | "bar"
            | "vec"
            | "dot"
            | "ddot"
            | "tilde"
            | "overline"
            | "underline"
            | "widehat"
            | "widetilde"
            | "breve"
            | "check"
            | "acute"
            | "grave"
            | "mathrm"
            | "mathbf"
            | "mathbb"
            | "left"
            | "right"
            | "langle"
            | "rangle"
            | "lfloor"
            | "rfloor"
            | "lceil"
            | "rceil"
            | "sin"
            | "cos"
            | "tan"
            | "log"
            | "ln"
            | "lim"
            | "sum"
            | "prod"
            | "int"
            | "alpha"
            | "beta"
            | "gamma"
            | "Delta"
            | "delta"
            | "epsilon"
            | "varepsilon"
            | "theta"
            | "lambda"
            | "mu"
            | "pi"
            | "sigma"
            | "phi"
            | "omega"
            | "infty"
            | "div"
            | "partial"
            | "nabla"
            | "in"
            | "angle"
            | "cdot"
            | "times"
            | "pm"
            | "leq"
            | "geq"
            | "neq"
            | "approx"
            | "to"
            | "rightarrow"
            | "leftarrow"
            | "ldots"
            | "quad"
            | ","
            | ";"
            | "!"
    ) || (name.len() == 1 && !name.chars().next().is_some_and(char::is_alphabetic))
}
