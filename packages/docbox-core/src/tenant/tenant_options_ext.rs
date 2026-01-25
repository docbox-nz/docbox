use docbox_database::models::tenant::Tenant;
use docbox_storage::StorageLayerOptions;

/// Extension trait for [Tenant] to provide storage layer options
/// to initialize a [`docbox_storage::StorageLayer`]
pub trait TenantOptionsExt {
    fn storage_layer_options(&self) -> StorageLayerOptions;
}

impl TenantOptionsExt for Tenant {
    fn storage_layer_options(&self) -> StorageLayerOptions {
        StorageLayerOptions {
            bucket_name: self.s3_name.clone(),
        }
    }
}
