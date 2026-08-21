//! Concrete order persistence adapters.
#![forbid(unsafe_code)]

pub mod audit;
pub mod jobs;
#[cfg(feature = "dynamodb")]
mod dynamodb;
mod memory;
#[cfg(feature = "postgres")]
mod postgres;
#[cfg(feature = "sqlite")]
mod sqlite;

pub use audit::OrderAuditReader;
#[cfg(feature = "dynamodb")]
pub use dynamodb::DynamoDbOrderStore;
pub use memory::MemoryOrderStore;
#[cfg(feature = "postgres")]
pub use postgres::PostgresOrderStore;
#[cfg(feature = "sqlite")]
pub use sqlite::SqliteOrderStore;
