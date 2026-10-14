//! The clock behind pywr-core's timings.
//!
//! `std::time::Instant::now` panics on `wasm32-unknown-unknown`, so that target uses
//! `web_time::Instant`, which reads the JavaScript host's clock.

#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
pub use std::time::Instant;
#[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
pub use web_time::Instant;
