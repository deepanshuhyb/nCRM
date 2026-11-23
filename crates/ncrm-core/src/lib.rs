//! Core domain and database layer for ncrm.
//!
//! Everything that touches a studio's data goes through [`db::TenantTx`], a
//! transaction that is locked to one organization by Postgres row-level
//! security. See `docs/TENANCY.md`.

pub mod access;
pub mod audit;
pub mod auth;
pub mod branches;
pub mod db;
pub mod error;
pub mod orgs;
pub mod phone;
pub mod staff;

pub use error::{Error, Result};
