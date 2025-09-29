use docbox_database::{models::tenant::Tenant, sqlx::types::Uuid};
use serde::Serialize;

pub mod verify_storage;

#[derive(Debug, Clone, Default, Serialize)]
#[serde(tag = "type")]
pub enum VerifyOutcome {
    /// Not yet verified
    #[default]
    Pending,
    /// Successfully verified
    Success,
    /// Not applicable for the current config
    Skipped,
    /// Failed verification test
    Failure { message: String },
}

/// Dummy tenant details used for the verification process
pub(crate) fn verify_dummy_tenant() -> Tenant {
    Tenant {
        id: Uuid::nil(),
        name: "Docbox Verification Tenant".to_string(),
        db_name: "docbox-verification-test-do-not-use".to_string(),
        db_secret_name: "docbox-verification-test-do-not-use".to_string(),
        s3_name: "docbox-verification-test-do-not-use".to_string(),
        os_index_name: "docbox-verification-test-do-not-use".to_string(),
        env: "DocboxVerification".to_string(),
        event_queue_url: None,
    }
}
