pub mod event;
pub mod lifecycle;
pub mod node;
pub mod topology;

use libp2p::{multiaddr::Protocol, Multiaddr};
use std::{str::FromStr, time::Duration};
use thiserror::Error;
use tokio::sync::mpsc;
use void_identity::VoidPeerId;
use void_protocol::{Envelope, VoidUri};

pub use event::{EventBus, RuntimeMeshAnnouncement, TransportEvent};
pub use lifecycle::{LifecycleEngine, LifecycleTransition, NodeLifecycleState};
pub use node::{run_transport_node, TransportNodeConfig};
pub use topology::{
    BootstrapPeerStatus, BootstrapState, ConnectionPath, DnsTopologyInfo, MeshState,
    NetworkReachability, NetworkTopologyInfo, PeerConnectionState, PeerRecord, PeerRuntimeInfo,
    PeerTopology, RuntimeShellTopologyInfo, SessionInfo, TransportHealth,
};

pub const DEFAULT_LISTEN_ADDR: &str = "/ip4/0.0.0.0/udp/0/quic-v1";
pub const VOID_IDENTIFY_PROTOCOL: &str = "/voidnet/identify/1.0.0";
pub const VOID_AGENT_VERSION: &str = "voidnet/0.1.0";
pub const RUNTIME_MESH_TOPIC: &str = "voidnet.runtime.mesh.v1";
pub const VOID_DNS_TOPIC: &str = "voidnet.dns.mesh.v1";

#[derive(Debug, Clone)]
pub struct NetworkConfig {
    pub listen: Vec<Multiaddr>,
    pub bootstrap: Vec<Multiaddr>,
    pub idle_timeout: Duration,
}

impl NetworkConfig {
    pub fn new(listen: Vec<Multiaddr>, bootstrap: Vec<Multiaddr>) -> Self {
        Self {
            listen,
            bootstrap,
            idle_timeout: Duration::from_secs(30),
        }
    }
}

impl Default for NetworkConfig {
    fn default() -> Self {
        Self::new(
            vec![Multiaddr::from_str(DEFAULT_LISTEN_ADDR).expect("valid default QUIC multiaddr")],
            Vec::new(),
        )
    }
}

#[derive(Debug, Clone)]
pub enum TransportCommand {
    Dial(Multiaddr),
    SendEnvelope { peer_id: String, envelope: Envelope },
    Publish { topic: String, envelope: Envelope },
    OpenStream { peer_id: String, uri: VoidUri },
    Shutdown,
}

#[derive(Clone)]
pub struct NetworkHandle {
    commands: mpsc::Sender<TransportCommand>,
}

impl NetworkHandle {
    pub async fn send(&self, command: TransportCommand) -> Result<(), TransportError> {
        self.commands
            .send(command)
            .await
            .map_err(|_| TransportError::CommandChannelClosed)
    }
}

pub struct NetworkInbox {
    pub commands: mpsc::Receiver<TransportCommand>,
    pub events: mpsc::Receiver<TransportEvent>,
    event_tx: mpsc::Sender<TransportEvent>,
}

impl NetworkInbox {
    pub async fn emit(&self, event: TransportEvent) -> Result<(), TransportError> {
        self.event_tx
            .send(event)
            .await
            .map_err(|_| TransportError::EventChannelClosed)
    }
}

pub fn network_channels(buffer: usize) -> (NetworkHandle, NetworkInbox) {
    let (command_tx, command_rx) = mpsc::channel(buffer);
    let (event_tx, event_rx) = mpsc::channel(buffer);

    (
        NetworkHandle {
            commands: command_tx,
        },
        NetworkInbox {
            commands: command_rx,
            events: event_rx,
            event_tx,
        },
    )
}

pub fn is_quic_address(address: &Multiaddr) -> bool {
    address
        .iter()
        .any(|protocol| matches!(protocol, Protocol::Quic | Protocol::QuicV1))
}

pub fn peer_topic(peer_id: &VoidPeerId) -> String {
    format!("void.peer.{}", peer_id.as_str())
}

#[derive(Debug, Error)]
pub enum TransportError {
    #[error("transport command channel is closed")]
    CommandChannelClosed,
    #[error("transport event channel is closed")]
    EventChannelClosed,
    #[error("address is not a QUIC multiaddr: {0}")]
    NonQuicAddress(String),
    #[error("transport backend failed: {0}")]
    Backend(String),
    #[error("transport IO failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("topology persistence failed: {0}")]
    Topology(#[from] serde_json::Error),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_quic_multiaddr() {
        let addr: Multiaddr = DEFAULT_LISTEN_ADDR.parse().unwrap();
        assert!(is_quic_address(&addr));
    }
}
