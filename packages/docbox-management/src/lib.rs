#![forbid(unsafe_code)]

pub mod config;
pub mod database;
pub mod password;
pub mod root;
pub mod server;
pub mod tenant;

/// docbox-core re-exports
pub mod core {
    pub use docbox_core::*;
}
