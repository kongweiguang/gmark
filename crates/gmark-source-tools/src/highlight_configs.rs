// @author kongweiguang

#[cfg(any(
    feature = "code-highlight-official",
    feature = "code-highlight-config",
    feature = "code-highlight-extra"
))]
use crate::highlight::configure_highlights;
#[cfg(any(
    feature = "code-highlight-official",
    feature = "code-highlight-config",
    feature = "code-highlight-extra"
))]
use tree_sitter_highlight::HighlightConfiguration;

#[cfg(feature = "code-highlight-official")]
pub(crate) fn build_rust_config() -> Option<HighlightConfiguration> {
    configure_highlights(
        tree_sitter_rust::LANGUAGE.into(),
        "rust",
        tree_sitter_rust::HIGHLIGHTS_QUERY,
        tree_sitter_rust::INJECTIONS_QUERY,
        "",
    )
}

#[cfg(feature = "code-highlight-official")]
pub(crate) fn build_javascript_config() -> Option<HighlightConfiguration> {
    configure_highlights(
        tree_sitter_javascript::LANGUAGE.into(),
        "javascript",
        tree_sitter_javascript::HIGHLIGHT_QUERY,
        tree_sitter_javascript::INJECTIONS_QUERY,
        tree_sitter_javascript::LOCALS_QUERY,
    )
}

#[cfg(feature = "code-highlight-official")]
pub(crate) fn build_jsx_config() -> Option<HighlightConfiguration> {
    let query = format!(
        "{}\n{}",
        tree_sitter_javascript::HIGHLIGHT_QUERY,
        tree_sitter_javascript::JSX_HIGHLIGHT_QUERY
    );
    configure_highlights(
        tree_sitter_javascript::LANGUAGE.into(),
        "javascript",
        &query,
        tree_sitter_javascript::INJECTIONS_QUERY,
        tree_sitter_javascript::LOCALS_QUERY,
    )
}

#[cfg(feature = "code-highlight-official")]
pub(crate) fn build_typescript_config() -> Option<HighlightConfiguration> {
    configure_highlights(
        tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
        "typescript",
        tree_sitter_typescript::HIGHLIGHTS_QUERY,
        "",
        tree_sitter_typescript::LOCALS_QUERY,
    )
}

#[cfg(feature = "code-highlight-official")]
pub(crate) fn build_tsx_config() -> Option<HighlightConfiguration> {
    configure_highlights(
        tree_sitter_typescript::LANGUAGE_TSX.into(),
        "tsx",
        tree_sitter_typescript::HIGHLIGHTS_QUERY,
        "",
        tree_sitter_typescript::LOCALS_QUERY,
    )
}

#[cfg(feature = "code-highlight-official")]
pub(crate) fn build_json_config() -> Option<HighlightConfiguration> {
    configure_highlights(
        tree_sitter_json::LANGUAGE.into(),
        "json",
        tree_sitter_json::HIGHLIGHTS_QUERY,
        "",
        "",
    )
}

#[cfg(feature = "code-highlight-official")]
pub(crate) fn build_markdown_config() -> Option<HighlightConfiguration> {
    configure_highlights(
        tree_sitter_md::LANGUAGE.into(),
        "markdown",
        tree_sitter_md::HIGHLIGHT_QUERY_BLOCK,
        tree_sitter_md::INJECTION_QUERY_BLOCK,
        "",
    )
}

#[cfg(feature = "code-highlight-official")]
pub(crate) fn build_bash_config() -> Option<HighlightConfiguration> {
    configure_highlights(
        tree_sitter_bash::LANGUAGE.into(),
        "bash",
        tree_sitter_bash::HIGHLIGHT_QUERY,
        "",
        "",
    )
}

#[cfg(feature = "code-highlight-official")]
pub(crate) fn build_c_config() -> Option<HighlightConfiguration> {
    configure_highlights(
        tree_sitter_c::LANGUAGE.into(),
        "c",
        tree_sitter_c::HIGHLIGHT_QUERY,
        "",
        "",
    )
}

#[cfg(feature = "code-highlight-official")]
pub(crate) fn build_cpp_config() -> Option<HighlightConfiguration> {
    configure_highlights(
        tree_sitter_cpp::LANGUAGE.into(),
        "cpp",
        tree_sitter_cpp::HIGHLIGHT_QUERY,
        "",
        "",
    )
}

#[cfg(feature = "code-highlight-official")]
pub(crate) fn build_csharp_config() -> Option<HighlightConfiguration> {
    configure_highlights(
        tree_sitter_c_sharp::LANGUAGE.into(),
        "c_sharp",
        tree_sitter_c_sharp::HIGHLIGHTS_QUERY,
        "",
        "",
    )
}

#[cfg(feature = "code-highlight-official")]
pub(crate) fn build_css_config() -> Option<HighlightConfiguration> {
    configure_highlights(
        tree_sitter_css::LANGUAGE.into(),
        "css",
        tree_sitter_css::HIGHLIGHTS_QUERY,
        "",
        "",
    )
}

#[cfg(feature = "code-highlight-official")]
pub(crate) fn build_go_config() -> Option<HighlightConfiguration> {
    configure_highlights(
        tree_sitter_go::LANGUAGE.into(),
        "go",
        tree_sitter_go::HIGHLIGHTS_QUERY,
        "",
        "",
    )
}

#[cfg(feature = "code-highlight-official")]
pub(crate) fn build_html_config() -> Option<HighlightConfiguration> {
    configure_highlights(
        tree_sitter_html::LANGUAGE.into(),
        "html",
        tree_sitter_html::HIGHLIGHTS_QUERY,
        tree_sitter_html::INJECTIONS_QUERY,
        "",
    )
}

#[cfg(feature = "code-highlight-official")]
pub(crate) fn build_java_config() -> Option<HighlightConfiguration> {
    configure_highlights(
        tree_sitter_java::LANGUAGE.into(),
        "java",
        tree_sitter_java::HIGHLIGHTS_QUERY,
        "",
        "",
    )
}

#[cfg(feature = "code-highlight-official")]
pub(crate) fn build_php_config() -> Option<HighlightConfiguration> {
    configure_highlights(
        tree_sitter_php::LANGUAGE_PHP.into(),
        "php",
        tree_sitter_php::HIGHLIGHTS_QUERY,
        tree_sitter_php::INJECTIONS_QUERY,
        "",
    )
}

#[cfg(feature = "code-highlight-official")]
pub(crate) fn build_python_config() -> Option<HighlightConfiguration> {
    configure_highlights(
        tree_sitter_python::LANGUAGE.into(),
        "python",
        tree_sitter_python::HIGHLIGHTS_QUERY,
        "",
        "",
    )
}

#[cfg(feature = "code-highlight-official")]
pub(crate) fn build_ruby_config() -> Option<HighlightConfiguration> {
    configure_highlights(
        tree_sitter_ruby::LANGUAGE.into(),
        "ruby",
        tree_sitter_ruby::HIGHLIGHTS_QUERY,
        "",
        tree_sitter_ruby::LOCALS_QUERY,
    )
}

#[cfg(feature = "code-highlight-config")]
pub(crate) fn build_yaml_config() -> Option<HighlightConfiguration> {
    configure_highlights(
        tree_sitter_yaml::LANGUAGE.into(),
        "yaml",
        tree_sitter_yaml::HIGHLIGHTS_QUERY,
        "",
        "",
    )
}

#[cfg(feature = "code-highlight-config")]
pub(crate) fn build_toml_config() -> Option<HighlightConfiguration> {
    configure_highlights(
        tree_sitter_toml::LANGUAGE.into(),
        "toml",
        tree_sitter_toml::HIGHLIGHTS_QUERY,
        "",
        "",
    )
}

#[cfg(feature = "code-highlight-extra")]
pub(crate) fn build_sql_config() -> Option<HighlightConfiguration> {
    configure_highlights(
        tree_sitter_sequel::LANGUAGE.into(),
        "sql",
        tree_sitter_sequel::HIGHLIGHTS_QUERY,
        "",
        "",
    )
}

#[cfg(feature = "code-highlight-extra")]
pub(crate) fn build_lua_config() -> Option<HighlightConfiguration> {
    configure_highlights(
        tree_sitter_lua::LANGUAGE.into(),
        "lua",
        tree_sitter_lua::HIGHLIGHTS_QUERY,
        tree_sitter_lua::INJECTIONS_QUERY,
        tree_sitter_lua::LOCALS_QUERY,
    )
}

#[cfg(feature = "code-highlight-extra")]
pub(crate) fn build_swift_config() -> Option<HighlightConfiguration> {
    configure_highlights(
        tree_sitter_swift::LANGUAGE.into(),
        "swift",
        tree_sitter_swift::HIGHLIGHTS_QUERY,
        tree_sitter_swift::INJECTIONS_QUERY,
        tree_sitter_swift::LOCALS_QUERY,
    )
}

#[cfg(feature = "code-highlight-extra")]
pub(crate) fn build_powershell_config() -> Option<HighlightConfiguration> {
    configure_highlights(
        tree_sitter_powershell::LANGUAGE.into(),
        "powershell",
        tree_sitter_powershell::HIGHLIGHTS_QUERY,
        "",
        "",
    )
}

#[cfg(feature = "code-highlight-extra")]
pub(crate) fn build_containerfile_config() -> Option<HighlightConfiguration> {
    configure_highlights(
        tree_sitter_containerfile::LANGUAGE.into(),
        "dockerfile",
        tree_sitter_containerfile::HIGHLIGHTS_QUERY,
        tree_sitter_containerfile::INJECTIONS_QUERY,
        "",
    )
}
