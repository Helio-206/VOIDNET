use crate::lifecycle::NodeLifecycleState;
use libp2p::{multiaddr::Protocol, Multiaddr};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fmt, fs, io,
    net::{IpAddr, Ipv4Addr, Ipv6Addr},
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PeerConnectionState {
    Discovered,
    Authenticating,
    Syncing,
    Active,
    Partitioned,
    Quarantined,
    Offline,
}

impl fmt::Display for PeerConnectionState {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            PeerConnectionState::Discovered => "DISCOVERED",
            PeerConnectionState::Authenticating => "AUTHENTICATING",
            PeerConnectionState::Syncing => "SYNCING",
            PeerConnectionState::Active => "ACTIVE",
            PeerConnectionState::Partitioned => "PARTITIONED",
            PeerConnectionState::Quarantined => "QUARANTINED",
            PeerConnectionState::Offline => "OFFLINE",
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TransportHealth {
    Unknown,
    Healthy,
    Degraded,
    Partitioned,
    Offline,
}

impl fmt::Display for TransportHealth {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            TransportHealth::Unknown => "UNKNOWN",
            TransportHealth::Healthy => "HEALTHY",
            TransportHealth::Degraded => "DEGRADED",
            TransportHealth::Partitioned => "PARTITIONED",
            TransportHealth::Offline => "OFFLINE",
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MeshState {
    Bootstrapping,
    Stable,
    Degraded,
    Partitioned,
    Recovering,
}

impl fmt::Display for MeshState {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            MeshState::Bootstrapping => "BOOTSTRAPPING",
            MeshState::Stable => "STABLE",
            MeshState::Degraded => "DEGRADED",
            MeshState::Partitioned => "PARTITIONED",
            MeshState::Recovering => "RECOVERING",
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ConnectionPath {
    #[default]
    Unknown,
    Direct,
    Relay,
}

impl fmt::Display for ConnectionPath {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            ConnectionPath::Unknown => "UNKNOWN",
            ConnectionPath::Direct => "DIRECT",
            ConnectionPath::Relay => "RELAY",
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum NetworkReachability {
    #[default]
    Unknown,
    Private,
    Public,
}

impl fmt::Display for NetworkReachability {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            NetworkReachability::Unknown => "UNKNOWN",
            NetworkReachability::Private => "PRIVATE",
            NetworkReachability::Public => "PUBLIC",
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BootstrapState {
    Configured,
    Dialing,
    Connected,
    Degraded,
}

impl fmt::Display for BootstrapState {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            BootstrapState::Configured => "CONFIGURED",
            BootstrapState::Dialing => "DIALING",
            BootstrapState::Connected => "CONNECTED",
            BootstrapState::Degraded => "DEGRADED",
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum RelayReservationState {
    #[default]
    Inactive,
    Attempting,
    Reserved,
    Failed,
}

impl fmt::Display for RelayReservationState {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            RelayReservationState::Inactive => "INACTIVE",
            RelayReservationState::Attempting => "ATTEMPTING",
            RelayReservationState::Reserved => "RESERVED",
            RelayReservationState::Failed => "FAILED",
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BootstrapPeerStatus {
    pub address: String,
    pub peer_id: Option<String>,
    pub state: BootstrapState,
    #[serde(default)]
    pub relay_reservation: RelayReservationState,
    pub reconnect_attempts: u32,
    pub last_attempt_unix_ms: Option<u128>,
    pub last_connected_unix_ms: Option<u128>,
    #[serde(default)]
    pub last_relay_reservation_attempt_unix_ms: Option<u128>,
    #[serde(default)]
    pub last_relay_reservation_success_unix_ms: Option<u128>,
    #[serde(default)]
    pub last_relay_reservation_error: Option<String>,
    pub last_error: Option<String>,
}

impl BootstrapPeerStatus {
    fn new(address: impl Into<String>) -> Self {
        let address = address.into();
        Self {
            peer_id: peer_id_from_address(&address),
            address,
            state: BootstrapState::Configured,
            relay_reservation: RelayReservationState::Inactive,
            reconnect_attempts: 0,
            last_attempt_unix_ms: None,
            last_connected_unix_ms: None,
            last_relay_reservation_attempt_unix_ms: None,
            last_relay_reservation_success_unix_ms: None,
            last_relay_reservation_error: None,
            last_error: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NetworkTopologyInfo {
    pub listen_addresses: Vec<String>,
    pub observed_addresses: Vec<String>,
    pub bootstrap_peers: Vec<BootstrapPeerStatus>,
    pub reachability: NetworkReachability,
    pub nat_detail: Option<String>,
    pub last_updated_unix_ms: u128,
}

impl Default for NetworkTopologyInfo {
    fn default() -> Self {
        Self {
            listen_addresses: Vec::new(),
            observed_addresses: Vec::new(),
            bootstrap_peers: Vec::new(),
            reachability: NetworkReachability::Unknown,
            nat_detail: None,
            last_updated_unix_ms: unix_millis(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PeerRuntimeInfo {
    pub runtime_version: String,
    pub node_state: NodeLifecycleState,
    pub uptime_secs: u64,
    pub capabilities: Vec<String>,
    pub transport_health: TransportHealth,
    pub runtime_ready: bool,
    pub last_updated_unix_ms: u128,
}

impl PeerRuntimeInfo {
    pub fn new(
        runtime_version: impl Into<String>,
        node_state: NodeLifecycleState,
        uptime_secs: u64,
        capabilities: Vec<String>,
        transport_health: TransportHealth,
        runtime_ready: bool,
    ) -> Self {
        Self {
            runtime_version: runtime_version.into(),
            node_state,
            uptime_secs,
            capabilities: normalize_capabilities(capabilities),
            transport_health,
            runtime_ready,
            last_updated_unix_ms: unix_millis(),
        }
    }

    fn refresh(
        &mut self,
        uptime_secs: u64,
        node_state: NodeLifecycleState,
        transport_health: TransportHealth,
        runtime_ready: bool,
    ) {
        self.uptime_secs = uptime_secs;
        self.node_state = node_state;
        self.transport_health = transport_health;
        self.runtime_ready = runtime_ready;
        self.last_updated_unix_ms = unix_millis();
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DnsTopologyInfo {
    pub cache_entries: usize,
    pub active_records: usize,
    pub conflicts: usize,
    pub runtime_registrations: Vec<String>,
    pub last_resolution_latency_ms: Option<u128>,
    pub last_updated_unix_ms: u128,
}

impl DnsTopologyInfo {
    pub fn new(
        cache_entries: usize,
        active_records: usize,
        conflicts: usize,
        runtime_registrations: Vec<String>,
        last_resolution_latency_ms: Option<u128>,
    ) -> Self {
        Self {
            cache_entries,
            active_records,
            conflicts,
            runtime_registrations,
            last_resolution_latency_ms,
            last_updated_unix_ms: unix_millis(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeShellTopologyInfo {
    pub mounted_surfaces: usize,
    pub active_sessions: usize,
    pub active_permissions: usize,
    pub failed_mounts: usize,
    pub registry_entries: usize,
    pub ui_surfaces: usize,
    #[serde(default)]
    pub gateway_registrations: usize,
    #[serde(default)]
    pub gateway_mounts: usize,
    #[serde(default)]
    pub gateway_active_routes: usize,
    #[serde(default)]
    pub gateway_permission_grants: usize,
    #[serde(default)]
    pub gateway_bridge_failures: usize,
    #[serde(default)]
    pub gateway_bridge_sessions: usize,
    #[serde(default)]
    pub gateway_snapshot_entries: usize,
    #[serde(default)]
    pub state_revisions: u64,
    #[serde(default)]
    pub rerender_count: u64,
    #[serde(default)]
    pub hot_reload_count: u64,
    #[serde(default)]
    pub sync_count: u64,
    #[serde(default)]
    pub permission_denials: u64,
    pub last_render_duration_ms: Option<u128>,
    pub last_action: Option<String>,
    #[serde(default)]
    pub last_error: Option<String>,
    #[serde(default)]
    pub gateway_last_route: Option<String>,
    #[serde(default)]
    pub gateway_last_external_target: Option<String>,
    #[serde(default)]
    pub gateway_last_bridge_state: Option<String>,
    #[serde(default)]
    pub gateway_last_cache_state: Option<String>,
    #[serde(default)]
    pub gateway_last_fetch_latency_ms: Option<u128>,
    #[serde(default)]
    pub gateway_last_response_size: Option<usize>,
    pub last_mount_latency_ms: Option<u128>,
    pub last_updated_unix_ms: u128,
}

impl RuntimeShellTopologyInfo {
    pub fn new(
        mounted_surfaces: usize,
        active_sessions: usize,
        active_permissions: usize,
        failed_mounts: usize,
        registry_entries: usize,
        ui_surfaces: usize,
        gateway_registrations: usize,
        gateway_mounts: usize,
        gateway_active_routes: usize,
        gateway_permission_grants: usize,
        gateway_bridge_failures: usize,
        gateway_bridge_sessions: usize,
        gateway_snapshot_entries: usize,
        state_revisions: u64,
        rerender_count: u64,
        hot_reload_count: u64,
        sync_count: u64,
        permission_denials: u64,
        last_render_duration_ms: Option<u128>,
        last_action: Option<String>,
        last_error: Option<String>,
        gateway_last_route: Option<String>,
        gateway_last_external_target: Option<String>,
        gateway_last_bridge_state: Option<String>,
        gateway_last_cache_state: Option<String>,
        gateway_last_fetch_latency_ms: Option<u128>,
        gateway_last_response_size: Option<usize>,
        last_mount_latency_ms: Option<u128>,
    ) -> Self {
        Self {
            mounted_surfaces,
            active_sessions,
            active_permissions,
            failed_mounts,
            registry_entries,
            ui_surfaces,
            gateway_registrations,
            gateway_mounts,
            gateway_active_routes,
            gateway_permission_grants,
            gateway_bridge_failures,
            gateway_bridge_sessions,
            gateway_snapshot_entries,
            state_revisions,
            rerender_count,
            hot_reload_count,
            sync_count,
            permission_denials,
            last_render_duration_ms,
            last_action,
            last_error,
            gateway_last_route,
            gateway_last_external_target,
            gateway_last_bridge_state,
            gateway_last_cache_state,
            gateway_last_fetch_latency_ms,
            gateway_last_response_size,
            last_mount_latency_ms,
            last_updated_unix_ms: unix_millis(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionInfo {
    pub session_id: Option<String>,
    pub encrypted: bool,
    pub cipher: Option<String>,
    pub established_at_unix_ms: Option<u128>,
    pub encryption_state: String,
    pub last_activity_unix_ms: Option<u128>,
    pub transport_state: String,
    #[serde(default)]
    pub connection_path: ConnectionPath,
    #[serde(default)]
    pub relay_peer_id: Option<String>,
    #[serde(default)]
    pub relay_activation_reason: Option<String>,
    #[serde(default)]
    pub relay_established_at_unix_ms: Option<u128>,
    #[serde(default)]
    pub hole_punch_attempts: u32,
    #[serde(default)]
    pub hole_punch_successes: u32,
    #[serde(default)]
    pub direct_upgrade_attempts: u32,
    pub reconnect_attempts: u32,
    pub last_error: Option<String>,
    pub last_changed_unix_ms: u128,
}

impl Default for SessionInfo {
    fn default() -> Self {
        Self {
            session_id: None,
            encrypted: false,
            cipher: None,
            established_at_unix_ms: None,
            encryption_state: "IDLE".to_string(),
            last_activity_unix_ms: None,
            transport_state: "UNKNOWN".to_string(),
            connection_path: ConnectionPath::Unknown,
            relay_peer_id: None,
            relay_activation_reason: None,
            relay_established_at_unix_ms: None,
            hole_punch_attempts: 0,
            hole_punch_successes: 0,
            direct_upgrade_attempts: 0,
            reconnect_attempts: 0,
            last_error: None,
            last_changed_unix_ms: unix_millis(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerRecord {
    pub peer_id: String,
    pub addresses: Vec<String>,
    pub state: PeerConnectionState,
    pub latency_ms: Option<u128>,
    pub transport: Option<String>,
    pub transport_health: TransportHealth,
    pub runtime: Option<PeerRuntimeInfo>,
    pub session: SessionInfo,
    pub failures: u32,
    pub last_seen_unix_ms: u128,
}

impl PeerRecord {
    fn new(peer_id: impl Into<String>) -> Self {
        Self {
            peer_id: peer_id.into(),
            addresses: Vec::new(),
            state: PeerConnectionState::Discovered,
            latency_ms: None,
            transport: None,
            transport_health: TransportHealth::Unknown,
            runtime: None,
            session: SessionInfo::default(),
            failures: 0,
            last_seen_unix_ms: unix_millis(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerTopology {
    pub local_peer_id: String,
    pub local_runtime: Option<PeerRuntimeInfo>,
    pub dns: Option<DnsTopologyInfo>,
    pub runtime_shell: Option<RuntimeShellTopologyInfo>,
    #[serde(default)]
    pub network: Option<NetworkTopologyInfo>,
    pub mesh_state: MeshState,
    pub peers: BTreeMap<String, PeerRecord>,
    pub updated_unix_ms: u128,
}

impl PeerTopology {
    pub fn new(local_peer_id: impl Into<String>) -> Self {
        Self {
            local_peer_id: local_peer_id.into(),
            local_runtime: None,
            dns: None,
            runtime_shell: None,
            network: None,
            mesh_state: MeshState::Bootstrapping,
            peers: BTreeMap::new(),
            updated_unix_ms: unix_millis(),
        }
    }

    pub fn load(path: impl AsRef<Path>) -> Result<Self, TopologyError> {
        let raw = fs::read_to_string(path)?;
        Ok(serde_json::from_str(&raw)?)
    }

    pub fn save(&self, path: impl AsRef<Path>) -> Result<(), TopologyError> {
        if let Some(parent) = path.as_ref().parent() {
            fs::create_dir_all(parent)?;
        }

        let raw = serde_json::to_string_pretty(self)?;
        fs::write(path, raw)?;
        Ok(())
    }

    pub fn observe_discovered(&mut self, peer_id: impl Into<String>, addresses: Vec<String>) {
        let peer_id = peer_id.into();
        let record = self
            .peers
            .entry(peer_id.clone())
            .or_insert_with(|| PeerRecord::new(peer_id));
        merge_addresses(&mut record.addresses, addresses);
        record.state = PeerConnectionState::Discovered;
        if record.transport_health == TransportHealth::Unknown {
            record.transport_health = TransportHealth::Degraded;
        }
        record.last_seen_unix_ms = unix_millis();
        self.updated_unix_ms = unix_millis();
    }

    pub fn observe_connected(
        &mut self,
        peer_id: impl Into<String>,
        address: Option<String>,
        transport: impl Into<String>,
    ) -> ConnectionPath {
        let peer_id = peer_id.into();
        let now = unix_millis();
        let connection_path = infer_connection_path(address.as_deref());
        let record = self
            .peers
            .entry(peer_id.clone())
            .or_insert_with(|| PeerRecord::new(peer_id));
        if let Some(address) = address {
            merge_addresses(&mut record.addresses, vec![address]);
        }
        record.transport = Some(transport.into());
        record.state = PeerConnectionState::Authenticating;
        record.transport_health = TransportHealth::Healthy;
        record.session.transport_state = "CONNECTED".to_string();
        record.session.connection_path = connection_path;
        if connection_path == ConnectionPath::Direct {
            clear_relay_session(&mut record.session);
        }
        record.session.last_error = None;
        record.session.last_changed_unix_ms = now;
        record.last_seen_unix_ms = now;
        self.updated_unix_ms = now;
        connection_path
    }

    pub fn observe_authenticated(&mut self, peer_id: impl Into<String>) {
        let peer_id = peer_id.into();
        self.set_state(peer_id.clone(), PeerConnectionState::Syncing);
        if let Some(record) = self.peers.get_mut(&peer_id) {
            if let Some(runtime) = record.runtime.as_mut() {
                runtime.node_state = NodeLifecycleState::Syncing;
                runtime.runtime_ready = false;
                runtime.transport_health = record.transport_health;
                runtime.last_updated_unix_ms = unix_millis();
            }
        }
    }

    pub fn observe_active(&mut self, peer_id: impl Into<String>) {
        let peer_id = peer_id.into();
        self.set_state(peer_id.clone(), PeerConnectionState::Active);
        if let Some(record) = self.peers.get_mut(&peer_id) {
            record.transport_health = TransportHealth::Healthy;
            if let Some(runtime) = record.runtime.as_mut() {
                runtime.node_state = NodeLifecycleState::Active;
                runtime.runtime_ready = true;
                runtime.transport_health = record.transport_health;
                runtime.last_updated_unix_ms = unix_millis();
            }
        }
    }

    pub fn observe_latency(&mut self, peer_id: impl Into<String>, latency_ms: u128) {
        let peer_id = peer_id.into();
        let record = self
            .peers
            .entry(peer_id.clone())
            .or_insert_with(|| PeerRecord::new(peer_id));
        record.latency_ms = Some(latency_ms);
        record.transport_health = TransportHealth::Healthy;
        record.last_seen_unix_ms = unix_millis();
        if matches!(record.state, PeerConnectionState::Syncing) {
            record.state = PeerConnectionState::Active;
        }
        if let Some(runtime) = record.runtime.as_mut() {
            runtime.node_state = NodeLifecycleState::Active;
            runtime.runtime_ready = true;
            runtime.transport_health = record.transport_health;
            runtime.last_updated_unix_ms = unix_millis();
        }
        self.updated_unix_ms = unix_millis();
    }

    pub fn observe_disconnected(&mut self, peer_id: impl Into<String>) {
        let peer_id = peer_id.into();
        self.set_state(peer_id.clone(), PeerConnectionState::Offline);
        if let Some(record) = self.peers.get_mut(&peer_id) {
            record.transport_health = TransportHealth::Offline;
            record.session.encrypted = false;
            record.session.cipher = None;
            record.session.encryption_state = "OFFLINE".to_string();
            record.session.transport_state = "OFFLINE".to_string();
            record.session.connection_path = ConnectionPath::Unknown;
            clear_relay_session(&mut record.session);
            record.session.last_changed_unix_ms = unix_millis();
            if let Some(runtime) = record.runtime.as_mut() {
                runtime.node_state = NodeLifecycleState::Offline;
                runtime.runtime_ready = false;
                runtime.transport_health = record.transport_health;
                runtime.last_updated_unix_ms = unix_millis();
            }
        }
    }

    pub fn observe_failure(&mut self, peer_id: Option<String>, error: impl Into<String>) {
        let error = error.into();
        if let Some(peer_id) = peer_id {
            let record = self
                .peers
                .entry(peer_id.clone())
                .or_insert_with(|| PeerRecord::new(peer_id));
            record.failures = record.failures.saturating_add(1);
            record.transport_health = TransportHealth::Degraded;
            record.session.reconnect_attempts = record.session.reconnect_attempts.saturating_add(1);
            record.session.transport_state = "DEGRADED".to_string();
            record.session.encryption_state = "FAILED".to_string();
            record.session.last_error = Some(error);
            record.session.last_changed_unix_ms = unix_millis();
            if record.failures >= 3 {
                record.state = PeerConnectionState::Quarantined;
            }
            if let Some(runtime) = record.runtime.as_mut() {
                runtime.transport_health = record.transport_health;
                runtime.node_state = if record.state == PeerConnectionState::Quarantined {
                    NodeLifecycleState::Quarantined
                } else {
                    NodeLifecycleState::Discovering
                };
                runtime.runtime_ready = false;
                runtime.last_updated_unix_ms = unix_millis();
            }
            record.last_seen_unix_ms = unix_millis();
            self.updated_unix_ms = unix_millis();
        }
    }

    pub fn observe_runtime_state(&mut self, peer_id: impl Into<String>, runtime: PeerRuntimeInfo) {
        let peer_id = peer_id.into();
        let record = self
            .peers
            .entry(peer_id.clone())
            .or_insert_with(|| PeerRecord::new(peer_id));
        record.transport_health = runtime.transport_health;
        record.runtime = Some(runtime);
        record.last_seen_unix_ms = unix_millis();
        self.updated_unix_ms = unix_millis();
    }

    pub fn observe_transport_encryption(
        &mut self,
        peer_id: impl Into<String>,
        cipher: impl Into<String>,
    ) {
        let peer_id = peer_id.into();
        let record = self
            .peers
            .entry(peer_id.clone())
            .or_insert_with(|| PeerRecord::new(peer_id));
        record.session.encrypted = true;
        record.session.cipher = Some(cipher.into());
        record.session.encryption_state = "TRANSPORT-SECURE".to_string();
        record.session.transport_state = record
            .transport
            .clone()
            .unwrap_or_else(|| "QUIC".to_string());
        record.session.last_activity_unix_ms = Some(unix_millis());
        record.session.last_changed_unix_ms = unix_millis();
        record.transport_health = TransportHealth::Healthy;
        record.last_seen_unix_ms = unix_millis();
        self.updated_unix_ms = unix_millis();
    }

    pub fn observe_session_negotiating(
        &mut self,
        peer_id: impl Into<String>,
        session_id: impl Into<String>,
    ) {
        let peer_id = peer_id.into();
        let record = self
            .peers
            .entry(peer_id.clone())
            .or_insert_with(|| PeerRecord::new(peer_id));
        let now = unix_millis();
        record.session.session_id = Some(session_id.into());
        record.session.encryption_state = "NEGOTIATING".to_string();
        record.session.transport_state = "GOSSIPSUB-DIRECT".to_string();
        record.session.connection_path = ConnectionPath::Direct;
        record.session.last_activity_unix_ms = Some(now);
        record.session.last_changed_unix_ms = now;
        record.last_seen_unix_ms = now;
        self.updated_unix_ms = now;
    }

    pub fn observe_encrypted_session(
        &mut self,
        peer_id: impl Into<String>,
        session_id: Option<String>,
        cipher: impl Into<String>,
    ) {
        let peer_id = peer_id.into();
        let record = self
            .peers
            .entry(peer_id.clone())
            .or_insert_with(|| PeerRecord::new(peer_id));
        let now = unix_millis();
        record.session.encrypted = true;
        record.session.cipher = Some(cipher.into());
        if session_id.is_some() {
            record.session.session_id = session_id;
        }
        if record.session.established_at_unix_ms.is_none() {
            record.session.established_at_unix_ms = Some(now);
        }
        record.session.encryption_state = "ESTABLISHED".to_string();
        record.session.transport_state = "GOSSIPSUB-DIRECT".to_string();
        record.session.connection_path = ConnectionPath::Direct;
        record.session.last_activity_unix_ms = Some(now);
        record.session.last_error = None;
        record.session.last_changed_unix_ms = now;
        record.transport_health = TransportHealth::Healthy;
        record.last_seen_unix_ms = now;
        self.updated_unix_ms = now;
    }

    pub fn observe_session_activity(&mut self, peer_id: impl Into<String>) {
        let peer_id = peer_id.into();
        let now = unix_millis();
        let record = self
            .peers
            .entry(peer_id.clone())
            .or_insert_with(|| PeerRecord::new(peer_id));
        record.session.last_activity_unix_ms = Some(now);
        record.session.last_changed_unix_ms = now;
        record.last_seen_unix_ms = now;
        self.updated_unix_ms = now;
    }

    pub fn observe_session_failure(
        &mut self,
        peer_id: impl Into<String>,
        error: impl Into<String>,
    ) {
        let peer_id = peer_id.into();
        let record = self
            .peers
            .entry(peer_id.clone())
            .or_insert_with(|| PeerRecord::new(peer_id));
        record.session.encryption_state = "FAILED".to_string();
        record.session.last_error = Some(error.into());
        record.session.last_changed_unix_ms = unix_millis();
        record.last_seen_unix_ms = unix_millis();
        self.updated_unix_ms = unix_millis();
    }

    pub fn set_local_runtime(&mut self, runtime: PeerRuntimeInfo) {
        self.local_runtime = Some(runtime);
        self.updated_unix_ms = unix_millis();
    }

    pub fn set_dns_state(&mut self, dns: DnsTopologyInfo) {
        self.dns = Some(dns);
        self.updated_unix_ms = unix_millis();
    }

    pub fn set_runtime_shell_state(&mut self, runtime_shell: RuntimeShellTopologyInfo) {
        self.runtime_shell = Some(runtime_shell);
        self.updated_unix_ms = unix_millis();
    }

    pub fn set_listen_addresses(&mut self, listen_addresses: Vec<String>) {
        let network = self.network.get_or_insert_with(NetworkTopologyInfo::default);
        network.listen_addresses = dedupe_strings(listen_addresses);
        network.reachability = infer_reachability(
            &network.listen_addresses,
            &network.observed_addresses,
        );
        network.last_updated_unix_ms = unix_millis();
        self.updated_unix_ms = unix_millis();
    }

    pub fn configure_bootstrap(&mut self, bootstrap_addresses: Vec<String>) {
        let network = self.network.get_or_insert_with(NetworkTopologyInfo::default);
        network.bootstrap_peers = dedupe_strings(bootstrap_addresses)
            .into_iter()
            .map(BootstrapPeerStatus::new)
            .collect();
        network.last_updated_unix_ms = unix_millis();
        self.updated_unix_ms = unix_millis();
    }

    pub fn observe_bootstrap_dial(&mut self, address: impl Into<String>) -> u32 {
        let address = address.into();
        let network = self.network.get_or_insert_with(NetworkTopologyInfo::default);
        let record = bootstrap_record_mut(&mut network.bootstrap_peers, None, Some(address.as_str()));
        let attempt = if let Some(record) = record {
            record.reconnect_attempts = record.reconnect_attempts.saturating_add(1);
            record.state = BootstrapState::Dialing;
            record.last_attempt_unix_ms = Some(unix_millis());
            record.last_error = None;
            record.reconnect_attempts
        } else {
            network.bootstrap_peers.push(BootstrapPeerStatus::new(address));
            let record = network.bootstrap_peers.last_mut().expect("bootstrap peer inserted");
            record.reconnect_attempts = 1;
            record.state = BootstrapState::Dialing;
            record.last_attempt_unix_ms = Some(unix_millis());
            record.reconnect_attempts
        };
        network.last_updated_unix_ms = unix_millis();
        self.updated_unix_ms = unix_millis();
        attempt
    }

    pub fn observe_bootstrap_connected(
        &mut self,
        peer_id: impl AsRef<str>,
        address: Option<&str>,
    ) -> bool {
        let Some(network) = self.network.as_mut() else {
            return false;
        };
        let Some(record) = bootstrap_record_mut(
            &mut network.bootstrap_peers,
            Some(peer_id.as_ref()),
            address,
        ) else {
            return false;
        };
        record.peer_id.get_or_insert_with(|| peer_id.as_ref().to_string());
        record.state = BootstrapState::Connected;
        record.last_connected_unix_ms = Some(unix_millis());
        record.last_error = None;
        network.last_updated_unix_ms = unix_millis();
        self.updated_unix_ms = unix_millis();
        true
    }

    pub fn observe_relay_reservation_attempt(
        &mut self,
        peer_id: Option<&str>,
        address: Option<&str>,
    ) -> bool {
        let Some(network) = self.network.as_mut() else {
            return false;
        };
        let Some(record) = bootstrap_record_mut(&mut network.bootstrap_peers, peer_id, address) else {
            return false;
        };
        record.relay_reservation = RelayReservationState::Attempting;
        record.last_relay_reservation_attempt_unix_ms = Some(unix_millis());
        record.last_relay_reservation_error = None;
        network.last_updated_unix_ms = unix_millis();
        self.updated_unix_ms = unix_millis();
        true
    }

    pub fn observe_relay_reservation_accepted(
        &mut self,
        peer_id: Option<&str>,
        address: Option<&str>,
    ) -> bool {
        let Some(network) = self.network.as_mut() else {
            return false;
        };
        let Some(record) = bootstrap_record_mut(&mut network.bootstrap_peers, peer_id, address) else {
            return false;
        };
        record.relay_reservation = RelayReservationState::Reserved;
        record.last_relay_reservation_success_unix_ms = Some(unix_millis());
        record.last_relay_reservation_error = None;
        network.last_updated_unix_ms = unix_millis();
        self.updated_unix_ms = unix_millis();
        true
    }

    pub fn observe_relay_reservation_failed(
        &mut self,
        peer_id: Option<&str>,
        address: Option<&str>,
        error: impl Into<String>,
    ) -> bool {
        let Some(network) = self.network.as_mut() else {
            return false;
        };
        let Some(record) = bootstrap_record_mut(&mut network.bootstrap_peers, peer_id, address) else {
            return false;
        };
        record.relay_reservation = RelayReservationState::Failed;
        record.last_relay_reservation_error = Some(error.into());
        if record.last_relay_reservation_attempt_unix_ms.is_none() {
            record.last_relay_reservation_attempt_unix_ms = Some(unix_millis());
        }
        network.last_updated_unix_ms = unix_millis();
        self.updated_unix_ms = unix_millis();
        true
    }

    pub fn observe_bootstrap_failure(
        &mut self,
        peer_id: Option<&str>,
        address: Option<&str>,
        error: impl Into<String>,
    ) -> bool {
        let Some(network) = self.network.as_mut() else {
            return false;
        };
        let Some(record) = bootstrap_record_mut(&mut network.bootstrap_peers, peer_id, address) else {
            return false;
        };
        record.state = BootstrapState::Degraded;
        record.last_error = Some(error.into());
        if record.last_attempt_unix_ms.is_none() {
            record.last_attempt_unix_ms = Some(unix_millis());
        }
        network.last_updated_unix_ms = unix_millis();
        self.updated_unix_ms = unix_millis();
        true
    }

    pub fn observe_observed_address(&mut self, address: impl Into<String>) -> Option<NetworkReachability> {
        let address = address.into();
        if address.is_empty() {
            return None;
        }
        let network = self.network.get_or_insert_with(NetworkTopologyInfo::default);
        let previous = network.reachability;
        merge_addresses(&mut network.observed_addresses, vec![address]);
        network.reachability = infer_reachability(
            &network.listen_addresses,
            &network.observed_addresses,
        );
        network.last_updated_unix_ms = unix_millis();
        self.updated_unix_ms = unix_millis();
        if network.reachability != previous {
            Some(network.reachability)
        } else {
            None
        }
    }

    pub fn observe_nat_status(
        &mut self,
        status: impl Into<String>,
        public_address: Option<String>,
        confidence: usize,
    ) -> Option<NetworkReachability> {
        let status = status.into();
        let network = self.network.get_or_insert_with(NetworkTopologyInfo::default);
        let previous = network.reachability;
        if let Some(address) = public_address.clone() {
            merge_addresses(&mut network.observed_addresses, vec![address]);
        }
        network.nat_detail = Some(match public_address {
            Some(address) => format!("status={status} public_address={address} confidence={confidence}"),
            None => format!("status={status} confidence={confidence}"),
        });
        network.reachability = match status.as_str() {
            "Public" => NetworkReachability::Public,
            "Private" => NetworkReachability::Private,
            _ => infer_reachability(&network.listen_addresses, &network.observed_addresses),
        };
        network.last_updated_unix_ms = unix_millis();
        self.updated_unix_ms = unix_millis();
        if network.reachability != previous {
            Some(network.reachability)
        } else {
            None
        }
    }

    pub fn observe_relay_session(
        &mut self,
        peer_id: impl Into<String>,
        relay_peer_id: Option<String>,
        reason: impl Into<String>,
    ) {
        let peer_id = peer_id.into();
        let now = unix_millis();
        let record = self
            .peers
            .entry(peer_id.clone())
            .or_insert_with(|| PeerRecord::new(peer_id));
        record.session.connection_path = ConnectionPath::Relay;
        record.session.transport_state = "RELAY-CIRCUIT".to_string();
        record.session.relay_peer_id = relay_peer_id;
        record.session.relay_activation_reason = Some(reason.into());
        record.session.relay_established_at_unix_ms.get_or_insert(now);
        record.session.last_changed_unix_ms = now;
        record.last_seen_unix_ms = now;
        self.updated_unix_ms = now;
    }

    pub fn observe_hole_punch_attempt(&mut self, peer_id: impl Into<String>) {
        let peer_id = peer_id.into();
        let now = unix_millis();
        let record = self
            .peers
            .entry(peer_id.clone())
            .or_insert_with(|| PeerRecord::new(peer_id));
        record.session.hole_punch_attempts = record.session.hole_punch_attempts.saturating_add(1);
        record.session.direct_upgrade_attempts = record.session.direct_upgrade_attempts.saturating_add(1);
        record.session.last_changed_unix_ms = now;
        record.last_seen_unix_ms = now;
        self.updated_unix_ms = now;
    }

    pub fn observe_hole_punch_succeeded(&mut self, peer_id: impl Into<String>) {
        let peer_id = peer_id.into();
        let now = unix_millis();
        let record = self
            .peers
            .entry(peer_id.clone())
            .or_insert_with(|| PeerRecord::new(peer_id));
        record.session.hole_punch_successes = record.session.hole_punch_successes.saturating_add(1);
        record.session.last_error = None;
        record.session.last_changed_unix_ms = now;
        record.last_seen_unix_ms = now;
        self.updated_unix_ms = now;
    }

    pub fn observe_hole_punch_failed(
        &mut self,
        peer_id: impl Into<String>,
        error: impl Into<String>,
    ) {
        let peer_id = peer_id.into();
        let now = unix_millis();
        let record = self
            .peers
            .entry(peer_id.clone())
            .or_insert_with(|| PeerRecord::new(peer_id));
        record.session.last_error = Some(error.into());
        record.session.last_changed_unix_ms = now;
        record.last_seen_unix_ms = now;
        self.updated_unix_ms = now;
    }

    pub fn network_reachability(&self) -> NetworkReachability {
        self.network
            .as_ref()
            .map(|network| network.reachability)
            .unwrap_or(NetworkReachability::Unknown)
    }

    pub fn refresh_local_runtime(
        &mut self,
        uptime_secs: u64,
        node_state: NodeLifecycleState,
        transport_health: TransportHealth,
        runtime_ready: bool,
    ) {
        if let Some(runtime) = self.local_runtime.as_mut() {
            runtime.refresh(uptime_secs, node_state, transport_health, runtime_ready);
            self.updated_unix_ms = unix_millis();
        }
    }

    pub fn set_mesh_state(&mut self, state: MeshState) -> bool {
        if self.mesh_state == state {
            return false;
        }

        self.mesh_state = state;
        self.updated_unix_ms = unix_millis();
        true
    }

    pub fn mark_partitioned(&mut self) {
        self.mesh_state = MeshState::Partitioned;
        for record in self.peers.values_mut() {
            if record.state == PeerConnectionState::Quarantined {
                continue;
            }
            record.state = PeerConnectionState::Partitioned;
            record.transport_health = TransportHealth::Partitioned;
            if let Some(runtime) = record.runtime.as_mut() {
                runtime.node_state = NodeLifecycleState::Partitioned;
                runtime.runtime_ready = false;
                runtime.transport_health = TransportHealth::Partitioned;
                runtime.last_updated_unix_ms = unix_millis();
            }
        }
        self.updated_unix_ms = unix_millis();
    }

    pub fn active_peer_count(&self) -> usize {
        self.peers
            .values()
            .filter(|peer| peer.state == PeerConnectionState::Active)
            .count()
    }

    pub fn known_peer_count(&self) -> usize {
        self.peers.len()
    }

    pub fn encrypted_session_count(&self) -> usize {
        self.peers
            .values()
            .filter(|peer| peer.session.encrypted)
            .count()
    }

    pub fn runtime_ready_peer_count(&self) -> usize {
        self.peers
            .values()
            .filter(|peer| {
                peer.runtime
                    .as_ref()
                    .map(|runtime| runtime.runtime_ready)
                    .unwrap_or(false)
            })
            .count()
    }

    pub fn connected_addresses(&self) -> Vec<String> {
        self.peers
            .values()
            .filter(|peer| peer.state != PeerConnectionState::Offline)
            .flat_map(|peer| peer.addresses.iter().cloned())
            .collect()
    }

    pub fn render_table(&self) -> String {
        let mut lines = vec!["PEER ID                                            STATE           LATENCY   HEALTH       READY  RUNTIME".to_string()];
        for peer in self.peers.values() {
            let latency = peer
                .latency_ms
                .map(|latency| format!("{latency}ms"))
                .unwrap_or_else(|| "-".to_string());
            let ready = peer
                .runtime
                .as_ref()
                .map(|runtime| if runtime.runtime_ready { "yes" } else { "no" })
                .unwrap_or("-");
            let runtime_version = peer
                .runtime
                .as_ref()
                .map(|runtime| runtime.runtime_version.as_str())
                .unwrap_or("-");
            lines.push(format!(
                "{:<50} {:<15} {:<9} {:<12} {:<6} {}",
                peer.peer_id, peer.state, latency, peer.transport_health, ready, runtime_version
            ));
        }

        if self.peers.is_empty() {
            lines.push("no peers observed".to_string());
        }

        lines.join("\n")
    }

    pub fn render_ascii(&self) -> String {
        let mut lines = vec![format!("{} [{}]", self.local_peer_id, self.mesh_state)];
        if let Some(runtime) = &self.local_runtime {
            lines.push(format!(
                "  runtime={} state={} uptime={}s ready={} health={}",
                runtime.runtime_version,
                runtime.node_state,
                runtime.uptime_secs,
                if runtime.runtime_ready { "yes" } else { "no" },
                runtime.transport_health,
            ));
        }
        if let Some(dns) = &self.dns {
            lines.push(format!(
                "  dns cache={} active={} conflicts={} route_latency={}ms registrations={}",
                dns.cache_entries,
                dns.active_records,
                dns.conflicts,
                dns.last_resolution_latency_ms.unwrap_or_default(),
                if dns.runtime_registrations.is_empty() {
                    "-".to_string()
                } else {
                    dns.runtime_registrations.join(",")
                }
            ));
        }
        if let Some(runtime_shell) = &self.runtime_shell {
            lines.push(format!(
                "  shell mounts={} sessions={} permissions={} failed_mounts={} registry={} ui_surfaces={} gateways={} gateway_mounts={} gateway_routes={} gateway_permission_grants={} gateway_bridge_failures={} gateway_bridge_sessions={} gateway_snapshots={} revisions={} rerenders={} hot_reloads={} syncs={} denials={} mount_latency={}ms render_latency={}ms last_action={} last_error={} gateway_last_route={} gateway_target={} gateway_bridge_state={} gateway_cache_state={} gateway_fetch_latency={}ms gateway_response_size={}",
                runtime_shell.mounted_surfaces,
                runtime_shell.active_sessions,
                runtime_shell.active_permissions,
                runtime_shell.failed_mounts,
                runtime_shell.registry_entries,
                runtime_shell.ui_surfaces,
                runtime_shell.gateway_registrations,
                runtime_shell.gateway_mounts,
                runtime_shell.gateway_active_routes,
                runtime_shell.gateway_permission_grants,
                runtime_shell.gateway_bridge_failures,
                runtime_shell.gateway_bridge_sessions,
                runtime_shell.gateway_snapshot_entries,
                runtime_shell.state_revisions,
                runtime_shell.rerender_count,
                runtime_shell.hot_reload_count,
                runtime_shell.sync_count,
                runtime_shell.permission_denials,
                runtime_shell.last_mount_latency_ms.unwrap_or_default(),
                runtime_shell.last_render_duration_ms.unwrap_or_default(),
                runtime_shell.last_action.as_deref().unwrap_or("-"),
                runtime_shell.last_error.as_deref().unwrap_or("-"),
                runtime_shell.gateway_last_route.as_deref().unwrap_or("-"),
                runtime_shell.gateway_last_external_target.as_deref().unwrap_or("-"),
                runtime_shell.gateway_last_bridge_state.as_deref().unwrap_or("-"),
                runtime_shell.gateway_last_cache_state.as_deref().unwrap_or("-"),
                runtime_shell.gateway_last_fetch_latency_ms.unwrap_or_default(),
                runtime_shell.gateway_last_response_size.unwrap_or_default(),
            ));
        }

        if self.peers.is_empty() {
            lines.push("  no observed peers".to_string());
            return lines.join("\n");
        }

        for peer in self.peers.values() {
            let edge = match peer.state {
                PeerConnectionState::Active => "----",
                PeerConnectionState::Syncing | PeerConnectionState::Authenticating => "..->",
                PeerConnectionState::Partitioned => "-x-",
                PeerConnectionState::Quarantined => "-!-",
                PeerConnectionState::Discovered | PeerConnectionState::Offline => "....",
            };
            let runtime_state = peer
                .runtime
                .as_ref()
                .map(|runtime| runtime.node_state.to_string())
                .unwrap_or_else(|| "UNKNOWN".to_string());
            lines.push(format!(
                "  {edge} {} [{}] health={} runtime={} enc={}",
                peer.peer_id,
                peer.state,
                peer.transport_health,
                runtime_state,
                if peer.session.encrypted { "yes" } else { "no" },
            ));
        }
        lines.join("\n")
    }

    pub fn render_runtime(&self) -> String {
        let mut lines = vec![format!("local_peer={}", self.local_peer_id)];
        if let Some(runtime) = &self.local_runtime {
            lines.extend(render_runtime_lines("local", runtime));
        } else {
            lines.push("local.runtime=unavailable".to_string());
        }

        for peer in self.peers.values() {
            if let Some(runtime) = &peer.runtime {
                lines.extend(render_runtime_lines(&peer.peer_id, runtime));
            }
        }

        lines.join("\n")
    }

    pub fn render_sessions(&self) -> String {
        let mut lines = vec!["PEER ID                                            SESSION ID           STATE         LAST ACTIVITY   TRANSPORT         PATH     CIPHER               LAST ERROR".to_string()];
        for peer in self.peers.values() {
            lines.push(format!(
                "{:<50} {:<20} {:<13} {:<15} {:<17} {:<8} {:<20} {}",
                peer.peer_id,
                peer.session.session_id.as_deref().unwrap_or("-"),
                peer.session.encryption_state,
                peer.session
                    .last_activity_unix_ms
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "-".to_string()),
                peer.session.transport_state,
                peer.session.connection_path,
                peer.session.cipher.as_deref().unwrap_or("-"),
                peer.session.last_error.as_deref().unwrap_or("-"),
            ));
        }

        if self.peers.is_empty() {
            lines.push("no peer sessions observed".to_string());
        }

        lines.join("\n")
    }

    pub fn render_diagnostics(&self) -> String {
        let active = self.active_peer_count();
        let encrypted = self.encrypted_session_count();
        let ready = self.runtime_ready_peer_count();
        let degraded = self
            .peers
            .values()
            .filter(|peer| peer.transport_health == TransportHealth::Degraded)
            .count();

        let mut lines = vec![
            format!("local_peer={}", self.local_peer_id),
            format!("mesh_state={}", self.mesh_state),
            format!("known_peers={}", self.known_peer_count()),
            format!("active_peers={active}"),
            format!("runtime_ready_peers={ready}"),
            format!("encrypted_sessions={encrypted}"),
            format!("degraded_peers={degraded}"),
            format!("direct_connections={}", self.direct_connection_count()),
            format!("relay_connections={}", self.relay_connection_count()),
            format!("hole_punch_attempts={}", self.hole_punch_attempt_count()),
            format!("hole_punch_successes={}", self.hole_punch_success_count()),
            format!("updated_unix_ms={}", self.updated_unix_ms),
        ];

        if let Some(network) = &self.network {
            lines.push(format!("network_reachability={}", network.reachability));
            lines.push(format!(
                "network_listen_addresses={}",
                if network.listen_addresses.is_empty() {
                    "-".to_string()
                } else {
                    network.listen_addresses.join(",")
                }
            ));
            lines.push(format!(
                "network_observed_addresses={}",
                if network.observed_addresses.is_empty() {
                    "-".to_string()
                } else {
                    network.observed_addresses.join(",")
                }
            ));
            lines.push(format!(
                "network_bootstrap_configured={}",
                network.bootstrap_peers.len()
            ));
            lines.push(format!(
                "network_bootstrap_connected={}",
                self.connected_bootstrap_count()
            ));
            lines.push(format!(
                "network_bootstrap_degraded={}",
                self.degraded_bootstrap_count()
            ));
            if let Some(detail) = &network.nat_detail {
                lines.push(format!("network_nat_detail={detail}"));
            }
        }

        if let Some(runtime) = &self.local_runtime {
            lines.push(format!("local_runtime_version={}", runtime.runtime_version));
            lines.push(format!("local_runtime_state={}", runtime.node_state));
            lines.push(format!("local_runtime_ready={}", runtime.runtime_ready));
            lines.push(format!("local_runtime_uptime_secs={}", runtime.uptime_secs));
            lines.push(format!(
                "local_transport_health={}",
                runtime.transport_health
            ));
        }
        if let Some(dns) = &self.dns {
            lines.push(format!("dns_cache_entries={}", dns.cache_entries));
            lines.push(format!("dns_active_records={}", dns.active_records));
            lines.push(format!("dns_conflicts={}", dns.conflicts));
            lines.push(format!(
                "dns_runtime_registrations={}",
                if dns.runtime_registrations.is_empty() {
                    "-".to_string()
                } else {
                    dns.runtime_registrations.join(",")
                }
            ));
            if let Some(latency_ms) = dns.last_resolution_latency_ms {
                lines.push(format!("dns_resolution_latency_ms={latency_ms}"));
            }
        }
        if let Some(runtime_shell) = &self.runtime_shell {
            lines.push(format!(
                "runtime_shell_mounts={}",
                runtime_shell.mounted_surfaces
            ));
            lines.push(format!(
                "runtime_shell_sessions={}",
                runtime_shell.active_sessions
            ));
            lines.push(format!(
                "runtime_shell_permissions={}",
                runtime_shell.active_permissions
            ));
            lines.push(format!(
                "runtime_shell_failed_mounts={}",
                runtime_shell.failed_mounts
            ));
            lines.push(format!(
                "runtime_shell_registry_entries={}",
                runtime_shell.registry_entries
            ));
            lines.push(format!(
                "runtime_shell_ui_surfaces={}",
                runtime_shell.ui_surfaces
            ));
            lines.push(format!(
                "runtime_shell_gateway_registrations={}",
                runtime_shell.gateway_registrations
            ));
            lines.push(format!(
                "runtime_shell_gateway_mounts={}",
                runtime_shell.gateway_mounts
            ));
            lines.push(format!(
                "runtime_shell_gateway_active_routes={}",
                runtime_shell.gateway_active_routes
            ));
            lines.push(format!(
                "runtime_shell_gateway_permission_grants={}",
                runtime_shell.gateway_permission_grants
            ));
            lines.push(format!(
                "runtime_shell_gateway_bridge_failures={}",
                runtime_shell.gateway_bridge_failures
            ));
            lines.push(format!(
                "runtime_shell_gateway_bridge_sessions={}",
                runtime_shell.gateway_bridge_sessions
            ));
            lines.push(format!(
                "runtime_shell_gateway_snapshot_entries={}",
                runtime_shell.gateway_snapshot_entries
            ));
            lines.push(format!(
                "runtime_shell_state_revisions={}",
                runtime_shell.state_revisions
            ));
            lines.push(format!(
                "runtime_shell_rerender_count={}",
                runtime_shell.rerender_count
            ));
            lines.push(format!(
                "runtime_shell_hot_reload_count={}",
                runtime_shell.hot_reload_count
            ));
            lines.push(format!(
                "runtime_shell_sync_count={}",
                runtime_shell.sync_count
            ));
            lines.push(format!(
                "runtime_shell_permission_denials={}",
                runtime_shell.permission_denials
            ));
            if let Some(latency_ms) = runtime_shell.last_mount_latency_ms {
                lines.push(format!("runtime_shell_mount_latency_ms={latency_ms}"));
            }
            if let Some(render_ms) = runtime_shell.last_render_duration_ms {
                lines.push(format!("runtime_shell_render_latency_ms={render_ms}"));
            }
            if let Some(action) = &runtime_shell.last_action {
                lines.push(format!("runtime_shell_last_action={action}"));
            }
            if let Some(error) = &runtime_shell.last_error {
                lines.push(format!("runtime_shell_last_error={error}"));
            }
            if let Some(route) = &runtime_shell.gateway_last_route {
                lines.push(format!("runtime_shell_gateway_last_route={route}"));
            }
            if let Some(target) = &runtime_shell.gateway_last_external_target {
                lines.push(format!(
                    "runtime_shell_gateway_last_external_target={target}"
                ));
            }
            if let Some(state) = &runtime_shell.gateway_last_bridge_state {
                lines.push(format!("runtime_shell_gateway_last_bridge_state={state}"));
            }
            if let Some(cache_state) = &runtime_shell.gateway_last_cache_state {
                lines.push(format!(
                    "runtime_shell_gateway_last_cache_state={cache_state}"
                ));
            }
            if let Some(latency_ms) = runtime_shell.gateway_last_fetch_latency_ms {
                lines.push(format!(
                    "runtime_shell_gateway_last_fetch_latency_ms={latency_ms}"
                ));
            }
            if let Some(response_size) = runtime_shell.gateway_last_response_size {
                lines.push(format!(
                    "runtime_shell_gateway_last_response_size={response_size}"
                ));
            }
        }

        lines.join("\n")
    }

    pub fn render_network_status(&self) -> String {
        let mut lines = vec![format!("local_peer={}", self.local_peer_id)];
        lines.push(format!("mesh_state={}", self.mesh_state));
        lines.push(format!("reachability={}", self.network_reachability()));
        lines.push(format!("direct_connections={}", self.direct_connection_count()));
        lines.push(format!("relay_connections={}", self.relay_connection_count()));
        lines.push(format!("relay_reservations={}", self.reserved_relay_count()));
        lines.push(format!("hole_punch_attempts={}", self.hole_punch_attempt_count()));
        lines.push(format!("hole_punch_successes={}", self.hole_punch_success_count()));
        lines.push(format!("bootstrap_connected={}", self.connected_bootstrap_count()));
        lines.push(format!("bootstrap_degraded={}", self.degraded_bootstrap_count()));
        if let Some(network) = &self.network {
            lines.push(format!(
                "listen_addresses={}",
                if network.listen_addresses.is_empty() {
                    "-".to_string()
                } else {
                    network.listen_addresses.join(",")
                }
            ));
            lines.push(format!(
                "observed_addresses={}",
                if network.observed_addresses.is_empty() {
                    "-".to_string()
                } else {
                    network.observed_addresses.join(",")
                }
            ));
        }
        lines.join("\n")
    }

    pub fn render_network_reachability(&self) -> String {
        let mut lines = vec![format!("reachability={}", self.network_reachability())];
        let Some(network) = &self.network else {
            lines.push("nat_detail=-".to_string());
            lines.push("observed_addresses=-".to_string());
            lines.push("listen_addresses=-".to_string());
            return lines.join("\n");
        };
        lines.push(format!(
            "nat_detail={}",
            network.nat_detail.as_deref().unwrap_or("-")
        ));
        lines.push(format!(
            "observed_addresses={}",
            if network.observed_addresses.is_empty() {
                "-".to_string()
            } else {
                network.observed_addresses.join(",")
            }
        ));
        lines.push(format!(
            "listen_addresses={}",
            if network.listen_addresses.is_empty() {
                "-".to_string()
            } else {
                network.listen_addresses.join(",")
            }
        ));
        lines.join("\n")
    }

    pub fn render_network_bootstrap(&self) -> String {
        let mut lines = vec!["BOOTSTRAP ADDRESS                                   PEER ID                                            STATE       RESERVATION ATTEMPTS LAST CONNECTED   LAST ERROR".to_string()];
        let Some(network) = &self.network else {
            lines.push("no bootstrap peers configured".to_string());
            return lines.join("\n");
        };
        if network.bootstrap_peers.is_empty() {
            lines.push("no bootstrap peers configured".to_string());
            return lines.join("\n");
        }
        for bootstrap in &network.bootstrap_peers {
            lines.push(format!(
                "{:<51} {:<50} {:<11} {:<11} {:<8} {:<16} {}",
                bootstrap.address,
                bootstrap.peer_id.as_deref().unwrap_or("-"),
                bootstrap.state,
                bootstrap.relay_reservation,
                bootstrap.reconnect_attempts,
                bootstrap
                    .last_connected_unix_ms
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "-".to_string()),
                bootstrap.last_error.as_deref().unwrap_or("-"),
            ));
        }
        lines.join("\n")
    }

    pub fn render_network_peers(&self) -> String {
        let mut lines = vec!["PEER ID                                            STATE           PATH     LATENCY   HEALTH       RECONNECTS LAST ERROR".to_string()];
        if self.peers.is_empty() {
            lines.push("no peers observed".to_string());
            return lines.join("\n");
        }
        for peer in self.peers.values() {
            lines.push(format!(
                "{:<50} {:<15} {:<8} {:<9} {:<12} {:<10} {}",
                peer.peer_id,
                peer.state,
                peer.session.connection_path,
                peer.latency_ms
                    .map(|latency| format!("{latency}ms"))
                    .unwrap_or_else(|| "-".to_string()),
                peer.transport_health,
                peer.session.reconnect_attempts,
                peer.session.last_error.as_deref().unwrap_or("-"),
            ));
        }
        lines.join("\n")
    }

    pub fn render_network_relays(&self) -> String {
        let relay_bootstrap: Vec<&BootstrapPeerStatus> = self
            .network
            .as_ref()
            .map(|network| {
                network
                    .bootstrap_peers
                    .iter()
                    .filter(|bootstrap| bootstrap.relay_reservation != RelayReservationState::Inactive)
                    .collect()
            })
            .unwrap_or_default();
        let relay_peers: Vec<&PeerRecord> = self
            .peers
            .values()
            .filter(|peer| peer.session.connection_path == ConnectionPath::Relay)
            .collect();
        let mut lines = vec![format!("relay_reservations={}", self.reserved_relay_count())];
        lines.push(format!("relay_connections={}", relay_peers.len()));
        if relay_bootstrap.is_empty() && relay_peers.is_empty() {
            lines.push("relay_state=inactive".to_string());
            lines.push("note=relay fallback is not active in the current topology snapshot".to_string());
            return lines.join("\n");
        }
        for bootstrap in relay_bootstrap {
            lines.push(format!(
                "reservation relay_peer={} state={} last_error={}",
                bootstrap.peer_id.as_deref().unwrap_or("unknown"),
                bootstrap.relay_reservation,
                bootstrap
                    .last_relay_reservation_error
                    .as_deref()
                    .unwrap_or("-")
            ));
        }
        for peer in relay_peers {
            lines.push(format!(
                "peer={} relay_peer={} transport={} reason={} relay_age_secs={} latency={} last_error={}",
                peer.peer_id,
                peer.session.relay_peer_id.as_deref().unwrap_or("unknown"),
                peer.transport.as_deref().unwrap_or("-"),
                peer.session.relay_activation_reason.as_deref().unwrap_or("-"),
                peer
                    .session
                    .relay_established_at_unix_ms
                    .map(relay_age_secs)
                    .unwrap_or_default(),
                peer.latency_ms
                    .map(|latency| format!("{latency}ms"))
                    .unwrap_or_else(|| "-".to_string()),
                peer.session.last_error.as_deref().unwrap_or("-"),
            ));
        }
        lines.join("\n")
    }

    pub fn render_network_sessions(&self) -> String {
        let mut lines = vec!["PEER ID                                            PATH     RELAY PEER                                         HOLE PUNCH  DIRECT UPGRADE  REASON               LAST ERROR".to_string()];
        if self.peers.is_empty() {
            lines.push("no peer sessions observed".to_string());
            return lines.join("\n");
        }
        for peer in self.peers.values() {
            lines.push(format!(
                "{:<50} {:<8} {:<50} {:<11} {:<15} {:<20} {}",
                peer.peer_id,
                peer.session.connection_path,
                peer.session.relay_peer_id.as_deref().unwrap_or("-"),
                format!(
                    "{}/{}",
                    peer.session.hole_punch_successes,
                    peer.session.hole_punch_attempts
                ),
                peer.session.direct_upgrade_attempts,
                peer.session.relay_activation_reason.as_deref().unwrap_or("-"),
                peer.session.last_error.as_deref().unwrap_or("-"),
            ));
        }
        lines.join("\n")
    }

    pub fn render_network_diagnostics(&self) -> String {
        let mut sections = vec![
            self.render_network_status(),
            self.render_network_reachability(),
            self.render_network_bootstrap(),
            self.render_network_peers(),
            self.render_network_sessions(),
        ];
        sections.push(self.render_network_relays());
        sections.join("\n\n")
    }

    fn set_state(&mut self, peer_id: impl Into<String>, state: PeerConnectionState) {
        let peer_id = peer_id.into();
        let record = self
            .peers
            .entry(peer_id.clone())
            .or_insert_with(|| PeerRecord::new(peer_id));
        record.state = state;
        record.last_seen_unix_ms = unix_millis();
        self.updated_unix_ms = unix_millis();
    }

    fn direct_connection_count(&self) -> usize {
        self.peers
            .values()
            .filter(|peer| peer.session.connection_path == ConnectionPath::Direct)
            .count()
    }

    fn relay_connection_count(&self) -> usize {
        self.peers
            .values()
            .filter(|peer| peer.session.connection_path == ConnectionPath::Relay)
            .count()
    }

    fn reserved_relay_count(&self) -> usize {
        self.network
            .as_ref()
            .map(|network| {
                network
                    .bootstrap_peers
                    .iter()
                    .filter(|bootstrap| bootstrap.relay_reservation == RelayReservationState::Reserved)
                    .count()
            })
            .unwrap_or_default()
    }

    fn hole_punch_attempt_count(&self) -> u32 {
        self.peers
            .values()
            .map(|peer| peer.session.hole_punch_attempts)
            .sum()
    }

    fn hole_punch_success_count(&self) -> u32 {
        self.peers
            .values()
            .map(|peer| peer.session.hole_punch_successes)
            .sum()
    }

    fn connected_bootstrap_count(&self) -> usize {
        self.network
            .as_ref()
            .map(|network| {
                network
                    .bootstrap_peers
                    .iter()
                    .filter(|bootstrap| bootstrap.state == BootstrapState::Connected)
                    .count()
            })
            .unwrap_or_default()
    }

    fn degraded_bootstrap_count(&self) -> usize {
        self.network
            .as_ref()
            .map(|network| {
                network
                    .bootstrap_peers
                    .iter()
                    .filter(|bootstrap| bootstrap.state == BootstrapState::Degraded)
                    .count()
            })
            .unwrap_or_default()
    }
}

fn render_runtime_lines(peer_id: &str, runtime: &PeerRuntimeInfo) -> Vec<String> {
    vec![
        format!("[{peer_id}] runtime_version={}", runtime.runtime_version),
        format!("[{peer_id}] node_state={}", runtime.node_state),
        format!("[{peer_id}] uptime_secs={}", runtime.uptime_secs),
        format!("[{peer_id}] runtime_ready={}", runtime.runtime_ready),
        format!("[{peer_id}] transport_health={}", runtime.transport_health),
        format!(
            "[{peer_id}] capabilities={}",
            runtime.capabilities.join(",")
        ),
    ]
}

fn merge_addresses(existing: &mut Vec<String>, incoming: Vec<String>) {
    for address in incoming {
        if !existing.contains(&address) {
            existing.push(address);
        }
    }
}

fn dedupe_strings(items: Vec<String>) -> Vec<String> {
    let mut items = items;
    items.sort();
    items.dedup();
    items
}

fn bootstrap_record_mut<'a>(
    records: &'a mut [BootstrapPeerStatus],
    peer_id: Option<&str>,
    address: Option<&str>,
) -> Option<&'a mut BootstrapPeerStatus> {
    records.iter_mut().find(|record| {
        peer_id
            .map(|candidate| record.peer_id.as_deref() == Some(candidate))
            .unwrap_or(false)
            || address.map(|candidate| record.address == candidate).unwrap_or(false)
    })
}

fn peer_id_from_address(address: &str) -> Option<String> {
    address.split("/p2p/").nth(1).map(str::to_string)
}

fn clear_relay_session(session: &mut SessionInfo) {
    session.relay_peer_id = None;
    session.relay_activation_reason = None;
    session.relay_established_at_unix_ms = None;
}

fn relay_age_secs(established_at_unix_ms: u128) -> u128 {
    unix_millis().saturating_sub(established_at_unix_ms) / 1000
}

fn infer_connection_path(address: Option<&str>) -> ConnectionPath {
    match address {
        Some(address) if address.contains("/p2p-circuit") => ConnectionPath::Relay,
        Some(_) => ConnectionPath::Direct,
        None => ConnectionPath::Unknown,
    }
}

fn infer_reachability(
    listen_addresses: &[String],
    observed_addresses: &[String],
) -> NetworkReachability {
    observed_addresses
        .iter()
        .chain(listen_addresses.iter())
        .filter_map(|address| parse_address_reachability(address))
        .max_by_key(|reachability| match reachability {
            NetworkReachability::Unknown => 0,
            NetworkReachability::Private => 1,
            NetworkReachability::Public => 2,
        })
        .unwrap_or(NetworkReachability::Unknown)
}

fn parse_address_reachability(address: &str) -> Option<NetworkReachability> {
    let Ok(address) = address.parse::<Multiaddr>() else {
        return None;
    };
    address.iter().find_map(|protocol| match protocol {
        Protocol::Ip4(ip) => classify_ip(IpAddr::V4(ip)),
        Protocol::Ip6(ip) => classify_ip(IpAddr::V6(ip)),
        _ => None,
    })
}

fn classify_ip(address: IpAddr) -> Option<NetworkReachability> {
    match address {
        IpAddr::V4(ip)
            if ip.is_unspecified() || ip.is_loopback() || ip.is_link_local() =>
        {
            Some(NetworkReachability::Unknown)
        }
        IpAddr::V4(ip) if is_private_v4(ip) => Some(NetworkReachability::Private),
        IpAddr::V4(_) => Some(NetworkReachability::Public),
        IpAddr::V6(ip) if ip.is_unspecified() || ip.is_loopback() => {
            Some(NetworkReachability::Unknown)
        }
        IpAddr::V6(ip) if is_private_v6(ip) => Some(NetworkReachability::Private),
        IpAddr::V6(_) => Some(NetworkReachability::Public),
    }
}

fn is_private_v4(ip: Ipv4Addr) -> bool {
    ip.is_private() || ip.is_link_local() || ip.is_broadcast()
}

fn is_private_v6(ip: Ipv6Addr) -> bool {
    ip.is_unique_local() || ip.is_unicast_link_local()
}

fn normalize_capabilities(mut capabilities: Vec<String>) -> Vec<String> {
    capabilities.sort();
    capabilities.dedup();
    capabilities
}

fn unix_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default()
}

#[derive(Debug, thiserror::Error)]
pub enum TopologyError {
    #[error("topology IO failed: {0}")]
    Io(#[from] io::Error),
    #[error("topology JSON failed: {0}")]
    Json(#[from] serde_json::Error),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn topology_renders_peer_table() {
        let mut topology = PeerTopology::new("local");
        topology.observe_discovered("peer-a", vec!["/ip4/127.0.0.1/udp/1/quic-v1".into()]);
        topology.observe_connected("peer-a", None, "quic-v1");
        topology.observe_latency("peer-a", 41);
        topology.observe_runtime_state(
            "peer-a",
            PeerRuntimeInfo::new(
                "voidnet/0.1.0",
                NodeLifecycleState::Active,
                12,
                vec!["runtime/mount".into()],
                TransportHealth::Healthy,
                true,
            ),
        );

        let table = topology.render_table();
        assert!(table.contains("peer-a"));
        assert!(table.contains("41ms"));
        assert!(table.contains("HEALTHY"));
    }

    #[test]
    fn renders_runtime_and_sessions() {
        let mut topology = PeerTopology::new("local");
        topology.set_local_runtime(PeerRuntimeInfo::new(
            "voidnet/0.1.0",
            NodeLifecycleState::Discovering,
            4,
            vec!["runtime/mount".into(), "mesh/runtime-events".into()],
            TransportHealth::Healthy,
            true,
        ));
        topology.observe_connected("peer-a", None, "quic-v1");
        topology.observe_transport_encryption("peer-a", "libp2p-quic");
        topology.observe_runtime_state(
            "peer-a",
            PeerRuntimeInfo::new(
                "voidnet/0.1.0",
                NodeLifecycleState::Active,
                8,
                vec!["runtime/mount".into()],
                TransportHealth::Healthy,
                true,
            ),
        );

        assert!(topology
            .render_runtime()
            .contains("runtime_version=voidnet/0.1.0"));
        assert!(topology.render_sessions().contains("libp2p-quic"));
    }

    #[test]
    fn tracks_bootstrap_and_reachability_diagnostics() {
        let mut topology = PeerTopology::new("local");
        topology.set_listen_addresses(vec!["/ip4/0.0.0.0/udp/40100/quic-v1".into()]);
        topology.configure_bootstrap(vec![
            "/ip4/203.0.113.10/udp/40100/quic-v1/p2p/12D3KooWbootstrap".into(),
        ]);

        let attempt = topology.observe_bootstrap_dial(
            "/ip4/203.0.113.10/udp/40100/quic-v1/p2p/12D3KooWbootstrap",
        );
        assert_eq!(attempt, 1);
        assert!(topology.observe_bootstrap_connected(
            "12D3KooWbootstrap",
            Some("/ip4/203.0.113.10/udp/40100/quic-v1/p2p/12D3KooWbootstrap"),
        ));
        assert_eq!(
            topology.observe_observed_address("/ip4/198.51.100.22/udp/40100/quic-v1"),
            Some(NetworkReachability::Public)
        );

        let diagnostics = topology.render_network_diagnostics();
        assert!(diagnostics.contains("bootstrap_connected=1"));
        assert!(diagnostics.contains("reachability=PUBLIC"));
        assert!(diagnostics.contains("12D3KooWbootstrap"));
    }
}
