// @author kongweiguang

#![no_main]

use gmark_fuzz_support::run_markdown_parser_program;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    run_markdown_parser_program(data);
});
