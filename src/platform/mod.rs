// @author kongweiguang

//! OS-facing window, URL, accessibility, and instance services.
//!
//! Platform modules deliberately depend only on shared configuration, identity,
//! GPUI, and UI tokens. They do not depend on editor or application modules.

pub(crate) mod accessibility;
pub(crate) mod identity;
#[cfg(target_os = "windows")]
pub(crate) mod single_instance;
#[cfg(any(target_os = "macos", test))]
pub(crate) mod url;
pub(crate) mod window;
