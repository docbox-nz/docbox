use crate::database::{DatabaseProvider, close_pool_on_drop};
use docbox_core::{
    database::{
        DbErr, DbSecrets, ROOT_DATABASE_NAME, create::make_role_iam_only, models::tenant::Tenant,
    },
    secrets::{SecretManager, SecretManagerError},
};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum MigrateIAMError {
    #[error("error connecting to root database: {0}")]
    ConnectRootDatabase(DbErr),

    #[error(transparent)]
    GetTenantSecret(SecretManagerError),

    #[error("tenant database secret is missing, is this already an IAM tenant?")]
    MissingTenantSecret,

    #[error("error making the role IAM accessible: {0}")]
    MakeRoleIAM(DbErr),

    #[error("error updating tenant: {0}")]
    UpdateTenant(DbErr),
}

#[tracing::instrument(skip(db_provider, secrets))]
pub async fn migrate_tenant_secret_to_iam(
    db_provider: &impl DatabaseProvider,
    secrets: &SecretManager,
    tenant: &mut Tenant,
) -> Result<(), MigrateIAMError> {
    let root_db = db_provider
        .connect(ROOT_DATABASE_NAME)
        .await
        .map_err(MigrateIAMError::ConnectRootDatabase)?;

    let _guard = close_pool_on_drop(&root_db);

    let secret_name = tenant
        .db_secret_name
        .as_ref()
        .ok_or(MigrateIAMError::MissingTenantSecret)?;

    let secret: DbSecrets = secrets
        .parsed_secret(secret_name)
        .await
        .map_err(MigrateIAMError::GetTenantSecret)?
        .ok_or(MigrateIAMError::MissingTenantSecret)?;

    make_role_iam_only(&root_db, &secret.username)
        .await
        .map_err(MigrateIAMError::MakeRoleIAM)?;

    tenant
        .set_db_iam_user_name(&root_db, Some(secret.username.clone()))
        .await
        .map_err(MigrateIAMError::UpdateTenant)?;

    tenant
        .set_db_secret_name(&root_db, None)
        .await
        .map_err(MigrateIAMError::UpdateTenant)?;

    Ok(())
}
