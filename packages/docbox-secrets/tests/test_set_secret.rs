use docbox_secrets::{Secret, SecretManager, SetSecretOutcome, memory::MemorySecretManager};

use crate::common::aws::{test_aws_secrets_manager_client, test_loker_container};

mod common;

#[tokio::test]
async fn test_set_secret_aws_success() {
    let loker_container = test_loker_container().await;
    let secrets_manager = test_aws_secrets_manager_client(&loker_container).await;

    // Should not have a secret
    assert!(!secrets_manager.has_secret("test").await.unwrap());

    // Create secret
    secrets_manager.set_secret("test", "test").await.unwrap();

    // Should retrieve the same value
    let value = secrets_manager.get_secret("test").await.unwrap();
    assert_eq!(value, Some(Secret::String("test".to_string())));

    // Should report a Updated outcome
    let outcome = secrets_manager.set_secret("test", "test-2").await.unwrap();
    assert_eq!(outcome, SetSecretOutcome::Updated);

    // Should retrieve the new value
    let value = secrets_manager.get_secret("test").await.unwrap();
    assert_eq!(value, Some(Secret::String("test-2".to_string())));
}

#[tokio::test]
async fn test_set_secret_memory_success() {
    let secrets_manager = SecretManager::Memory(MemorySecretManager::default());

    // Should not have a secret
    assert!(!secrets_manager.has_secret("test").await.unwrap());

    // Create secret
    secrets_manager.set_secret("test", "test").await.unwrap();

    // Should retrieve the same value
    let value = secrets_manager.get_secret("test").await.unwrap();
    assert_eq!(value, Some(Secret::String("test".to_string())));

    // Should report a Updated outcome
    let outcome = secrets_manager.set_secret("test", "test-2").await.unwrap();
    assert_eq!(outcome, SetSecretOutcome::Updated);

    // Should retrieve the new value
    let value = secrets_manager.get_secret("test").await.unwrap();
    assert_eq!(value, Some(Secret::String("test-2".to_string())));
}
