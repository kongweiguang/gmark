// @author kongweiguang

//! Temporary root compatibility facade for application identity.
//!
//! The root registry cannot declare `app` within this independently landed
//! slice. This facade hosts the consolidated tree until the documented root
//! registry switch moves this declaration to `lib.rs`.

#[path = "app/mod.rs"]
pub(crate) mod app;

pub(crate) use app::bootstrap::identity::*;
