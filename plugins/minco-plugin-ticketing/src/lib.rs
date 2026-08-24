//! Project-scoped Ticketing for Minco applications.
#![forbid(unsafe_code)]

pub(crate) const MAX_TICKET_LIST_FETCH_LIMIT: usize = 201;

#[cfg(feature = "http")]
mod http;
#[cfg(feature = "jobs")]
mod jobs;
mod model;
#[cfg(feature = "sqlite")]
mod persistence;
#[cfg(feature = "http")]
mod plugin;
mod service;
mod store;

#[cfg(feature = "http")]
pub use http::*;
#[cfg(feature = "jobs")]
pub use jobs::*;
pub use model::*;
#[cfg(feature = "sqlite")]
pub use persistence::*;
#[cfg(feature = "http")]
pub use plugin::TicketingPlugin;
pub use service::*;
pub use store::*;
