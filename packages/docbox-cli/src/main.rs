use clap::{Parser, Subcommand};
use docbox_core::{
    search::SearchIndexFactoryConfig, secrets::SecretsManagerConfig,
    storage::StorageLayerFactoryConfig,
};
use eyre::Context;
use serde::Deserialize;
use sqlx::{postgres::PgConnectOptions, PgPool};
use std::path::PathBuf;
use uuid::Uuid;

mod create_root;
mod create_tenant;
mod delete_tenant;
mod get_tenant;
mod migrate;
mod rebuild_tenant_index;

#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Args {
    #[command(subcommand)]
    pub command: Option<Commands>,

    /// Path to the cli configuration file
    #[arg(short, long)]
    pub config: PathBuf,
}

#[derive(Clone, Deserialize)]
pub struct CliConfiguration {
    pub database: CliDatabaseConfiguration,
    pub secrets: SecretsManagerConfig,
    pub search: SearchIndexFactoryConfig,
    pub storage: StorageLayerFactoryConfig,
}

#[derive(Clone, Deserialize)]
pub struct CliDatabaseConfiguration {
    pub host: String,
    pub port: u16,
    pub username: String,
    pub password: String,
    pub root_secret_name: String,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Initialize the root docbox database
    CreateRoot {},

    /// Create a new tenant
    CreateTenant {
        /// File containing the tenant configuration details
        #[arg(short, long)]
        file: PathBuf,
    },

    /// Rebuild the tenant search index from its files
    RebuildTenantIndex {
        /// Environment of the tenant
        #[arg(short, long)]
        env: String,

        /// ID of the tenant to rebuild
        #[arg(short, long)]
        tenant_id: Uuid,
    },

    /// Delete a tenant
    DeleteTenant {
        // Environment to target
        #[arg(short, long)]
        env: String,
        /// Specific tenant to delete
        #[arg(short, long)]
        tenant_id: Uuid,
    },

    /// Get a tenant
    GetTenant {
        // Environment to target
        #[arg(short, long)]
        env: String,
        /// Specific tenant to delete
        #[arg(short, long)]
        tenant_id: Uuid,
    },

    /// Run a migration
    Migrate {
        // Environment to target
        #[arg(short, long)]
        env: String,
        /// File containing the migration
        #[arg(short, long)]
        file: PathBuf,
        /// Specific tenant to run against
        #[arg(short, long)]
        tenant_id: Option<Uuid>,

        #[arg(short, long)]
        skip_failed: bool,
    },
}

#[tokio::main]
async fn main() -> eyre::Result<()> {
    // Load environment variables
    _ = dotenvy::dotenv();

    // Setup colorful error logging
    color_eyre::install()?;

    // Start configuring a `fmt` subscriber
    let subscriber = tracing_subscriber::fmt()
        // Use the logging options from env variables
        .with_env_filter("aws_sdk_secretsmanager=info,aws_runtime=info,aws_smithy_runtime=info,hyper_util=info,debug")
        // Display source code file paths
        .with_file(true)
        // Display source code line numbers
        .with_line_number(true)
        // Don't display the event's target (module path)
        .with_target(false)
        // Build the subscriber
        .finish();

    // use that subscriber to process traces emitted after this point
    tracing::subscriber::set_global_default(subscriber)?;

    let args = Args::parse();

    let command = match args.command {
        Some(command) => command,
        None => {
            return Err(eyre::eyre!("please specify a command"));
        }
    };

    // Load the create tenant config
    let config_raw = tokio::fs::read(args.config).await?;
    let config: CliConfiguration =
        serde_json::from_slice(&config_raw).context("failed to parse config")?;

    match command {
        Commands::CreateRoot {} => {
            create_root::create_root(&config).await?;
            Ok(())
        }
        Commands::CreateTenant { file } => {
            create_tenant::create_tenant(&config, file).await?;
            Ok(())
        }
        Commands::DeleteTenant { env, tenant_id } => {
            delete_tenant::delete_tenant(&config, env, tenant_id).await?;
            Ok(())
        }
        Commands::GetTenant { env, tenant_id } => {
            get_tenant::get_tenant(&config, env, tenant_id).await?;
            Ok(())
        }
        Commands::Migrate {
            env,
            file,
            tenant_id,
            skip_failed,
        } => {
            migrate::migrate(&config, env, file, tenant_id, skip_failed).await?;
            Ok(())
        }
        Commands::RebuildTenantIndex { env, tenant_id } => {
            rebuild_tenant_index::rebuild_tenant_index(&config, env, tenant_id).await?;
            Ok(())
        }
    }
}

async fn connect_db(
    host: &str,
    port: u16,
    username: &str,
    password: &str,
    database: &str,
) -> eyre::Result<PgPool> {
    println!("connecting to database {database}");
    let options = PgConnectOptions::new()
        .host(host)
        .port(port)
        .username(username)
        .password(password)
        .database(database);

    PgPool::connect_with(options)
        .await
        .context("failed to connect to database")
}
