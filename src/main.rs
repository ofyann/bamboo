mod cli;
mod config;
mod error;
mod image;
mod auth;
mod registry;
mod sync;
mod logging;
mod init;

use clap::Parser;
use cli::{Cli, Commands};

#[tokio::main]
async fn main() {
    config::preload_from_args();
    let cli = Cli::parse();
    let result = match cli.command {
        Commands::Sync(args) => {
            let level = if args.quiet {
                logging::LogLevel::Warn
            } else if args.verbose {
                logging::LogLevel::Debug
            } else {
                logging::LogLevel::Info
            };
            logging::set_level(level);
            sync::run(args).await
        }
        Commands::Init(args) => init::run(args),
    };

    if let Err(e) = result {
        logging::error(&e.to_string());
        std::process::exit(1);
    }
}
