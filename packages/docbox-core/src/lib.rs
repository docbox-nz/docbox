#![forbid(unsafe_code)]

pub mod aws;
pub mod document_box;
pub mod events;
pub mod files;
pub mod folders;
pub mod links;
pub mod notifications;
pub mod purge;
pub mod tasks;
pub mod tenant;
pub mod utils;

/// Re-exports of the docbox-database crate
pub mod database {
    pub use docbox_database::*;
}

/// Re-exports of the docbox-processing crate
pub mod processing {
    pub use docbox_processing::*;
}

/// Re-exports of the docbox-search crate
pub mod search {
    pub use docbox_search::*;
}

/// Re-exports of the docbox-secrets crate
pub mod secrets {
    pub use docbox_secrets::*;
}

/// Re-exports of the docbox-storage crate
pub mod storage {
    pub use docbox_storage::*;
}

/// Re-exports of the docbox-web-scraper crate
pub mod web_scraper {
    pub use docbox_web_scraper::*;
}
