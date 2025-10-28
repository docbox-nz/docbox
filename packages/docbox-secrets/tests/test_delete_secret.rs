use crate::common::aws::{test_aws_secrets_manager_client, test_loker_container};
use docbox_secrets::{SecretManager, memory::MemorySecretManager};

mod common;

#[tokio::test]
async fn test_delete_secret_aws_success() {
    let loker_container = test_loker_container().await;
    let secrets_manager = test_aws_secrets_manager_client(&loker_container).await;

    // Should report a Created outcome
    secrets_manager.set_secret("test", "test").await.unwrap();

    // Should have a secret
    assert!(secrets_manager.has_secret("test").await.unwrap());

    // Delete the secret
    secrets_manager.delete_secret("test", true).await.unwrap();

    // Should not have a secret
    assert!(!secrets_manager.has_secret("test").await.unwrap());
}

#[tokio::test]
async fn test_delete_secret_memory_success() {
    let secrets_manager = SecretManager::Memory(MemorySecretManager::default());

    // Should report a Created outcome
    secrets_manager.set_secret("test", "test").await.unwrap();

    // Should have a secret
    assert!(secrets_manager.has_secret("test").await.unwrap());

    // Delete the secret
    secrets_manager.delete_secret("test", true).await.unwrap();

    // Should not have a secret
    assert!(!secrets_manager.has_secret("test").await.unwrap());
}
