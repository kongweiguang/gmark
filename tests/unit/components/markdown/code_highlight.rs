// @author kongweiguang

use std::path::Path;

use super::{
    CodeLanguageKey, code_language_for_path, highlight_code_block, resolve_code_language_key,
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
