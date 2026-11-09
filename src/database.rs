//! File: Jason Stys: narrow pinned `SQLx` PostgreSQL imports without unused SQLite/MySQL/macro packages.
//! Exports: original query, pool, transaction, migration and error types; no new database behavior.
//! Variables: none; exact versions stay synchronized in Cargo.toml and Cargo.lock.

pub use sqlx_core::{
    error, error::Error, migrate, row::Row, sql_str::SqlStr, transaction::Transaction,
};
pub use sqlx_core::{query::query, query_scalar::query_scalar};
pub use sqlx_postgres as postgres;
pub use sqlx_postgres::{PgPool, Postgres};
