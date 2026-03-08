use rapina::prelude::*;

#[derive(Config, Clone)]
pub struct AppConfig {
    #[env = "DATABASE_URL"]
    pub database_url: String,

    #[env = "SERVER_PORT"]
    #[default = "8000"]
    pub server_port: u16,

    #[env = "HOST"]
    #[default = "127.0.0.1"]
    pub host: String,

    #[env = "GITHUB_APP_ID"]
    pub github_app_id: String,

    #[env = "GITHUB_PRIVATE_KEY"]
    pub github_private_key: String,

    #[env = "GITHUB_WEBHOOK_SECRET"]
    pub github_webhook_secret: String,

    #[env = "MERGE_STRATEGY"]
    #[default = "batch"]
    pub merge_strategy: String,

    #[env = "BATCH_SIZE"]
    #[default = "5"]
    pub batch_size: usize,

    #[env = "BATCH_INTERVAL_SECS"]
    #[default = 10]
    pub batch_interval_secs: usize,

    #[env = "CI_TIMEOUT_SECS"]
    #[default = "1800"]
    pub ci_timeout_secs: u32,

    #[env = "POLL_INTERVAL_SECS"]
    #[default = "15"]
    pub poll_interval_secs: u32,

    #[env = "DASHBOARD_URL"]
    #[default = ""]
    pub dashboard_url: String,
}
