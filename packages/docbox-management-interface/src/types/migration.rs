use serde::{Deserialize, Serialize};

/// Which service the migration is for
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TenantMigrationService {
    Database,
    Search,
    Storage,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PendingMigration {
    /// The service the migration is for
    pub service: TenantMigrationService,
    /// The name of the migration
    pub name: String,
}
