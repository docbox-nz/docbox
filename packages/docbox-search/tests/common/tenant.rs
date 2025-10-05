use docbox_database::models::tenant::Tenant;

/// Create a mock tenant for testing
#[allow(dead_code)]
pub fn test_tenant() -> Tenant {
    Tenant {
        id: "00000000-0000-0000-0000-000000000000".parse().unwrap(),
        name: "test".to_string(),
        db_name: "test".to_string(),
        db_secret_name: "test".to_string(),
        s3_name: "test".to_string(),
        os_index_name: "test".to_string(),
        env: "Development".to_string(),
        event_queue_url: None,
    }
}
