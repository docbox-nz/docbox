#![forbid(unsafe_code)]

pub mod database;
pub mod password;
pub mod root;
pub mod tenant;

/// docbox-core re-exports
pub mod core {
    pub use docbox_core::*;
}

/// docbox-search re-exports
pub mod search {
    pub use docbox_search::*;
}

/// docbox-secrets re-exports
pub mod secrets {
    pub use docbox_secrets::*;
}

/// docbox-storage re-exports
pub mod storage {
    pub use docbox_storage::*;
}
