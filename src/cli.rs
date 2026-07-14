use clap::{Parser, Subcommand};
use std::time::Duration;

#[derive(Parser, Debug)]
#[command(name = "bamboo")]
#[command(about = "Sync container images between registries")]
#[command(version)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    Sync(SyncArgs),
}

#[derive(Parser, Debug, Clone)]
pub struct SyncArgs {
    /// Image reference to sync, e.g. nginx:1.25 or quay.io/coreos/etcd:v3.5
    pub image: String,

    /// Source registry (e.g. a HubProxy mirror)
    #[arg(long, env = "BAMBOO_SOURCE_REGISTRY", default_value = "hubproxy.example.com")]
    pub source_registry: String,

    /// Target registry (your private Docker Distribution)
    #[arg(long, env = "BAMBOO_TARGET_REGISTRY", default_value = "registry.example.com:5000")]
    pub target_registry: String,

    /// Dry run: only print source and target URIs
    #[arg(long, short, default_value_t = false)]
    pub dry_run: bool,

    /// Source registry credentials as user:pass
    #[arg(long, env = "BAMBOO_SOURCE_CREDS")]
    pub source_creds: Option<String>,

    /// Target registry credentials as user:pass
    #[arg(long, env = "BAMBOO_CREDS")]
    pub creds: Option<String>,

    /// Path to docker config auth file (used for both source and target registries)
    #[arg(long, env = "BAMBOO_AUTHFILE", default_value = "~/.docker/config.json")]
    pub authfile: String,

    /// Skip TLS verification for source registry
    #[arg(long, env = "BAMBOO_INSECURE_SRC", default_value_t = false)]
    pub insecure_src: bool,

    /// Skip TLS verification for target registry
    #[arg(long, env = "BAMBOO_INSECURE_DEST", default_value_t = false)]
    pub insecure_dest: bool,

    /// Number of retries on failure
    #[arg(long, env = "BAMBOO_RETRIES", default_value_t = 3)]
    pub retries: usize,

    /// Delay between retries
    #[arg(long, env = "BAMBOO_RETRY_DELAY", value_parser = parse_duration, default_value = "5s")]
    pub retry_delay: Duration,

    /// Force sync even if digests match
    #[arg(long, default_value_t = false)]
    pub force: bool,
}

fn parse_duration(s: &str) -> Result<Duration, String> {
    humantime::parse_duration(s).map_err(|e| e.to_string())
}
