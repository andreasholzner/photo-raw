//! photoflow: automation around darktable for the repetitive parts of raw editing.
//! The binary is a thin command-line front end; the executable scenarios in `features/`
//! (ADR-0006) test this library.

pub mod aurora;
pub mod classify;
pub mod config;
pub mod darktable;
pub mod edit_state;
pub mod exif;
pub mod image_diff;
pub mod locate;
pub mod process;
pub mod reference_set;
pub mod scenarios;
pub mod scene;
pub mod selftest;
pub mod sidecar;
