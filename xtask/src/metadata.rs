// @author kongweiguang

//! Cargo metadata loading without adding runtime dependencies to `xtask`.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Clone, Debug)]
pub(crate) struct Package {
    pub(crate) dependencies: BTreeSet<String>,
    pub(crate) manifest_path: PathBuf,
    pub(crate) name: String,
}

#[derive(Debug)]
pub(crate) struct WorkspaceMetadata {
    packages: BTreeMap<String, Package>,
}

impl WorkspaceMetadata {
    pub(crate) fn package(&self, name: &str) -> Option<&Package> {
        self.packages.get(name)
    }

    pub(crate) fn is_workspace_package(&self, name: &str) -> bool {
        self.packages.contains_key(name)
    }
}

pub(crate) fn load(root: &Path) -> Result<WorkspaceMetadata, String> {
    let manifest_path = root.join("Cargo.toml");
    if !manifest_path.is_file() {
        return Err(format!(
            "architecture gate requires '{}'; Cargo metadata cannot be loaded",
            manifest_path.display()
        ));
    }
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let output = Command::new(cargo)
        .current_dir(root)
        .arg("metadata")
        .arg("--no-deps")
        .arg("--format-version=1")
        .arg("--manifest-path")
        .arg(&manifest_path)
        .output()
        .map_err(|error| format!("failed to execute Cargo metadata: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "Cargo metadata failed for '{}': {}",
            manifest_path.display(),
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    parse_metadata(&String::from_utf8_lossy(&output.stdout))
}

fn parse_metadata(input: &str) -> Result<WorkspaceMetadata, String> {
    let value = JsonParser::new(input).parse()?;
    let packages = value
        .field("packages")
        .and_then(JsonValue::array)
        .ok_or_else(|| "Cargo metadata did not contain a packages array".to_owned())?;
    let mut parsed = BTreeMap::new();
    for package in packages {
        let name = package
            .field("name")
            .and_then(JsonValue::string)
            .ok_or_else(|| "Cargo metadata package was missing a name".to_owned())?
            .to_owned();
        let manifest_path = package
            .field("manifest_path")
            .and_then(JsonValue::string)
            .ok_or_else(|| {
                format!("Cargo metadata package '{name}' was missing a manifest path")
            })?;
        let dependencies = package
            .field("dependencies")
            .and_then(JsonValue::array)
            .ok_or_else(|| format!("Cargo metadata package '{name}' was missing dependencies"))?
            .iter()
            .filter_map(|dependency| dependency.field("name").and_then(JsonValue::string))
            .map(str::to_owned)
            .collect();
        parsed.insert(
            name.clone(),
            Package {
                dependencies,
                manifest_path: PathBuf::from(manifest_path),
                name,
            },
        );
    }
    Ok(WorkspaceMetadata { packages: parsed })
}

#[derive(Debug)]
enum JsonValue {
    Array(Vec<JsonValue>),
    Object(BTreeMap<String, JsonValue>),
    Other,
    String(String),
}

impl JsonValue {
    fn array(&self) -> Option<&[JsonValue]> {
        match self {
            Self::Array(values) => Some(values),
            _ => None,
        }
    }

    fn field(&self, name: &str) -> Option<&JsonValue> {
        match self {
            Self::Object(values) => values.get(name),
            _ => None,
        }
    }

    fn string(&self) -> Option<&str> {
        match self {
            Self::String(value) => Some(value),
            _ => None,
        }
    }
}

struct JsonParser<'a> {
    bytes: &'a [u8],
    cursor: usize,
}

impl<'a> JsonParser<'a> {
    fn new(input: &'a str) -> Self {
        Self {
            bytes: input.as_bytes(),
            cursor: 0,
        }
    }

    fn parse(mut self) -> Result<JsonValue, String> {
        let value = self.value()?;
        self.whitespace();
        if self.cursor == self.bytes.len() {
            Ok(value)
        } else {
            Err(self.error("unexpected trailing JSON"))
        }
    }

    fn value(&mut self) -> Result<JsonValue, String> {
        self.whitespace();
        match self.peek() {
            Some(b'{') => self.object(),
            Some(b'[') => self.array(),
            Some(b'"') => self.string().map(JsonValue::String),
            Some(b't') => self.literal(b"true"),
            Some(b'f') => self.literal(b"false"),
            Some(b'n') => self.literal(b"null"),
            Some(b'-' | b'0'..=b'9') => self.number(),
            Some(_) => Err(self.error("invalid JSON value")),
            None => Err(self.error("unexpected end of JSON")),
        }
    }

    fn object(&mut self) -> Result<JsonValue, String> {
        self.expect(b'{')?;
        self.whitespace();
        let mut values = BTreeMap::new();
        if self.consume(b'}') {
            return Ok(JsonValue::Object(values));
        }
        loop {
            self.whitespace();
            let key = self.string()?;
            self.whitespace();
            self.expect(b':')?;
            let value = self.value()?;
            values.insert(key, value);
            self.whitespace();
            if self.consume(b'}') {
                return Ok(JsonValue::Object(values));
            }
            self.expect(b',')?;
        }
    }

    fn array(&mut self) -> Result<JsonValue, String> {
        self.expect(b'[')?;
        self.whitespace();
        let mut values = Vec::new();
        if self.consume(b']') {
            return Ok(JsonValue::Array(values));
        }
        loop {
            values.push(self.value()?);
            self.whitespace();
            if self.consume(b']') {
                return Ok(JsonValue::Array(values));
            }
            self.expect(b',')?;
        }
    }

    fn string(&mut self) -> Result<String, String> {
        self.expect(b'"')?;
        let mut value = Vec::new();
        while let Some(byte) = self.peek() {
            self.cursor += 1;
            match byte {
                b'"' => {
                    return String::from_utf8(value)
                        .map_err(|_| self.error("invalid UTF-8 in JSON string"));
                }
                b'\\' => self.escape(&mut value)?,
                0..=0x1f => return Err(self.error("control character in JSON string")),
                _ => value.push(byte),
            }
        }
        Err(self.error("unterminated JSON string"))
    }

    fn escape(&mut self, value: &mut Vec<u8>) -> Result<(), String> {
        let escaped = self
            .peek()
            .ok_or_else(|| self.error("unterminated JSON escape"))?;
        self.cursor += 1;
        match escaped {
            b'"' | b'\\' | b'/' => value.push(escaped),
            b'b' => value.push(0x08),
            b'f' => value.push(0x0c),
            b'n' => value.push(b'\n'),
            b'r' => value.push(b'\r'),
            b't' => value.push(b'\t'),
            b'u' => {
                let code_point = self.hex_code_point()?;
                let code_point = if (0xd800..=0xdbff).contains(&code_point) {
                    self.expect(b'\\')?;
                    self.expect(b'u')?;
                    let low = self.hex_code_point()?;
                    if !(0xdc00..=0xdfff).contains(&low) {
                        return Err(self.error("invalid JSON surrogate pair"));
                    }
                    0x1_0000 + ((code_point - 0xd800) << 10) + (low - 0xdc00)
                } else {
                    code_point
                };
                let character = char::from_u32(code_point)
                    .ok_or_else(|| self.error("invalid JSON unicode escape"))?;
                let mut encoded = [0; 4];
                value.extend_from_slice(character.encode_utf8(&mut encoded).as_bytes());
            }
            _ => return Err(self.error("invalid JSON escape")),
        }
        Ok(())
    }

    fn hex_code_point(&mut self) -> Result<u32, String> {
        let start = self.cursor;
        let end = start + 4;
        let digits = self
            .bytes
            .get(start..end)
            .ok_or_else(|| self.error("truncated JSON unicode escape"))?;
        self.cursor = end;
        std::str::from_utf8(digits)
            .ok()
            .and_then(|digits| u32::from_str_radix(digits, 16).ok())
            .ok_or_else(|| self.error("invalid JSON unicode escape"))
    }

    fn literal(&mut self, expected: &[u8]) -> Result<JsonValue, String> {
        if self.bytes.get(self.cursor..self.cursor + expected.len()) == Some(expected) {
            self.cursor += expected.len();
            Ok(JsonValue::Other)
        } else {
            Err(self.error("invalid JSON literal"))
        }
    }

    fn number(&mut self) -> Result<JsonValue, String> {
        let start = self.cursor;
        while self
            .peek()
            .is_some_and(|byte| matches!(byte, b'-' | b'+' | b'.' | b'e' | b'E' | b'0'..=b'9'))
        {
            self.cursor += 1;
        }
        (self.cursor > start)
            .then_some(JsonValue::Other)
            .ok_or_else(|| self.error("invalid JSON number"))
    }

    fn consume(&mut self, expected: u8) -> bool {
        if self.peek() == Some(expected) {
            self.cursor += 1;
            true
        } else {
            false
        }
    }

    fn expect(&mut self, expected: u8) -> Result<(), String> {
        self.consume(expected)
            .then_some(())
            .ok_or_else(|| self.error(&format!("expected '{}'", expected as char)))
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.cursor).copied()
    }

    fn whitespace(&mut self) {
        while self.peek().is_some_and(|byte| byte.is_ascii_whitespace()) {
            self.cursor += 1;
        }
    }

    fn error(&self, message: &str) -> String {
        format!(
            "failed to parse Cargo metadata JSON at byte {}: {message}",
            self.cursor
        )
    }
}
