//! First-class, AI-ready client feedback loops for Minco applications.
//!
//! The crate provides a feedback domain and application service, a small
//! framework-independent Web Component, screenshot and voice capture, optional
//! transcription providers, developer inbox APIs, a clarification/status loop,
//! durable `PostgreSQL` and `SQLite` adapters, notifications, audit events, domain
//! events, and deterministic Markdown/JSON context for coding agents.
#![forbid(unsafe_code)]

mod model;
mod service;
mod store;
mod transcription;

#[cfg(feature = "client")]
mod client;
#[cfg(feature = "http")]
mod http;
#[cfg(any(feature = "postgres", feature = "sqlite"))]
mod persistence;
#[cfg(feature = "http")]
mod plugin;

pub use model::*;
pub use service::*;
pub use store::*;
pub use transcription::*;

#[cfg(feature = "client")]
pub use client::*;
#[cfg(feature = "http")]
pub use http::{
    ClientCreateResponse, ClientFeedbackAttachment, ClientFeedbackThread, ClientMutationResponse,
    feedback_request_body_budget, feedback_router,
};
#[cfg(any(feature = "postgres", feature = "sqlite"))]
pub use persistence::*;
#[cfg(feature = "http")]
pub use plugin::FeedbackPlugin;
