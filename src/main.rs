use bamboo::cli::{Cli, Commands};
use clap::Parser;

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    let result = match cli.command {
        Commands::Sync(args) => {
            bamboo::logging::init_from_flags(args.quiet, args.verbose);
            match bamboo::config_resolver::resolve_sync(&args).await {
                Ok(spec) => bamboo::sync::run(spec).await,
                Err(e) => Err(e),
            }
        }
        Commands::SyncAll(args) => {
            bamboo::logging::init_from_flags(args.quiet, args.verbose);
            match bamboo::config_resolver::resolve_sync_all(&args).await {
                Ok((specs, options)) => bamboo::sync_all::run(specs, options).await,
                Err(e) => Err(e),
            }
        }
        Commands::Init(args) => bamboo::init::run(args),
    };

    if let Err(e) = result {
        bamboo::logging::error(&e.to_string());
        std::process::exit(1);
    }
}
