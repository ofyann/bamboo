use bamboo::cli::{Cli, Commands};
use clap::Parser;

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    let verbose = match &cli.command {
        Commands::Sync(args) => args.verbose.unwrap_or(false),
        Commands::SyncAll(args) => args.verbose.unwrap_or(false),
        Commands::Init(_) => false,
    };

    let result = match cli.command {
        Commands::Sync(args) => {
            let level = bamboo::logging::level_from_flags(args.quiet, args.verbose);
            bamboo::logging::init_subscriber(level);
            match bamboo::config_resolver::resolve_sync(&args).await {
                Ok(spec) => bamboo::sync::run(spec).await,
                Err(e) => Err(e),
            }
        }
        Commands::SyncAll(args) => {
            let level = bamboo::logging::level_from_flags(args.quiet, args.verbose);
            bamboo::logging::init_subscriber(level);
            match bamboo::config_resolver::resolve_sync_all(&args).await {
                Ok((specs, options)) => bamboo::sync_all::run(specs, options).await,
                Err(e) => Err(e),
            }
        }
        Commands::Init(args) => bamboo::init::run(args),
    };

    if let Err(e) = result {
        if verbose {
            tracing::error!("{:?}", e);
        } else {
            tracing::error!("{}", bamboo::error_reporter::format(&e));
        }
        std::process::exit(1);
    }
}
