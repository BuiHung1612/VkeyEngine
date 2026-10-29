//! VKeyEngine core: platform independent Vietnamese input engine.

pub mod convert;
pub mod engine;
pub mod macros;
#[doc(hidden)]
pub mod sim;
pub mod smart_switch;
pub mod stats;
pub mod tables;

#[cfg(target_os = "macos")]
pub mod mac;

#[cfg(windows)]
pub mod win;
