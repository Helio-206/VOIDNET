use anyhow::{Context, Result};
use clap::Parser;
use libp2p::Multiaddr;
use serde::Deserialize;
use std::{collections::BTreeSet, fs, path::{Path, PathBuf}, time::Duration};
use tracing_subscriber::EnvFilter;
use void_identity::default_node_dir;
use void_transport::{is_quic_address, run_transport_node, NetworkConfig, TransportNodeConfig};

const DEFAULT_BOOTSTRAP_CONFIG_FILE: &str = "bootstrap.toml";

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
    bootstrap_config: Option<PathBuf>,

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

    #[arg(long)]
    relay_server: bool,
}

#[derive(Debug, Default, Deserialize)]
struct BootstrapFile {
    #[serde(default)]
    bootstrap_nodes: Vec<String>,
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
    let bootstrap_config_path = args
        .bootstrap_config
        .clone()
        .unwrap_or_else(|| data_dir.join(DEFAULT_BOOTSTRAP_CONFIG_FILE));
    let bootstrap = load_bootstrap_nodes(&bootstrap_config_path, &args.bootstrap)?;
    let network = NetworkConfig::new(vec![args.listen], bootstrap);
    let mut config = TransportNodeConfig::new(data_dir.clone(), network);
    config.reconnect_interval = Duration::from_secs(args.reconnect_secs);
    config.partition_after = Duration::from_secs(args.partition_after_secs);
    config.exit_after = args.exit_after_secs.map(Duration::from_secs);
    config.enable_mdns = !args.no_mdns;
    config.enable_relay_server = args.relay_server;
    if let Some(topology_file) = args.topology_file {
        config.topology_file = topology_file;
    }

    run_transport_node(config)
        .await
        .with_context(|| format!("VOID node failed in {}", data_dir.display()))
}

fn load_bootstrap_nodes(
    config_path: &Path,
    cli_bootstrap: &[Multiaddr],
) -> Result<Vec<Multiaddr>> {
    let mut merged = BTreeSet::new();

    if config_path.exists() {
        let raw = fs::read_to_string(config_path)
            .with_context(|| format!("failed to read bootstrap config {}", config_path.display()))?;
        let file_config: BootstrapFile = toml::from_str(&raw)
            .with_context(|| format!("failed to parse bootstrap config {}", config_path.display()))?;
        for address in file_config.bootstrap_nodes {
            let parsed: Multiaddr = address
                .parse()
                .with_context(|| format!("invalid bootstrap multiaddr in {}: {address}", config_path.display()))?;
            if !is_quic_address(&parsed) {
                anyhow::bail!("bootstrap address must use QUIC: {parsed}");
            }
            merged.insert(parsed.to_string());
        }
    }

    for address in cli_bootstrap {
        merged.insert(address.to_string());
    }

    merged
        .into_iter()
        .map(|address| {
            address
                .parse::<Multiaddr>()
                .with_context(|| format!("invalid bootstrap multiaddr: {address}"))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use libp2p::identity::Keypair;
    use std::{env, time::{SystemTime, UNIX_EPOCH}};

    #[test]
    fn merges_bootstrap_config_with_cli_addresses() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = env::temp_dir().join(format!("voidnet-bootstrap-{unique}.toml"));
        let peer_a = Keypair::generate_ed25519().public().to_peer_id();
        let peer_b = Keypair::generate_ed25519().public().to_peer_id();
        let peer_c = Keypair::generate_ed25519().public().to_peer_id();
        fs::write(
            &path,
            format!(
                "bootstrap_nodes = [\n  \"/ip4/203.0.113.10/udp/40100/quic-v1/p2p/{peer_a}\",\n  \"/ip4/203.0.113.11/udp/40101/quic-v1/p2p/{peer_b}\"\n]"
            ),
        )
        .unwrap();

        let cli_address: Multiaddr = format!(
            "/ip4/203.0.113.12/udp/40102/quic-v1/p2p/{peer_c}"
        )
        .parse()
        .unwrap();
        let bootstrap = load_bootstrap_nodes(&path, &[cli_address]).unwrap();

        assert_eq!(bootstrap.len(), 3);
        assert!(bootstrap.iter().any(|address| address.to_string().contains(&peer_a.to_string())));
        assert!(bootstrap.iter().any(|address| address.to_string().contains(&peer_b.to_string())));
        assert!(bootstrap.iter().any(|address| address.to_string().contains(&peer_c.to_string())));

        let _ = fs::remove_file(path);
    }
}
