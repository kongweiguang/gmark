// @author kongweiguang

//! Pinned CommonMark/GFM semantic and source-preservation corpus replay.

use gmark_document::SourceDocument;
use pulldown_cmark::{Options, Parser};
use serde::Deserialize;

const CORPUS_JSON: &str = include_str!("corpus/markdown-spec-0.13.4.json");

#[derive(Debug, Deserialize)]
struct Corpus {
    schema: u32,
    source: CorpusSource,
    suites: Vec<CorpusSuite>,
}

#[derive(Debug, Deserialize)]
struct CorpusSource {
    #[serde(rename = "crate")]
    crate_id: String,
    version: String,
    crate_checksum: String,
    vcs_revision: String,
}

#[derive(Debug, Deserialize)]
struct CorpusSuite {
    name: String,
    cases: Vec<CorpusCase>,
}

#[derive(Debug, Deserialize)]
struct CorpusCase {
    id: u32,
    markdown: String,
    html: String,
    smart_punctuation: bool,
    metadata_blocks: bool,
    old_footnotes: bool,
    subscript: bool,
    wikilinks: bool,
}

fn load_corpus() -> Corpus {
    let corpus: Corpus =
        serde_json::from_str(CORPUS_JSON).expect("pinned corpus must be valid JSON");
    assert_eq!(corpus.schema, 1);
    assert_eq!(corpus.source.crate_id, "pulldown-cmark");
    assert_eq!(corpus.source.version, "0.13.4");
    assert_eq!(
        corpus.source.crate_checksum,
        "e9f068eba8e7071c5f9511831b44f32c740d5adf574e990f946ddb53db2f314e"
    );
    assert_eq!(
        corpus.source.vcs_revision,
        "38e4d08f14ec4bd9783270e9623db7681ebed968"
    );
    let counts = corpus
        .suites
        .iter()
        .map(|suite| (suite.name.as_str(), suite.cases.len()))
        .collect::<Vec<_>>();
    assert_eq!(
        counts,
        [
            ("commonmark", 652),
            ("gfm_table", 9),
            ("gfm_strikethrough", 3),
            ("gfm_tasklist", 2),
        ]
    );
    corpus
}

fn parser_options(case: &CorpusCase) -> Options {
    let mut options = Options::empty();
    options.insert(Options::ENABLE_MATH);
    options.insert(Options::ENABLE_TABLES);
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_SUPERSCRIPT);
    options.insert(Options::ENABLE_TASKLISTS);
    options.insert(Options::ENABLE_GFM);
    options.insert(Options::ENABLE_HEADING_ATTRIBUTES);
    options.insert(Options::ENABLE_DEFINITION_LIST);
    if case.wikilinks {
        options.insert(Options::ENABLE_WIKILINKS);
    }
    if case.subscript {
        options.insert(Options::ENABLE_SUBSCRIPT);
    }
    if case.old_footnotes {
        options.insert(Options::ENABLE_OLD_FOOTNOTES);
    } else {
        options.insert(Options::ENABLE_FOOTNOTES);
    }
    if case.metadata_blocks {
        options.insert(Options::ENABLE_YAML_STYLE_METADATA_BLOCKS);
        options.insert(Options::ENABLE_PLUSES_DELIMITED_METADATA_BLOCKS);
    }
    if case.smart_punctuation {
        options.insert(Options::ENABLE_SMART_PUNCTUATION);
    }
    options
}

fn normalize_html(html: &str) -> String {
    html.replace("<br>", "<br />")
        .replace("<br/>", "<br />")
        .replace("<hr>", "<hr />")
        .replace("<hr/>", "<hr />")
        .replace(">\n<", "><")
}

#[test]
fn pinned_commonmark_and_gfm_semantics_match() {
    let corpus = load_corpus();
    for suite in corpus.suites {
        for case in suite.cases {
            let mut rendered = String::new();
            pulldown_cmark::html::push_html(
                &mut rendered,
                Parser::new_ext(&case.markdown, parser_options(&case)),
            );
            assert_eq!(
                normalize_html(&rendered),
                normalize_html(&case.html),
                "{} case {} semantic output changed",
                suite.name,
                case.id
            );
        }
    }
}

#[test]
fn source_document_preserves_every_official_fixture_without_edits() {
    let corpus = load_corpus();
    for suite in corpus.suites {
        for case in suite.cases {
            let document = SourceDocument::new(&case.markdown);
            assert_eq!(
                document.text(),
                case.markdown,
                "{} case {} changed source on load",
                suite.name,
                case.id
            );
            assert_eq!(
                document.snapshot().text(),
                case.markdown,
                "{} case {} changed source in immutable snapshot",
                suite.name,
                case.id
            );
        }
    }
}
