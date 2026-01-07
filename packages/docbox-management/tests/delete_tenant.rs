/// Tests deleting a tenant using a opensearch search backend
#[tokio::test]
async fn test_delete_tenant_opensearch() {
    // TODO:
}

/// Tests deleting a tenant using a typesense search backend
#[tokio::test]
async fn test_delete_tenant_typesense() {
    // TODO:
}

/// Tests deleting a tenant using a database search backend
#[tokio::test]
async fn test_delete_tenant_database() {
    // TODO:
}

/// Tests that deleting a tenant including the delete content option deletes
/// all the content within the tenant
#[tokio::test]
async fn test_delete_tenant_contents() {
    // TODO:
}

/// Tests that the storage can be maintained optionally when deleting a tenant
#[tokio::test]
async fn test_delete_tenant_maintain_storage() {
    // TODO:
}

/// Tests that the search index can be maintained optionally when deleting a tenant
#[tokio::test]
async fn test_delete_tenant_maintain_search() {
    // TODO:
}

/// Tests that the database can be maintained optionally when deleting a tenant
#[tokio::test]
async fn test_delete_tenant_maintain_database() {
    // TODO:
}

/// Tests that a missing database can be tolerated when deleting a tenant
#[tokio::test]
async fn test_delete_tenant_handle_missing_database() {
    // TODO:
}
