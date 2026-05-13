use anyhow::{Context, Result};
use clap::Parser;
use libp2p::Multiaddr;
use std::{path::PathBuf, time::Duration};
use tracing_subscriber::EnvFilter;
use void_identity::default_node_dir;
use void_transport::{is_quic_address, run_transport_node, NetworkConfig, TransportNodeConfig};

#[derive(Debug, Parser)]
#[command(name = "void-node", about = "VOIDNET autonomous transport node")]
struct Args {
    #[arg(long)]
    data_dir: Option<PathBuf>,

    #[arg(long, default_value = "/ip4/0.0.0.0/udp/0/quic-v1")]
    listen: Multiaddr,

    #[arg(long = "bootstrap")]
    bootstrap: Vec<Multiaddr>,

    #[arg(long)]
    topology_file: Option<PathBuf>,

    #[arg(long, default_value_t = 8)]
    reconnect_secs: u64,

    #[arg(long, default_value_t = 30)]
    partition_after_secs: u64,

    #[arg(long)]
    exit_after_secs: Option<u64>,

    #[arg(long)]
    no_mdns: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("void_node=info,void_transport=info")),
        )
        .with_writer(std::io::stdout)
        .init();

    let args = Args::parse();
    if !is_quic_address(&args.listen) {
        anyhow::bail!("listen address must use QUIC: {}", args.listen);
    }

    for address in &args.bootstrap {
        if !is_quic_address(address) {
            anyhow::bail!("bootstrap address must use QUIC: {address}");
        }
    }

    let data_dir = args.data_dir.unwrap_or_else(default_node_dir);
    let network = NetworkConfig::new(vec![args.listen], args.bootstrap);
    let mut config = TransportNodeConfig::new(data_dir.clone(), network);
    config.reconnect_interval = Duration::from_secs(args.reconnect_secs);
    config.partition_after = Duration::from_secs(args.partition_after_secs);
    config.exit_after = args.exit_after_secs.map(Duration::from_secs);
    config.enable_mdns = !args.no_mdns;
    if let Some(topology_file) = args.topology_file {
        config.topology_file = topology_file;
    }

    run_transport_node(config)
        .await
        .with_context(|| format!("VOID node failed in {}", data_dir.display()))
}
