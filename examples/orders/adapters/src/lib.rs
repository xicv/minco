//! Concrete order persistence adapters.
#![forbid(unsafe_code)]

#[cfg(feature = "dynamodb")]
mod dynamodb;
mod memory;
#[cfg(feature = "postgres")]
mod postgres;
#[cfg(feature = "sqlite")]
mod sqlite;

#[cfg(feature = "dynamodb")]
pub use dynamodb::DynamoDbOrderStore;
pub use memory::MemoryOrderStore;
#[cfg(feature = "postgres")]
pub use postgres::PostgresOrderStore;
#[cfg(feature = "sqlite")]
pub use sqlite::SqliteOrderStore;
