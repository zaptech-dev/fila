use std::env;
use std::path::Path;

use jsonwebtoken::EncodingKey;
use rapina::database::DatabaseConfig;

use crate::github::client::GitHubClient;

fn print_line(label: &str, value: &str) {
    println!("{:<24} {}", label, value);
}

pub async fn run() -> i32 {
    let mut issues: Vec<String> = Vec::new();

    println!("fila doctor\n");

    // .env file presence
    let dotenv_exists = Path::new(".env").exists();
    print_line(
        ".env file",
        if dotenv_exists { "found" } else { "not found" },
    );

    // Required env vars — never print values
    let required = [
        "DATABASE_URL",
        "GITHUB_APP_ID",
        "GITHUB_PRIVATE_KEY",
        "GITHUB_WEBHOOK_SECRET",
    ];

    for var in &required {
        match env::var(var) {
            Ok(_) => print_line(var, "set"),
            Err(_) => {
                print_line(var, "MISSING");
                issues.push(format!("{var} is not set"));
            }
        }
    }

    println!();

    // Optional vars with defaults
    let optionals: &[(&str, &str)] = &[
        ("SERVER_PORT", "8000"),
        ("HOST", "127.0.0.1"),
        ("MERGE_STRATEGY", "batch"),
        ("BATCH_SIZE", "5"),
        ("BATCH_INTERVAL_SECS", "10"),
        ("CI_TIMEOUT_SECS", "1800"),
        ("POLL_INTERVAL_SECS", "15"),
    ];

    for (var, default) in optionals {
        let value = env::var(var).unwrap_or_else(|_| default.to_string());
        print_line(var, &value);
    }

    // Validate MERGE_STRATEGY
    let strategy = env::var("MERGE_STRATEGY").unwrap_or_else(|_| "batch".to_string());
    if strategy != "batch" && strategy != "sequential" {
        issues.push(format!(
            "MERGE_STRATEGY is \"{strategy}\", expected \"batch\" or \"sequential\""
        ));
    }

    println!();

    // Private key validation
    let key_result = env::var("GITHUB_PRIVATE_KEY");
    match &key_result {
        Ok(key) => match EncodingKey::from_rsa_pem(key.as_bytes()) {
            Ok(_) => print_line("Private key", "valid RSA PEM"),
            Err(e) => {
                print_line("Private key", &format!("INVALID ({e})"));
                issues.push(format!("GITHUB_PRIVATE_KEY is not valid RSA PEM: {e}"));
            }
        },
        Err(_) => print_line("Private key", "skipped (not set)"),
    }

    // Database connectivity
    let db_result = env::var("DATABASE_URL");
    match &db_result {
        Ok(url) => {
            let db_config = DatabaseConfig::new(url);
            match db_config.connect().await {
                Ok(_) => {
                    let db_type = if url.starts_with("sqlite") {
                        "sqlite"
                    } else if url.starts_with("postgres") {
                        "postgres"
                    } else {
                        "unknown"
                    };
                    print_line("Database", &format!("connected ({db_type})"));
                }
                Err(e) => {
                    print_line("Database", &format!("FAILED ({e})"));
                    issues.push(format!("Database connection failed: {e}"));
                }
            }
        }
        Err(_) => print_line("Database", "skipped (DATABASE_URL not set)"),
    }

    // GitHub API auth
    let app_id = env::var("GITHUB_APP_ID");
    match (&app_id, &key_result) {
        (Ok(id), Ok(key)) => {
            let client = GitHubClient::new(id.clone(), key.clone());
            match client.get_app_info().await {
                Ok(info) => {
                    print_line("GitHub API", &format!("authenticated as \"{}\"", info.name));
                }
                Err(e) => {
                    print_line("GitHub API", &format!("FAILED ({e})"));
                    issues.push(format!("GitHub API authentication failed: {e}"));
                }
            }
        }
        _ => print_line("GitHub API", "skipped (credentials not set)"),
    }

    // Summary
    println!();
    if issues.is_empty() {
        println!("All checks passed.");
        0
    } else {
        println!(
            "{} {} found:",
            issues.len(),
            if issues.len() == 1 { "issue" } else { "issues" }
        );
        for issue in &issues {
            println!("  - {issue}");
        }
        1
    }
}
