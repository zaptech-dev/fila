use std::sync::Arc;

use fila::batch;
use fila::config::app::AppConfig;
use fila::github::client::GitHubClient;
use rapina::database::DatabaseConfig;

#[tokio::main]
async fn main() -> std::io::Result<()> {
    dotenvy::dotenv().ok();

    let config = AppConfig::from_env().expect("Missing initial config");

    let addr = format!("{}:{}", config.host, config.server_port);

    // Create a separate DB connection for the batch runner
    let runner_db = DatabaseConfig::new(&config.database_url)
        .connect()
        .await
        .expect("Failed to connect database for batch runner");

    let github = Arc::new(GitHubClient::new(
        config.github_app_id.clone(),
        config.github_private_key.clone(),
    ));

    // Spawn the batch runner before starting the server
    batch::runner::spawn(runner_db, github, config.clone());

    fila::build_app(config, true).await.listen(&addr).await
}
