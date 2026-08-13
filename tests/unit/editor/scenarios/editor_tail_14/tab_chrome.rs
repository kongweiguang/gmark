// @author kongweiguang

// Keep these pieces in the parent scenario module so their shared GPUI fixture
// imports remain visible and the test harness does not introduce a second
// `test` binding that conflicts with the `#[gpui::test]` attribute.
include!("tab_chrome_parts/close.rs");
include!("tab_chrome_parts/global.rs");
include!("tab_chrome_parts/layout.rs");
include!("tab_chrome_parts/panes.rs");
