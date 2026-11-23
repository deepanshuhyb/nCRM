//! Core domain and database layer for ncrm.
//!
//! Everything that touches a studio's data goes through [`db::TenantTx`], a
//! transaction that is locked to one organization by Postgres row-level
//! security. See `docs/TENANCY.md`.

pub mod access;
pub mod attendance;
pub mod audit;
pub mod auth;
pub mod batches;
pub mod branches;
pub mod credits;
pub mod db;
pub mod enrollments;
pub mod error;
pub mod fees;
pub mod import;
pub mod orgs;
pub mod phone;
pub mod plans;
pub mod rooms;
pub mod staff;
pub mod students;
pub mod validate;

pub use error::{Error, Result};
