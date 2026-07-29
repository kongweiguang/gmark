// @author kongweiguang

//! UI-neutral theme tokens consumed by the export CSS generator.

/// An sRGB export colour with alpha. It intentionally has no window-system or
/// rendering-framework dependency.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ExportColor {
    /// Red channel from zero through 255.
    pub red: u8,
    /// Green channel from zero through 255.
    pub green: u8,
    /// Blue channel from zero through 255.
    pub blue: u8,
    /// Opacity from zero through one.
    pub alpha: f32,
}

impl ExportColor {
    /// Creates an opaque sRGB colour.
    pub const fn opaque(red: u8, green: u8, blue: u8) -> Self {
        Self {
            red,
            green,
            blue,
            alpha: 1.0,
        }
    }

    pub(crate) fn css(self) -> String {
        format!(
            "rgba({},{},{},{:.3})",
            self.red,
            self.green,
            self.blue,
            self.alpha.clamp(0.0, 1.0)
        )
    }
}

/// Browser colour-scheme hint kept separate from the input colour space.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ExportColorScheme {
    /// Prefer browser light controls and default rendering.
    Light,
    /// Prefer browser dark controls and default rendering.
    #[default]
    Dark,
}

impl ExportColorScheme {
    pub(crate) const fn css_name(self) -> &'static str {
        match self {
            Self::Light => "light",
            Self::Dark => "dark",
        }
    }
}

/// CSS-compatible font weights without a dependency on a UI text stack.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ExportFontWeight {
    Thin,
    Light,
    #[default]
    Normal,
    Medium,
    Semibold,
    Bold,
    Extrabold,
    Black,
}

impl ExportFontWeight {
    pub(crate) const fn css_value(self) -> u16 {
        match self {
            Self::Thin => 100,
            Self::Light => 300,
            Self::Normal => 400,
            Self::Medium => 500,
            Self::Semibold => 600,
            Self::Bold => 700,
            Self::Extrabold => 800,
            Self::Black => 900,
        }
    }
}

/// The semantic colours referenced by the HTML export stylesheet.
#[derive(Clone, Debug, PartialEq)]
pub struct ExportThemeColors {
    pub background: ExportColor,
    pub text: ExportColor,
    pub muted: ExportColor,
    pub link: ExportColor,
    pub border: ExportColor,
    pub code_background: ExportColor,
    pub code_text: ExportColor,
    pub comment_background: ExportColor,
    pub table_header_background: ExportColor,
    pub table_cell_background: ExportColor,
    pub quote_border: ExportColor,
    pub quote_text: ExportColor,
    pub callout_note_background: ExportColor,
    pub callout_note_border: ExportColor,
    pub callout_tip_background: ExportColor,
    pub callout_tip_border: ExportColor,
    pub callout_important_background: ExportColor,
    pub callout_important_border: ExportColor,
    pub callout_warning_background: ExportColor,
    pub callout_warning_border: ExportColor,
    pub callout_caution_background: ExportColor,
    pub callout_caution_border: ExportColor,
    pub heading: [ExportColor; 6],
}

impl Default for ExportThemeColors {
    fn default() -> Self {
        let background = ExportColor::opaque(41, 42, 48);
        let text = ExportColor::opaque(255, 255, 255);
        Self {
            background,
            text,
            muted: ExportColor {
                red: 143,
                green: 143,
                blue: 152,
                alpha: 0.8,
            },
            link: ExportColor::opaque(10, 132, 255),
            border: ExportColor::opaque(72, 73, 80),
            code_background: ExportColor::opaque(33, 34, 46),
            code_text: text,
            comment_background: ExportColor {
                red: 127,
                green: 140,
                blue: 152,
                alpha: 0.19,
            },
            table_header_background: ExportColor::opaque(33, 34, 46),
            table_cell_background: background,
            quote_border: ExportColor::opaque(125, 126, 135),
            quote_text: ExportColor::opaque(200, 200, 208),
            callout_note_background: ExportColor {
                red: 148,
                green: 163,
                blue: 184,
                alpha: 0.12,
            },
            callout_note_border: ExportColor::opaque(148, 163, 180),
            callout_tip_background: ExportColor {
                red: 29,
                green: 78,
                blue: 216,
                alpha: 0.12,
            },
            callout_tip_border: ExportColor::opaque(96, 165, 250),
            callout_important_background: ExportColor {
                red: 167,
                green: 139,
                blue: 250,
                alpha: 0.12,
            },
            callout_important_border: ExportColor::opaque(167, 139, 250),
            callout_warning_background: ExportColor {
                red: 251,
                green: 113,
                blue: 133,
                alpha: 0.12,
            },
            callout_warning_border: ExportColor::opaque(251, 113, 133),
            callout_caution_background: ExportColor {
                red: 220,
                green: 38,
                blue: 38,
                alpha: 0.12,
            },
            callout_caution_border: ExportColor::opaque(248, 113, 113),
            heading: [text; 6],
        }
    }
}

/// Typography values needed by browser rendering.
#[derive(Clone, Debug, PartialEq)]
pub struct ExportThemeTypography {
    pub text_size: f32,
    pub text_line_height: f32,
    pub heading_sizes: [f32; 6],
    pub heading_weight: ExportFontWeight,
    pub code_size: f32,
}

impl Default for ExportThemeTypography {
    fn default() -> Self {
        Self {
            text_size: 16.0,
            text_line_height: 1.6,
            heading_sizes: [32.0, 26.0, 22.0, 20.0, 18.0, 16.0],
            heading_weight: ExportFontWeight::Bold,
            code_size: 14.0,
        }
    }
}

/// The small subset of spatial tokens referenced by exported CSS.
#[derive(Clone, Debug, PartialEq)]
pub struct ExportThemeDimensions {
    pub callout_radius: f32,
    pub code_background_radius: f32,
}

impl Default for ExportThemeDimensions {
    fn default() -> Self {
        Self {
            callout_radius: 6.0,
            code_background_radius: 6.0,
        }
    }
}

/// Complete neutral theme input for HTML, PDF, and PNG export.
#[derive(Clone, Debug, PartialEq)]
pub struct ExportTheme {
    pub color_scheme: ExportColorScheme,
    pub colors: ExportThemeColors,
    pub typography: ExportThemeTypography,
    pub dimensions: ExportThemeDimensions,
}

impl Default for ExportTheme {
    fn default() -> Self {
        Self {
            color_scheme: ExportColorScheme::Dark,
            colors: ExportThemeColors::default(),
            typography: ExportThemeTypography::default(),
            dimensions: ExportThemeDimensions::default(),
        }
    }
}

pub(crate) fn theme_css(theme: &ExportTheme) -> String {
    let colors = &theme.colors;
    let typography = &theme.typography;
    let dimensions = &theme.dimensions;
    let mut css = format!(
        r#":root {{
  color-scheme: {};
  --vlt-bg: {};
  --vlt-text: {};
  --vlt-muted: {};
  --vlt-link: {};
  --vlt-border: {};
  --vlt-code-bg: {};
  --vlt-code-text: {};
  --vlt-comment-bg: {};
  --vlt-table-head-bg: {};
  --vlt-table-cell-bg: {};
  --vlt-quote-border: {};
  --vlt-quote-text: {};
  --vlt-callout-note-bg: {};
  --vlt-callout-note-border: {};
  --vlt-callout-tip-bg: {};
  --vlt-callout-tip-border: {};
  --vlt-callout-important-bg: {};
  --vlt-callout-important-border: {};
  --vlt-callout-warning-bg: {};
  --vlt-callout-warning-border: {};
  --vlt-callout-caution-bg: {};
  --vlt-callout-caution-border: {};
}}

* {{ box-sizing: border-box; }}
html {{ background-color: var(--vlt-bg); color: var(--vlt-text); }}
body {{
  margin: 0;
  background-color: var(--vlt-bg);
  color: var(--vlt-text);
  font-family: system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", "Noto Serif Tibetan", "Noto Sans Tibetan", "Microsoft Himalaya", Kailasa, "BabelStone Tibetan", sans-serif;
  font-size: {}px;
  line-height: {};
}}
{}
p, ul, ol, blockquote, pre, table, hr {{ margin: 0 0 1rem; }}
h1, h2, h3, h4, h5, h6 {{ margin: 1.6em 0 0.65em; line-height: 1.2; font-weight: {}; }}
h1 {{ color: {}; font-size: {}px; }}
h2 {{ color: {}; font-size: {}px; }}
h3 {{ color: {}; font-size: {}px; }}
h4 {{ color: {}; font-size: {}px; }}
h5 {{ color: {}; font-size: {}px; }}
h6 {{ color: {}; font-size: {}px; }}
a {{ color: var(--vlt-link); text-decoration-thickness: 0.08em; text-underline-offset: 0.18em; }}
blockquote {{ margin-left: 0; padding: 0.5rem 0 0.5rem 1rem; border-left: 3px solid var(--vlt-quote-border); color: var(--vlt-quote-text); }}
blockquote.markdown-alert-note, blockquote.markdown-alert-tip, blockquote.markdown-alert-important, blockquote.markdown-alert-warning, blockquote.markdown-alert-caution {{ padding: 0.75rem 1rem; border-left: 4px solid; border-radius: {}px; }}
blockquote.markdown-alert-note {{ background-color: var(--vlt-callout-note-bg); border-color: var(--vlt-callout-note-border); }}
blockquote.markdown-alert-tip {{ background-color: var(--vlt-callout-tip-bg); border-color: var(--vlt-callout-tip-border); }}
blockquote.markdown-alert-important {{ background-color: var(--vlt-callout-important-bg); border-color: var(--vlt-callout-important-border); }}
blockquote.markdown-alert-warning {{ background-color: var(--vlt-callout-warning-bg); border-color: var(--vlt-callout-warning-border); }}
blockquote.markdown-alert-caution {{ background-color: var(--vlt-callout-caution-bg); border-color: var(--vlt-callout-caution-border); }}
code {{ background-color: var(--vlt-code-bg); color: var(--vlt-code-text); border-radius: 4px; padding: 0.12em 0.32em; font-family: "SFMono-Regular", Consolas, "Liberation Mono", Menlo, monospace; font-size: {}px; }}
pre {{ overflow: auto; background-color: var(--vlt-code-bg); color: var(--vlt-code-text); border-radius: {}px; padding: 1rem; }}
pre code {{ padding: 0; background-color: transparent; }}
.vlt-comment {{ white-space: pre-wrap; padding: 0; border: 0; background-color: transparent; color: var(--vlt-link); }}
.vlt-raw-html {{ white-space: pre-wrap; background-color: var(--vlt-code-bg); color: var(--vlt-code-text); }}
.vlt-math, .vlt-mermaid {{ display: flex; justify-content: center; margin: 1rem 0; overflow-x: auto; }}
.vlt-math svg, .vlt-mermaid img {{ max-width: 100%; height: auto; }}
.vlt-mermaid img {{ display: block; margin: 0 auto; }}
.vlt-inline-math {{ display: inline-flex; align-items: center; vertical-align: middle; max-width: 100%; }}
.vlt-inline-math svg {{ max-height: 1.8em; width: auto; }}
.vlt-math-error, .vlt-mermaid-error {{ white-space: pre-wrap; background-color: var(--vlt-code-bg); color: var(--vlt-code-text); }}
table {{ width: 100%; border-collapse: collapse; display: table; }}
th, td {{
  border: 1px solid;
  border-color: var(--vlt-border);
  padding: 0.5rem 0.65rem;
  vertical-align: top;
}}
th {{ background-color: var(--vlt-table-head-bg); font-weight: 600; }}
td {{ background-color: var(--vlt-table-cell-bg); }}
img {{ max-width: 100%; height: auto; display: block; margin: 1rem auto; }}
hr {{ border: 0; border-top: 1px solid var(--vlt-border); }}
.gmark-toc {{ margin: 1rem 0 1.5rem; padding: 0.85rem 1rem; border-left: 2px solid var(--vlt-link); background-color: var(--vlt-code-bg); }}
.gmark-toc ol {{ margin: 0; padding-left: 1.2rem; }}
.gmark-toc li {{ margin: 0.22rem 0; }}
.gmark-toc-level-2 {{ margin-left: 1rem !important; }}
.gmark-toc-level-3 {{ margin-left: 2rem !important; }}
.gmark-toc-level-4 {{ margin-left: 3rem !important; }}
.gmark-toc-level-5 {{ margin-left: 4rem !important; }}
.gmark-toc-level-6 {{ margin-left: 5rem !important; }}
.footnote-definition {{ color: var(--vlt-muted); font-size: 0.92em; }}
"#,
        theme.color_scheme.css_name(),
        colors.background.css(),
        colors.text.css(),
        colors.muted.css(),
        colors.link.css(),
        colors.border.css(),
        colors.code_background.css(),
        colors.code_text.css(),
        colors.comment_background.css(),
        colors.table_header_background.css(),
        colors.table_cell_background.css(),
        colors.quote_border.css(),
        colors.quote_text.css(),
        colors.callout_note_background.css(),
        colors.callout_note_border.css(),
        colors.callout_tip_background.css(),
        colors.callout_tip_border.css(),
        colors.callout_important_background.css(),
        colors.callout_important_border.css(),
        colors.callout_warning_background.css(),
        colors.callout_warning_border.css(),
        colors.callout_caution_background.css(),
        colors.callout_caution_border.css(),
        typography.text_size,
        typography.text_line_height,
        document_layout_css(),
        typography.heading_weight.css_value(),
        colors.heading[0].css(),
        typography.heading_sizes[0],
        colors.heading[1].css(),
        typography.heading_sizes[1],
        colors.heading[2].css(),
        typography.heading_sizes[2],
        colors.heading[3].css(),
        typography.heading_sizes[3],
        colors.heading[4].css(),
        typography.heading_sizes[4],
        colors.heading[5].css(),
        typography.heading_sizes[5],
        dimensions.callout_radius,
        typography.code_size,
        dimensions.code_background_radius,
    );
    css.push_str(
        r#"
.gmark-resource-card { display: flex; align-items: center; gap: 0.65rem; margin: 0 0 1rem; padding: 0.65rem 0.8rem; border: 1px solid var(--vlt-border); border-radius: 0.55rem; background-color: var(--vlt-code-bg); color: var(--vlt-text); text-decoration: none; }
.gmark-resource-card:hover { border-color: var(--vlt-link); }
.gmark-resource-kind { flex: 0 0 auto; color: var(--vlt-link); font-size: 0.72em; font-weight: 700; }
.gmark-resource-main { min-width: 0; flex: 1 1 auto; display: flex; flex-direction: column; gap: 0.12rem; }
.gmark-resource-main strong, .gmark-resource-main small { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.gmark-resource-main small, .gmark-resource-status { color: var(--vlt-muted); }
.gmark-resource-status { flex: 0 0 auto; font-size: 0.86em; }
"#,
    );
    css
}

pub(crate) fn chromium_pdf_theme_css(theme: &ExportTheme) -> String {
    let mut css = theme_css(theme);
    css = css.replace(
        document_layout_css(),
        ".vlt-document {\n  width: auto;\n  max-width: none;\n  margin: 0;\n  padding: 0;\n}",
    );
    css.push_str(
        r#"
@page { size: A4; margin: 15mm; }
@media print {
  html, body { background-color: var(--vlt-bg); border: 0; outline: 0; box-shadow: none; print-color-adjust: exact; -webkit-print-color-adjust: exact; }
  .vlt-document { width: auto; max-width: none; margin: 0; padding: 0; border: 0; outline: 0; box-shadow: none; }
  pre, code { white-space: pre-wrap; overflow-wrap: anywhere; }
  img, svg { max-width: 100%; height: auto; break-inside: avoid; }
  table, blockquote, pre, .vlt-math, .vlt-mermaid { break-inside: avoid; }
}
"#,
    );
    css
}

fn document_layout_css() -> &'static str {
    ".vlt-document {\n  width: min(100% - 48px, 920px);\n  margin: 0 auto;\n  padding: 48px 0 72px;\n}"
}
