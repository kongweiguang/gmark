// @author kongweiguang

use std::path::Path;

/// 源码识别、折叠、格式化和高亮共享的规范语言标识。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum SourceLanguage {
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
    /// 无法确定语言时的安全回退，不产生语义 token。
    #[default]
    PlainText,
}

impl SourceLanguage {
    /// 从文件名和扩展名识别语言，沿用旧 Source 视图的扩展名兼容表。
    pub fn for_path(path: &Path) -> Self {
        let file_name = path.file_name().and_then(|name| name.to_str());
        if file_name.is_some_and(|name| name.eq_ignore_ascii_case("Cargo.lock")) {
            return Self::Toml;
        }

        let extension = path.extension().and_then(|value| value.to_str());
        extension.map_or(Self::PlainText, Self::from_extension)
    }

    /// 从常见文件扩展名识别语言。传入值可带一个可选的 `.` 前缀。
    pub fn from_extension(extension: &str) -> Self {
        let normalized = extension.strip_prefix('.').unwrap_or(extension);
        match normalized.to_ascii_lowercase().as_str() {
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

    /// 解析围栏代码块的语言别名；大小写不敏感且不猜测未知别名。
    pub fn from_alias(alias: &str) -> Option<Self> {
        let alias = alias.trim();
        (!alias.is_empty()).then(|| {
            LANGUAGE_DESCRIPTORS
                .iter()
                .find(|descriptor| {
                    descriptor
                        .aliases
                        .iter()
                        .any(|candidate| candidate.eq_ignore_ascii_case(alias))
                })
                .map(|descriptor| descriptor.language)
        })?
    }

    /// 解析 fenced-code info string 的第一个空白分隔字段。
    pub fn from_fence_info(info: Option<&str>) -> Option<Self> {
        Self::from_alias(info?.split_whitespace().next()?)
    }

    /// 此语言在配置和缓存中使用的稳定规范名。
    pub const fn canonical_name(self) -> &'static str {
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

    /// 该语言对应的围栏别名；JSON Lines 仅按文件扩展名识别，保持旧行为。
    pub fn aliases(self) -> &'static [&'static str] {
        LANGUAGE_DESCRIPTORS
            .iter()
            .find(|descriptor| descriptor.language == self)
            .map_or(&[], |descriptor| descriptor.aliases)
    }

    /// Plain text 与 JSON Lines 没有可跨行恢复的结构性折叠。
    pub const fn supports_folding(self) -> bool {
        !matches!(self, Self::PlainText | Self::JsonLines)
    }
}

/// 从路径识别语言的便捷入口。
pub fn detect_language(path: &Path) -> SourceLanguage {
    SourceLanguage::for_path(path)
}

/// 解析围栏语言并保留 `None`，让调用方明确选择 plain-text fallback 时机。
pub fn resolve_fence_language(info: Option<&str>) -> Option<SourceLanguage> {
    SourceLanguage::from_fence_info(info)
}

#[derive(Clone, Copy)]
struct LanguageDescriptor {
    language: SourceLanguage,
    aliases: &'static [&'static str],
}

// 这是旧 Markdown 代码围栏接受的完整别名表；路径扩展名表单独维护在上方。
const LANGUAGE_DESCRIPTORS: &[LanguageDescriptor] = &[
    LanguageDescriptor {
        language: SourceLanguage::Rust,
        aliases: &["rust", "rs"],
    },
    LanguageDescriptor {
        language: SourceLanguage::JavaScript,
        aliases: &["javascript", "js"],
    },
    LanguageDescriptor {
        language: SourceLanguage::JavaScriptJsx,
        aliases: &["jsx"],
    },
    LanguageDescriptor {
        language: SourceLanguage::TypeScript,
        aliases: &["typescript", "ts"],
    },
    LanguageDescriptor {
        language: SourceLanguage::TypeScriptTsx,
        aliases: &["tsx"],
    },
    LanguageDescriptor {
        language: SourceLanguage::Json,
        aliases: &["json"],
    },
    LanguageDescriptor {
        language: SourceLanguage::Markdown,
        aliases: &["markdown", "md"],
    },
    LanguageDescriptor {
        language: SourceLanguage::Bash,
        aliases: &["bash", "sh", "shell", "zsh"],
    },
    LanguageDescriptor {
        language: SourceLanguage::C,
        aliases: &["c", "h"],
    },
    LanguageDescriptor {
        language: SourceLanguage::Cpp,
        aliases: &["cpp", "cxx", "cc", "hpp", "hxx"],
    },
    LanguageDescriptor {
        language: SourceLanguage::CSharp,
        aliases: &["csharp", "cs", "c#"],
    },
    LanguageDescriptor {
        language: SourceLanguage::Css,
        aliases: &["css"],
    },
    LanguageDescriptor {
        language: SourceLanguage::Go,
        aliases: &["go", "golang"],
    },
    LanguageDescriptor {
        language: SourceLanguage::Html,
        aliases: &["html"],
    },
    LanguageDescriptor {
        language: SourceLanguage::Java,
        aliases: &["java"],
    },
    LanguageDescriptor {
        language: SourceLanguage::Php,
        aliases: &["php"],
    },
    LanguageDescriptor {
        language: SourceLanguage::Python,
        aliases: &["python", "py"],
    },
    LanguageDescriptor {
        language: SourceLanguage::Ruby,
        aliases: &["ruby", "rb"],
    },
    LanguageDescriptor {
        language: SourceLanguage::Yaml,
        aliases: &["yaml", "yml"],
    },
    LanguageDescriptor {
        language: SourceLanguage::Toml,
        aliases: &["toml"],
    },
    LanguageDescriptor {
        language: SourceLanguage::PlainText,
        aliases: &["text", "txt", "plain"],
    },
    LanguageDescriptor {
        language: SourceLanguage::Mermaid,
        aliases: &["mermaid"],
    },
];

#[cfg(feature = "code-highlight-core")]
pub(crate) fn tree_sitter_language(language: SourceLanguage) -> Option<tree_sitter::Language> {
    match language {
        #[cfg(feature = "code-highlight-official")]
        SourceLanguage::Rust => Some(tree_sitter_rust::LANGUAGE.into()),
        #[cfg(feature = "code-highlight-official")]
        SourceLanguage::JavaScript | SourceLanguage::JavaScriptJsx => {
            Some(tree_sitter_javascript::LANGUAGE.into())
        }
        #[cfg(feature = "code-highlight-official")]
        SourceLanguage::TypeScript => Some(tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into()),
        #[cfg(feature = "code-highlight-official")]
        SourceLanguage::TypeScriptTsx => Some(tree_sitter_typescript::LANGUAGE_TSX.into()),
        #[cfg(feature = "code-highlight-official")]
        SourceLanguage::Json | SourceLanguage::JsonLines => Some(tree_sitter_json::LANGUAGE.into()),
        #[cfg(feature = "code-highlight-official")]
        SourceLanguage::Markdown => Some(tree_sitter_md::LANGUAGE.into()),
        #[cfg(feature = "code-highlight-official")]
        SourceLanguage::Bash => Some(tree_sitter_bash::LANGUAGE.into()),
        #[cfg(feature = "code-highlight-official")]
        SourceLanguage::C => Some(tree_sitter_c::LANGUAGE.into()),
        #[cfg(feature = "code-highlight-official")]
        SourceLanguage::Cpp => Some(tree_sitter_cpp::LANGUAGE.into()),
        #[cfg(feature = "code-highlight-official")]
        SourceLanguage::CSharp => Some(tree_sitter_c_sharp::LANGUAGE.into()),
        #[cfg(feature = "code-highlight-official")]
        SourceLanguage::Css => Some(tree_sitter_css::LANGUAGE.into()),
        #[cfg(feature = "code-highlight-official")]
        SourceLanguage::Go => Some(tree_sitter_go::LANGUAGE.into()),
        #[cfg(feature = "code-highlight-official")]
        SourceLanguage::Html => Some(tree_sitter_html::LANGUAGE.into()),
        #[cfg(feature = "code-highlight-official")]
        SourceLanguage::Java => Some(tree_sitter_java::LANGUAGE.into()),
        #[cfg(feature = "code-highlight-official")]
        SourceLanguage::Php => Some(tree_sitter_php::LANGUAGE_PHP.into()),
        #[cfg(feature = "code-highlight-official")]
        SourceLanguage::Python => Some(tree_sitter_python::LANGUAGE.into()),
        #[cfg(feature = "code-highlight-official")]
        SourceLanguage::Ruby => Some(tree_sitter_ruby::LANGUAGE.into()),
        #[cfg(feature = "code-highlight-config")]
        SourceLanguage::Yaml => Some(tree_sitter_yaml::LANGUAGE.into()),
        #[cfg(feature = "code-highlight-config")]
        SourceLanguage::Toml => Some(tree_sitter_toml::LANGUAGE.into()),
        _ => None,
    }
}
