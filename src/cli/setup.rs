use std::fs;
use std::io::{self, Write};
use std::path::Path;

fn prompt(label: &str, default: &str) -> String {
    if default.is_empty() {
        print!("{label}: ");
    } else {
        print!("{label} [{default}]: ");
    }
    io::stdout().flush().unwrap();

    let mut input = String::new();
    io::stdin().read_line(&mut input).unwrap();
    let trimmed = input.trim();

    if trimmed.is_empty() {
        default.to_string()
    } else {
        trimmed.to_string()
    }
}

pub fn run() -> i32 {
    println!("fila setup\n");

    if Path::new(".env").exists() {
        print!(".env already exists. Overwrite? [y/N]: ");
        io::stdout().flush().unwrap();
        let mut answer = String::new();
        io::stdin().read_line(&mut answer).unwrap();
        if !answer.trim().eq_ignore_ascii_case("y") {
            println!("Aborted.");
            return 0;
        }
        println!();
    }

    let app_id = prompt("GitHub App ID", "");
    let key_path = prompt("Path to private key file (PEM)", "");
    let webhook_secret = prompt("GitHub Webhook Secret", "");
    let database_url = prompt("Database URL", "sqlite://fila.db?mode=rwc");
    let merge_strategy = prompt("Merge strategy (batch/sequential)", "batch");
    let batch_size = prompt("Batch size", "5");
    let batch_interval = prompt("Batch interval (seconds)", "10");
    let ci_timeout = prompt("CI timeout (seconds)", "1800");
    let poll_interval = prompt("Poll interval (seconds)", "15");
    let server_port = prompt("Server port", "8000");
    let host = prompt("Host", "127.0.0.1");

    // Read private key from file
    let private_key = if key_path.is_empty() {
        String::new()
    } else {
        match fs::read_to_string(&key_path) {
            Ok(contents) => contents,
            Err(e) => {
                println!("Failed to read private key file: {e}");
                return 1;
            }
        }
    };

    let mut env_content = String::new();

    env_content.push_str(&format!("DATABASE_URL={database_url}\n"));
    env_content.push_str(&format!("GITHUB_APP_ID={app_id}\n"));

    if !private_key.is_empty() {
        // Wrap in double quotes so dotenvy handles embedded newlines
        env_content.push_str(&format!("GITHUB_PRIVATE_KEY=\"{}\"\n", private_key.trim()));
    }

    env_content.push_str(&format!("GITHUB_WEBHOOK_SECRET={webhook_secret}\n"));
    env_content.push_str(&format!("MERGE_STRATEGY={merge_strategy}\n"));
    env_content.push_str(&format!("BATCH_SIZE={batch_size}\n"));
    env_content.push_str(&format!("BATCH_INTERVAL_SECS={batch_interval}\n"));
    env_content.push_str(&format!("CI_TIMEOUT_SECS={ci_timeout}\n"));
    env_content.push_str(&format!("POLL_INTERVAL_SECS={poll_interval}\n"));
    env_content.push_str(&format!("SERVER_PORT={server_port}\n"));
    env_content.push_str(&format!("HOST={host}\n"));

    match fs::write(".env", &env_content) {
        Ok(()) => {
            println!("\n.env written successfully.");
            println!("Run `fila doctor` to verify your configuration.");
            0
        }
        Err(e) => {
            println!("Failed to write .env: {e}");
            1
        }
    }
}
