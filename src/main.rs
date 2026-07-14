mod cli;
mod error;
mod image;
mod auth;
mod registry;
mod sync;
mod logging;

use clap::Parser;
use cli::{Cli, Commands};

#[tokio::main]
async fn main() {
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
    };

    if let Err(e) = result {
        logging::error(&e.to_string());
        std::process::exit(1);
    }
}
