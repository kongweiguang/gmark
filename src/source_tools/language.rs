// @author kongweiguang

use std::path::Path;

/// 源码视图、结构解析和格式化共同使用的规范语言标识。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum SourceLanguageId {
    Rust,
    JavaScript,
    JavaScriptJsx,
    TypeScript,
    TypeScriptTsx,
    Json,
    JsonLines,
    Markdown,
    Bash,
    C,
    Cpp,
    CSharp,
    Css,
    Go,
    Html,
    Java,
    Php,
    Python,
    Ruby,
    Yaml,
    Toml,
    Mermaid,
    PlainText,
}

impl SourceLanguageId {
    pub(crate) fn for_path(path: &Path) -> Self {
        let file_name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default();
        if file_name.eq_ignore_ascii_case("Cargo.lock") {
            return Self::Toml;
        }
        let extension = path
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();
        match extension.as_str() {
            "rs" => Self::Rust,
            "js" | "mjs" | "cjs" => Self::JavaScript,
            "jsx" => Self::JavaScriptJsx,
            "ts" | "mts" | "cts" => Self::TypeScript,
            "tsx" => Self::TypeScriptTsx,
            "json" | "jsonc" | "geojson" => Self::Json,
            "jsonl" | "ndjson" => Self::JsonLines,
            "md" | "markdown" => Self::Markdown,
            "bash" | "sh" | "zsh" => Self::Bash,
            "c" | "h" => Self::C,
            "cc" | "cpp" | "cxx" | "hpp" | "hxx" => Self::Cpp,
            "cs" => Self::CSharp,
            "css" => Self::Css,
            "go" => Self::Go,
            "htm" | "html" | "xml" | "svg" => Self::Html,
            "java" => Self::Java,
            "php" => Self::Php,
            "py" | "pyw" => Self::Python,
            "rb" => Self::Ruby,
            "yaml" | "yml" => Self::Yaml,
            "toml" => Self::Toml,
            "mmd" | "mermaid" => Self::Mermaid,
            _ => Self::PlainText,
        }
    }

    pub(crate) const fn canonical_name(self) -> &'static str {
        match self {
            Self::Rust => "rust",
            Self::JavaScript => "javascript",
            Self::JavaScriptJsx => "jsx",
            Self::TypeScript => "typescript",
            Self::TypeScriptTsx => "tsx",
            Self::Json | Self::JsonLines => "json",
            Self::Markdown => "markdown",
            Self::Bash => "bash",
            Self::C => "c",
            Self::Cpp => "cpp",
            Self::CSharp => "csharp",
            Self::Css => "css",
            Self::Go => "go",
            Self::Html => "html",
            Self::Java => "java",
            Self::Php => "php",
            Self::Python => "python",
            Self::Ruby => "ruby",
            Self::Yaml => "yaml",
            Self::Toml => "toml",
            Self::Mermaid => "mermaid",
            Self::PlainText => "text",
        }
    }

    pub(crate) const fn supports_folding(self) -> bool {
        !matches!(self, Self::PlainText | Self::JsonLines)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_detection_covers_structured_file_aliases() {
        assert_eq!(
            SourceLanguageId::for_path(Path::new("a.jsonl")),
            SourceLanguageId::JsonLines
        );
        assert_eq!(
            SourceLanguageId::for_path(Path::new("Cargo.lock")),
            SourceLanguageId::Toml
        );
        assert_eq!(
            SourceLanguageId::for_path(Path::new("flow.mmd")),
            SourceLanguageId::Mermaid
        );
        assert_eq!(
            SourceLanguageId::for_path(Path::new("notes.txt")),
            SourceLanguageId::PlainText
        );
    }
}
