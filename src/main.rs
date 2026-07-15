use bamboo::cli::{Cli, Commands};
use clap::Parser;

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

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
        tracing::error!("{}", e);
        std::process::exit(1);
    }
}
