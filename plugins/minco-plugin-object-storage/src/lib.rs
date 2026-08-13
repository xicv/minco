//! Provider-neutral object storage, direct access, and verified upload workflows.
#![forbid(unsafe_code)]

mod base;
#[cfg(feature = "http")]
mod http;
mod transfers;
mod uploads;

pub use base::*;
#[cfg(feature = "http")]
pub use http::*;
pub use transfers::*;
pub use uploads::*;
