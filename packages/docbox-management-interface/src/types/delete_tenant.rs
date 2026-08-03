use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DeleteTenantInput {
    /// Environment of the tenant to delete
    pub env: String,
    /// ID of the tenant to delete
    pub tenant_id: Uuid,
    /// Options for deleting the tenant
    #[serde(default)]
    pub options: DeleteTenantOptions,
}

/// Additional options to use when deleting the tenant
#[derive(Debug, Default, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct DeleteTenantOptions {
    /// Whether to delete data stored within the tenant
    pub delete_contents: bool,
    /// Whether to delete the tenant storage bucket itself (Requires "delete_contents")
    pub delete_storage: bool,
    /// Whether to delete the tenant search index itself (Requires "delete_contents")
    pub delete_search: bool,
    /// Whether to delete the tenant database itself (Requires "delete_contents")
    pub delete_database: bool,
    /// Whether when using AWS secrets manager to immediately delete the secret
    /// or to allow it to be recoverable for a short period of time.
    ///
    /// Note: If the secret is not immediately deleted a new tenant will not be
    /// able to make use of this secret name until the 30day recovery window
    /// has ended.
    pub permanently_delete_secret: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DeleteTenantOutput {}
