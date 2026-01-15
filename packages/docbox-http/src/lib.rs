#![forbid(unsafe_code)]

pub mod docs;
pub mod error;
pub mod extensions;
pub mod middleware;
pub mod models;
pub mod routes;

/// Re-exports of the docbox-core crate
pub mod core {
    pub use docbox_core::*;
}
