use std::sync::Arc;

use clap::Parser;

use fila::cli::{Cli, Command};
use fila::config::app::AppConfig;
use fila::github::client::GitHubClient;
use fila::queue;
use rapina::database::DatabaseConfig;

#[tokio::main]
async fn main() -> std::io::Result<()> {
    dotenvy::dotenv().ok();

    let cli = Cli::parse();

    match cli.command {
        Some(Command::Doctor) => {
            let code = fila::cli::doctor::run().await;
            std::process::exit(code);
        }
        Some(Command::Setup) => {
            let code = fila::cli::setup::run();
            std::process::exit(code);
        }
        None => {
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
            queue::runner::spawn(runner_db, github.clone(), config.clone());

            fila::build_app(config, github, true)
                .await
                .listen(&addr)
                .await
        }
    }
}
