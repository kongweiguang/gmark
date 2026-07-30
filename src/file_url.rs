// @author kongweiguang

//! Backwards-compatible file URL facade.

// This facade is consumed by the macOS open-URL callback and its external unit
// tests; other targets do not compile the root module outside test builds.
#[cfg(any(target_os = "macos", test))]
pub(crate) use crate::ui::platform::url::*;

#[cfg(test)]
#[path = "../tests/unit/file_url.rs"]
mod tests;
