//! Concrete order persistence adapters.
#![forbid(unsafe_code)]

mod memory;
#[cfg(feature = "postgres")]
mod postgres;
#[cfg(feature = "sqlite")]
mod sqlite;

pub use memory::MemoryOrderStore;
#[cfg(feature = "postgres")]
pub use postgres::PostgresOrderStore;
#[cfg(feature = "sqlite")]
pub use sqlite::SqliteOrderStore;
