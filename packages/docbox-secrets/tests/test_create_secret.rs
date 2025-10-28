use docbox_secrets::{Secret, SecretManager, SetSecretOutcome, memory::MemorySecretManager};

use crate::common::aws::{test_aws_secrets_manager_client, test_loker_container};

mod common;

#[tokio::test]
async fn test_create_secret_aws_success() {
    let loker_container = test_loker_container().await;
    let secrets_manager = test_aws_secrets_manager_client(&loker_container).await;

    // Should not have a secret
    assert!(!secrets_manager.has_secret("test").await.unwrap());

    // Should report a Created outcome
    let outcome = secrets_manager.set_secret("test", "test").await.unwrap();
    assert_eq!(outcome, SetSecretOutcome::Created);

    // Should retrieve the same value
    let value = secrets_manager.get_secret("test").await.unwrap();
    assert_eq!(value, Some(Secret::String("test".to_string())));
}

#[tokio::test]
async fn test_create_secret_memory_success() {
    let secrets_manager = SecretManager::Memory(MemorySecretManager::default());

    // Should not have a secret
    assert!(!secrets_manager.has_secret("test").await.unwrap());

    // Should report a Created outcome
    let outcome = secrets_manager.set_secret("test", "test").await.unwrap();
    assert_eq!(outcome, SetSecretOutcome::Created);

    // Should retrieve the same value
    let value = secrets_manager.get_secret("test").await.unwrap();
    assert_eq!(value, Some(Secret::String("test".to_string())));
}
