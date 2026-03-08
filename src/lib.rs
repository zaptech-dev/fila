pub mod batch;
mod batches;
pub mod config;
pub mod dashboard;
pub mod entity;
pub mod errors;
pub mod github;
mod merge_events;
mod pull_requests;
pub mod queue;
pub mod types;

use std::sync::Arc;

use rapina::cache::CacheConfig;
use rapina::database::DatabaseConfig;
use rapina::middleware::RequestLogMiddleware;
use rapina::prelude::*;

mod migrations;

use config::app::AppConfig;
use github::client::GitHubClient;

pub async fn build_app(config: AppConfig, enable_tracing: bool) -> Rapina {
    let db_config = DatabaseConfig::new(&config.database_url);

    let github = Arc::new(GitHubClient::new(
        config.github_app_id.clone(),
        config.github_private_key.clone(),
    ));

    let mut app = Rapina::new()
        .middleware(RequestLogMiddleware::new())
        .state(config)
        .state(github)
        .openapi("Fila", "0.1.0");

    if enable_tracing {
        app = app.with_tracing(TracingConfig::new())
    }

    app.with_database(db_config)
        .await
        .expect("Failed to connect database")
        .run_migrations::<migrations::Migrator>()
        .await
        .expect("Failed to run migrations")
        .with_cache(CacheConfig::in_memory(1000))
        .await
        .expect("Failed to initialize cache")
        .discover()
}
