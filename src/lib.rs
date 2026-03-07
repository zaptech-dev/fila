pub mod batch;
pub mod config;
pub mod entity;
pub mod github;
pub mod queue;

use std::sync::Arc;

use rapina::cache::CacheConfig;
use rapina::database::DatabaseConfig;
use rapina::middleware::RequestLogMiddleware;
use rapina::prelude::*;
use rapina::schemars;

mod migrations;

use config::app::AppConfig;
use github::client::GitHubClient;

#[derive(Serialize, JsonSchema)]
struct MessageResponse {
    message: String,
}

#[get("/")]
async fn hello() -> Json<MessageResponse> {
    Json(MessageResponse {
        message: "Hello from Rapina!".to_string(),
    })
}

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
