pub mod doctor;
pub mod setup;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "fila", version, about = "GitHub merge queue")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Subcommand)]
pub enum Command {
    /// Validate config, database, and GitHub auth
    Doctor,
    /// Interactive wizard to create a .env file
    Setup,
}
