// @author kongweiguang

use std::path::Path;

use super::{
    CodeHighlightClass, CodeLanguageKey, build_source_syntax_contexts, code_language_for_path,
    highlight_code_block, resolve_code_language_key,
};

#[test]
fn balanced_bundle_aliases_resolve_to_expected_keys() {
    assert_eq!(
        resolve_code_language_key(Some("rust")),
        Some(CodeLanguageKey::Rust)
    );
    assert_eq!(
        resolve_code_language_key(Some("js")),
        Some(CodeLanguageKey::JavaScript)
    );
    assert_eq!(
        resolve_code_language_key(Some("jsx")),
        Some(CodeLanguageKey::JavaScriptJsx)
    );
    assert_eq!(
        resolve_code_language_key(Some("ts")),
        Some(CodeLanguageKey::TypeScript)
    );
    assert_eq!(
        resolve_code_language_key(Some("tsx")),
        Some(CodeLanguageKey::TypeScriptTsx)
    );
    assert_eq!(
        resolve_code_language_key(Some("sh")),
        Some(CodeLanguageKey::Bash)
    );
    assert_eq!(
        resolve_code_language_key(Some("hpp")),
        Some(CodeLanguageKey::Cpp)
    );
    assert_eq!(
        resolve_code_language_key(Some("c#")),
        Some(CodeLanguageKey::CSharp)
    );
    assert_eq!(
        resolve_code_language_key(Some("golang")),
        Some(CodeLanguageKey::Go)
    );
    assert_eq!(
        resolve_code_language_key(Some("py")),
        Some(CodeLanguageKey::Python)
    );
    assert_eq!(
        resolve_code_language_key(Some("rb")),
        Some(CodeLanguageKey::Ruby)
    );
    assert_eq!(
        resolve_code_language_key(Some("yml")),
        Some(CodeLanguageKey::Yaml)
    );
    assert_eq!(
        resolve_code_language_key(Some("plain")),
        Some(CodeLanguageKey::PlainText)
    );
    assert_eq!(
        resolve_code_language_key(Some("mermaid")),
        Some(CodeLanguageKey::Mermaid)
    );
    assert_eq!(
        resolve_code_language_key(Some("postgresql")),
        Some(CodeLanguageKey::Sql)
    );
    assert_eq!(
        resolve_code_language_key(Some("pwsh")),
        Some(CodeLanguageKey::PowerShell)
    );
    assert_eq!(
        resolve_code_language_key(Some("containerfile")),
        Some(CodeLanguageKey::Containerfile)
    );
    assert_eq!(
        resolve_code_language_key(Some("jsonc")),
        Some(CodeLanguageKey::Json)
    );
    assert_eq!(
        resolve_code_language_key(Some("xml")),
        Some(CodeLanguageKey::Html)
    );
    assert_eq!(resolve_code_language_key(Some("unknown")), None);
}

#[test]
fn standalone_source_paths_map_to_registered_languages() {
    let samples = [
        ("main.rs", "rust"),
        ("app.tsx", "tsx"),
        ("data.json", "json"),
        ("config.yml", "yaml"),
        ("vector.svg", "html"),
        ("Cargo.lock", "toml"),
        ("query.sql", "sql"),
        ("script.lua", "lua"),
        ("App.swift", "swift"),
        ("profile.ps1", "powershell"),
        ("Dockerfile", "dockerfile"),
        ("Containerfile", "dockerfile"),
    ];
    for (path, expected) in samples {
        assert_eq!(code_language_for_path(Path::new(path)), Some(expected));
    }
    assert_eq!(code_language_for_path(Path::new("photo.png")), None);
}

#[test]
fn plain_fallback_languages_produce_empty_spans() {
    let mermaid = highlight_code_block(Some("mermaid"), "graph TD;\nA-->B")
        .expect("known plain fallback should still produce a result");
    assert_eq!(mermaid.language, CodeLanguageKey::Mermaid);
    assert!(mermaid.spans.is_empty());

    let text = highlight_code_block(Some("text"), "just text")
        .expect("plain text should still produce a result");
    assert_eq!(text.language, CodeLanguageKey::PlainText);
    assert!(text.spans.is_empty());
}

#[test]
fn json_source_fragment_keeps_keys_values_and_literals_highlighted() {
    let source = r#"  "name": "gmark", "count": 2, "ready": true, "next": null,"#;
    let result = highlight_code_block(Some("json"), source).expect("json highlight result");

    assert_eq!(result.language, CodeLanguageKey::Json);
    for class in [
        CodeHighlightClass::Property,
        CodeHighlightClass::String,
        CodeHighlightClass::Number,
        CodeHighlightClass::Constant,
        CodeHighlightClass::Punctuation,
    ] {
        assert!(
            result.spans.iter().any(|span| span.class == class),
            "JSON source fragment should contain {class:?}"
        );
    }
}

#[cfg(all(feature = "code-highlight-core", feature = "code-highlight-official"))]
#[test]
fn default_official_highlight_bundle_produces_spans() {
    let samples = [
        ("rust", "fn main() {\n    let value: i32 = 42;\n}\n"),
        ("js", "function greet(name) { return `hi ${name}`; }\n"),
        ("jsx", "const App = () => <div className=\"x\">Hi</div>;\n"),
        (
            "ts",
            "type User = { id: number };\nconst user: User = { id: 1 };\n",
        ),
        (
            "tsx",
            "const App = (): JSX.Element => <button>OK</button>;\n",
        ),
        ("json", "{\n  \"answer\": 42\n}\n"),
        ("md", "# Heading\n\n`code`\n"),
        ("bash", "echo \"hello\"\nif [ -f file ]; then echo ok; fi\n"),
        ("c", "int main(void) { return 0; }\n"),
        ("cpp", "class Box { public: int value = 1; };\n"),
        (
            "csharp",
            "class App { static void Main() { var x = 1; } }\n",
        ),
        ("css", "body { color: #fff; display: grid; }\n"),
        ("go", "package main\nfunc main() { println(\"hi\") }\n"),
        ("html", "<div class=\"card\"><span>Hi</span></div>\n"),
        (
            "java",
            "class App { int add(int a, int b) { return a + b; } }\n",
        ),
        ("php", "<?php echo \"hi\"; $x = 1; ?>\n"),
        ("python", "def double(x):\n    return x * 2\n"),
        ("ruby", "def hello(name)\n  puts \"Hi #{name}\"\nend\n"),
    ];

    for (language, sample) in samples {
        let result = highlight_code_block(Some(language), sample)
            .expect("known language should produce a result");
        assert!(
            !result.spans.is_empty(),
            "expected non-empty spans for {language}"
        );
    }
}

#[cfg(all(feature = "code-highlight-core", feature = "code-highlight-config"))]
#[test]
fn config_language_bundle_produces_spans() {
    let yaml = highlight_code_block(Some("yaml"), "key:\n  - value\n")
        .expect("yaml should produce a result");
    assert!(!yaml.spans.is_empty());

    let toml = highlight_code_block(
        Some("toml"),
        "[package]\nname = \"gmark\"\nversion = \"0.1.0\"\n",
    )
    .expect("toml should produce a result");
    assert!(!toml.spans.is_empty());
}

#[cfg(all(feature = "code-highlight-core", feature = "code-highlight-extra"))]
#[test]
fn extra_language_bundle_produces_spans() {
    let samples = [
        (
            "sql",
            "SELECT vehicle_id, speed FROM snowplow_table WHERE speed > 0 ORDER BY speed;\n",
        ),
        ("lua", "local value = 42\nprint(value)\n"),
        (
            "swift",
            "let greeting: String = \"hello\"\nprint(greeting)\n",
        ),
        (
            "powershell",
            "$items = Get-ChildItem\nforeach ($item in $items) { Write-Output $item }\n",
        ),
        (
            "dockerfile",
            "FROM rust:latest\nRUN cargo build --release\n",
        ),
    ];

    for (language, sample) in samples {
        let result = highlight_code_block(Some(language), sample)
            .expect("known extra language should produce a result");
        assert!(
            !result.spans.is_empty(),
            "expected non-empty spans for {language}"
        );
    }
}

#[cfg(all(feature = "code-highlight-core", feature = "code-highlight-extra"))]
#[test]
fn source_rows_keep_multiline_sql_highlight_context() {
    let lines = [
        "SELECT vehicle_id, sequence, action, road_id, target_road_id,",
        "       start_time, end_time, duration, speed",
        "FROM snowplow_table",
        "WHERE vehicle_id = :vehicle_id",
        "  AND action = 'clean'",
        "ORDER BY start_time",
        "LIMIT 1;",
    ];
    let contexts = build_source_syntax_contexts(
        Some("sql"),
        lines.iter().enumerate().map(|(line, text)| (line, *text)),
    );

    assert_eq!(contexts.len(), lines.len());
    for (line, text) in lines.iter().enumerate() {
        let result = contexts[&line].highlight(text);
        assert_eq!(result.language, CodeLanguageKey::Sql);
        assert!(
            !result.spans.is_empty(),
            "row {line} should retain SQL context"
        );
    }
}
