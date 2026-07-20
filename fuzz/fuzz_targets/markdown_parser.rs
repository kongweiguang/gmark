// @author kongweiguang

#![no_main]

use libfuzzer_sys::fuzz_target;
use pulldown_cmark::{Options, Parser};

fuzz_target!(|data: &[u8]| {
    let Ok(markdown) = std::str::from_utf8(data) else {
        return;
    };
    let options = Options::ENABLE_GFM
        | Options::ENABLE_FOOTNOTES
        | Options::ENABLE_MATH
        | Options::ENABLE_TABLES
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_STRIKETHROUGH;
    let parser = Parser::new_ext(markdown, options);
    for (_, range) in parser.into_offset_iter() {
        assert!(range.start <= range.end);
        assert!(range.end <= markdown.len());
        assert!(markdown.is_char_boundary(range.start));
        assert!(markdown.is_char_boundary(range.end));
    }

    let mut html = String::new();
    pulldown_cmark::html::push_html(&mut html, Parser::new_ext(markdown, options));
});
