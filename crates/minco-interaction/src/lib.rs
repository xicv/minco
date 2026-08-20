//! Provider-neutral interaction primitives for optional Minco features.
#![forbid(unsafe_code)]

mod activity;
mod attachment;
mod support_entry;
mod transcription;
mod workflow;

pub use activity::*;
pub use attachment::*;
pub use support_entry::*;
pub use transcription::*;
pub use workflow::*;
