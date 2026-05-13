use crate::{
    lifecycle::NodeLifecycleState,
    topology::{MeshState, NetworkReachability, PeerConnectionState, PeerRuntimeInfo},
};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tokio::sync::broadcast;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeMeshAnnouncement {
    pub peer_id: String,
    pub runtime: PeerRuntimeInfo,
    pub latency_ms: Option<u128>,
    pub encrypted_session_established: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TransportEvent {
    IdentityLoaded {
        peer_id: String,
        fingerprint: String,
        path: PathBuf,
    },
    IdentityPersisted {
        peer_id: String,
        fingerprint: String,
        path: PathBuf,
    },
    RuntimeMounted {
        peer_id: String,
        runtime: PeerRuntimeInfo,
    },
    RuntimeReady {
        peer_id: String,
        runtime: PeerRuntimeInfo,
    },
    SurfaceResolving {
        route: String,
    },
    SurfaceResolved {
        route: String,
        peer_owner: String,
        surface_id: String,
        latency_ms: u128,
    },
    SurfaceMounting {
        route: String,
        peer_owner: String,
        surface_id: String,
    },
    SurfaceMounted {
        route: String,
        surface_id: String,
        session_id: String,
        latency_ms: u128,
    },
    SurfaceLoaded {
        route: String,
        surface_id: String,
        state_revision: u64,
    },
    SurfaceUpdated {
        route: String,
        surface_id: String,
        changed_bindings: Vec<String>,
        source: String,
    },
    SurfaceActionTriggered {
        route: String,
        surface_id: String,
        action: String,
    },
    SurfaceStateChanged {
        route: String,
        surface_id: String,
        state_revision: u64,
        changed_bindings: Vec<String>,
        distributed: bool,
    },
    SurfaceParsed {
        route: String,
        surface_id: String,
    },
    RuntimeTreeBuilt {
        route: String,
        surface_id: String,
        nodes: usize,
    },
    SurfaceRendered {
        route: String,
        surface_id: String,
        nodes: usize,
        render_ms: u128,
    },
    SurfaceRenderCompleted {
        route: String,
        surface_id: String,
        nodes: usize,
        affected_nodes: usize,
        rerender_count: u64,
        render_ms: u128,
    },
    SurfaceRenderFailed {
        route: String,
        surface_id: String,
        error: String,
    },
    InputUpdated {
        route: String,
        surface_id: String,
        input_id: String,
    },
    ActionDispatched {
        route: String,
        surface_id: String,
        action: String,
    },
    SurfacePermissionDenied {
        route: String,
        surface_id: String,
        action: Option<String>,
        capability: String,
        reason: String,
    },
    GatewayRegistered {
        domain: String,
        gateway_id: String,
        owner_peer: String,
    },
    GatewayTrustEvaluated {
        domain: String,
        trust_state: String,
        trust_level: String,
        warning: Option<String>,
    },
    GatewayMounted {
        route: String,
        gateway_id: String,
        external_target: String,
    },
    GatewayBridgePrepared {
        route: String,
        gateway_id: String,
        protocol_stack: Vec<String>,
        external_target: String,
    },
    BridgeSessionStarted {
        route: String,
        session_id: String,
        gateway_id: String,
        external_target: String,
    },
    GatewayFetchDispatched {
        route: String,
        gateway_id: String,
        request_id: String,
        method: String,
        external_target: String,
    },
    GatewayResponseReceived {
        route: String,
        gateway_id: String,
        response_id: String,
        status: u16,
        content_type: Option<String>,
        bytes: usize,
        latency_ms: u128,
    },
    GatewayStreamUpdated {
        route: String,
        gateway_id: String,
        chunks: usize,
        bytes: usize,
        final_chunk: bool,
    },
    ExternalResourceMounted {
        route: String,
        gateway_id: String,
        snapshot_id: String,
        content_type: Option<String>,
        bytes: usize,
        cache_state: String,
    },
    ResponseSurfaceRendered {
        route: String,
        gateway_id: String,
        bytes: usize,
        render_mode: String,
    },
    GatewayBridgeFailed {
        route: String,
        gateway_id: String,
        error: String,
    },
    SurfaceHotReloaded {
        route: String,
        surface_id: String,
        generation: u64,
        preserved_session: bool,
    },
    SurfaceUnmounted {
        route: String,
        surface_id: String,
        session_id: String,
    },
    RuntimeShutdown {
        peer_id: String,
        runtime: PeerRuntimeInfo,
    },
    CapabilityRequested {
        route: String,
        capability: String,
        peer_owner: String,
    },
    CapabilityGranted {
        peer_id: String,
        capability: String,
        scope: String,
    },
    CapabilityRejected {
        peer_id: String,
        capability: String,
        reason: String,
    },
    BootstrapConfigured {
        peers: usize,
    },
    BootstrapDialAttempt {
        peer_id: Option<String>,
        address: String,
        attempt: u32,
    },
    BootstrapConnected {
        peer_id: String,
        address: Option<String>,
    },
    NatStatusChanged {
        status: String,
        public_address: Option<String>,
        confidence: usize,
    },
    RelayReservationAttempted {
        relay_peer_id: String,
        address: String,
    },
    RelayReservationAccepted {
        relay_peer_id: String,
        renewal: bool,
    },
    RelayReservationFailed {
        relay_peer_id: Option<String>,
        address: Option<String>,
        error: String,
    },
    RelayFallbackActivated {
        peer_id: String,
        relay_peer_id: Option<String>,
        reason: String,
    },
    RelayCircuitEstablished {
        peer_id: String,
        relay_peer_id: Option<String>,
        address: Option<String>,
    },
    RelaySessionEstablished {
        peer_id: String,
        relay_peer_id: Option<String>,
        address: Option<String>,
    },
    HolePunchAttempt {
        peer_id: String,
        relay_peer_id: Option<String>,
    },
    HolePunchSucceeded {
        peer_id: String,
    },
    HolePunchFailed {
        peer_id: String,
        error: String,
    },
    DirectUpgradeSucceeded {
        peer_id: String,
    },
    DirectUpgradeFailed {
        peer_id: String,
        error: String,
    },
    ReachabilityChanged {
        reachability: NetworkReachability,
        observed_address: Option<String>,
        detail: String,
    },
    Listening {
        address: String,
    },
    PeerDiscovered {
        peer_id: String,
        addresses: Vec<String>,
        source: DiscoverySource,
    },
    PeerAuthenticated {
        peer_id: String,
        agent: String,
        protocols: Vec<String>,
    },
    PeerRuntimeDiscovered {
        peer_id: String,
        runtime: PeerRuntimeInfo,
    },
    PeerDisconnected {
        peer_id: String,
        reason: String,
    },
    DirectConnectionEstablished {
        peer_id: String,
        address: Option<String>,
    },
    TransportConnected {
        peer_id: String,
        address: Option<String>,
        transport: String,
    },
    TransportFailed {
        peer_id: Option<String>,
        address: Option<String>,
        error: String,
    },
    SessionEncrypted {
        peer_id: String,
        transport: String,
        cipher: String,
    },
    EncryptedSessionEstablished {
        peer_id: String,
        transport: String,
        cipher: String,
    },
    DnsRecordPublished {
        domain: String,
        owner_peer_id: String,
        target_peer_id: String,
        runtime_surface: String,
        ttl_secs: u64,
    },
    DnsRecordUpdated {
        domain: String,
        owner_peer_id: String,
        target_peer_id: String,
        runtime_surface: String,
        ttl_secs: u64,
    },
    DnsRecordExpired {
        domain: String,
        owner_peer_id: String,
    },
    DnsRecordRejected {
        domain: String,
        reason: String,
    },
    DnsConflictDetected {
        domain: String,
        active_owner_peer_id: String,
        conflicting_owner_peer_id: String,
    },
    DnsResolutionSucceeded {
        domain: String,
        target_peer_id: String,
        runtime_surface: String,
        latency_ms: u128,
    },
    DnsResolutionFailed {
        domain: String,
        reason: String,
    },
    RuntimeSessionStarted {
        route: String,
        session_id: String,
        peer_owner: String,
        surface_id: String,
    },
    RuntimeSessionClosed {
        route: String,
        session_id: String,
        peer_owner: String,
        surface_id: String,
    },
    SessionNegotiationStarted {
        peer_id: String,
        session_id: String,
        transport: String,
    },
    PayloadVerified {
        peer_id: String,
        session_id: String,
        message_id: String,
    },
    EncryptedMessageDelivered {
        peer_id: String,
        session_id: String,
        direction: String,
        size_bytes: usize,
    },
    InvalidSignatureRejected {
        peer_id: Option<String>,
        context: String,
    },
    ReplayRejected {
        peer_id: String,
        context: String,
        nonce: String,
    },
    DecryptionFailed {
        peer_id: String,
        session_id: String,
        reason: String,
    },
    SessionRecovered {
        peer_id: String,
        active_rooms: usize,
        inbox_messages: usize,
        unread_messages: usize,
    },
    MessageReceived {
        peer_id: String,
        room: Option<String>,
        session_id: String,
    },
    InboxSynchronized {
        messages: usize,
        unread_messages: usize,
        room: Option<String>,
    },
    PresenceUpdated {
        peer_id: String,
        room: Option<String>,
        presence: String,
        last_seen_unix_ms: u128,
    },
    NotificationRaised {
        kind: String,
        room: Option<String>,
        peer_id: Option<String>,
        unread_notifications: usize,
    },
    RoomJoined {
        room: String,
        peer_id: String,
    },
    RoomLeft {
        room: String,
        peer_id: String,
    },
    RoomStateSynchronized {
        room: String,
        peer_id: String,
        members: usize,
        events: usize,
        reason: String,
    },
    RoomMembershipChanged {
        room: String,
        peer_id: String,
        action: String,
    },
    MeshPartitionDetected {
        affected_peers: usize,
        reason: String,
    },
    MeshStateChanged {
        state: MeshState,
        reason: String,
    },
    PartitionDetected {
        affected_peers: usize,
        reason: String,
    },
    PartitionRecovered {
        recovered_peers: usize,
        reason: String,
    },
    Ping {
        peer_id: String,
        latency_ms: u128,
    },
    PeerStateChanged {
        peer_id: String,
        state: PeerConnectionState,
    },
    LifecycleTransition {
        from: NodeLifecycleState,
        to: NodeLifecycleState,
        reason: String,
    },
    TopologyPersisted {
        path: PathBuf,
    },
    Shutdown {
        state: NodeLifecycleState,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DiscoverySource {
    Bootstrap,
    Mdns,
    Identify,
    Reconnect,
    Manual,
}

#[derive(Clone)]
pub struct EventBus {
    tx: broadcast::Sender<TransportEvent>,
}

impl EventBus {
    pub fn new(capacity: usize) -> Self {
        let (tx, _) = broadcast::channel(capacity);
        Self { tx }
    }

    pub fn emit(&self, event: TransportEvent) {
        let _ = self.tx.send(event);
    }

    pub fn subscribe(&self) -> broadcast::Receiver<TransportEvent> {
        self.tx.subscribe()
    }
}

impl TransportEvent {
    pub fn log_line(&self) -> String {
        match self {
            TransportEvent::IdentityLoaded {
                peer_id,
                fingerprint,
                path,
            } => format!(
                "[VOIDNET][IDENTITY] IdentityLoaded peer={peer_id} fingerprint={fingerprint} path={}",
                path.display()
            ),
            TransportEvent::IdentityPersisted {
                peer_id,
                fingerprint,
                path,
            } => format!(
                "[VOIDNET][IDENTITY] IdentityPersisted peer={peer_id} fingerprint={fingerprint} path={}",
                path.display()
            ),
            TransportEvent::RuntimeMounted { peer_id, runtime } => format!(
                "[VOIDNET][RUNTIME] RuntimeMounted peer={peer_id} version={} state={} ready={} capabilities={}",
                runtime.runtime_version,
                runtime.node_state,
                runtime.runtime_ready,
                runtime.capabilities.join(",")
            ),
            TransportEvent::RuntimeReady { peer_id, runtime } => format!(
                "[VOIDNET][RUNTIME] RuntimeReady peer={peer_id} uptime={}s health={} capabilities={}",
                runtime.uptime_secs,
                runtime.transport_health,
                runtime.capabilities.join(",")
            ),
            TransportEvent::SurfaceResolving { route } => {
                format!("[VOIDNET][RUNTIME] SurfaceResolving route={route}")
            }
            TransportEvent::SurfaceResolved {
                route,
                peer_owner,
                surface_id,
                latency_ms,
            } => format!(
                "[VOIDNET][RUNTIME] SurfaceResolved route={route} peer={peer_owner} surface={surface_id} latency={}ms",
                latency_ms
            ),
            TransportEvent::SurfaceMounting {
                route,
                peer_owner,
                surface_id,
            } => format!(
                "[VOIDNET][RUNTIME] SurfaceMounting route={route} peer={peer_owner} surface={surface_id}"
            ),
            TransportEvent::SurfaceMounted {
                route,
                surface_id,
                session_id,
                latency_ms,
            } => format!(
                "[VOIDNET][RUNTIME] SurfaceMounted route={route} surface={surface_id} session={session_id} latency={}ms",
                latency_ms
            ),
            TransportEvent::SurfaceLoaded {
                route,
                surface_id,
                state_revision,
            } => format!(
                "[VOIDNET][SURFACE] SurfaceLoaded route={route} surface={surface_id} revision={state_revision}"
            ),
            TransportEvent::SurfaceUpdated {
                route,
                surface_id,
                changed_bindings,
                source,
            } => format!(
                "[VOIDNET][SURFACE] SurfaceUpdated route={route} surface={surface_id} source={source} changed={}"
                ,
                if changed_bindings.is_empty() {
                    "-".to_string()
                } else {
                    changed_bindings.join(",")
                }
            ),
            TransportEvent::SurfaceActionTriggered {
                route,
                surface_id,
                action,
            } => format!(
                "[VOIDNET][SURFACE] SurfaceActionTriggered route={route} surface={surface_id} action={action}"
            ),
            TransportEvent::SurfaceStateChanged {
                route,
                surface_id,
                state_revision,
                changed_bindings,
                distributed,
            } => format!(
                "[VOIDNET][SURFACE] SurfaceStateChanged route={route} surface={surface_id} revision={state_revision} distributed={} changed={}"
                ,
                if *distributed { "yes" } else { "no" },
                if changed_bindings.is_empty() {
                    "-".to_string()
                } else {
                    changed_bindings.join(",")
                }
            ),
            TransportEvent::SurfaceParsed { route, surface_id } => {
                format!("[VOIDNET][UI] SurfaceParsed route={route} surface={surface_id}")
            }
            TransportEvent::RuntimeTreeBuilt {
                route,
                surface_id,
                nodes,
            } => format!(
                "[VOIDNET][UI] RuntimeTreeBuilt route={route} surface={surface_id} nodes={nodes}"
            ),
            TransportEvent::SurfaceRendered {
                route,
                surface_id,
                nodes,
                render_ms,
            } => format!(
                "[VOIDNET][UI] SurfaceRendered route={route} surface={surface_id} nodes={nodes} render={}ms",
                render_ms
            ),
            TransportEvent::SurfaceRenderCompleted {
                route,
                surface_id,
                nodes,
                affected_nodes,
                rerender_count,
                render_ms,
            } => format!(
                "[VOIDNET][SURFACE] SurfaceRenderCompleted route={route} surface={surface_id} nodes={nodes} affected_nodes={affected_nodes} rerenders={rerender_count} render={}ms",
                render_ms
            ),
            TransportEvent::SurfaceRenderFailed {
                route,
                surface_id,
                error,
            } => format!(
                "[VOIDNET][SURFACE] SurfaceRenderFailed route={route} surface={surface_id} error={error}"
            ),
            TransportEvent::InputUpdated {
                route,
                surface_id,
                input_id,
            } => format!(
                "[VOIDNET][UI] InputUpdated route={route} surface={surface_id} input={input_id}"
            ),
            TransportEvent::ActionDispatched {
                route,
                surface_id,
                action,
            } => format!(
                "[VOIDNET][UI] ActionDispatched route={route} surface={surface_id} action={action}"
            ),
            TransportEvent::SurfacePermissionDenied {
                route,
                surface_id,
                action,
                capability,
                reason,
            } => format!(
                "[VOIDNET][SURFACE] SurfacePermissionDenied route={route} surface={surface_id} action={} capability={capability} reason={reason}",
                action.as_deref().unwrap_or("-")
            ),
            TransportEvent::GatewayRegistered {
                domain,
                gateway_id,
                owner_peer,
            } => format!(
                "[VOIDNET][GATEWAY] GatewayRegistered route={domain} gateway_id={gateway_id} owner={owner_peer}"
            ),
            TransportEvent::GatewayTrustEvaluated {
                domain,
                trust_state,
                trust_level,
                warning,
            } => format!(
                "[VOIDNET][GATEWAY] TrustEvaluated domain={domain} state={trust_state} level={trust_level} warning={}",
                warning.as_deref().unwrap_or("-")
            ),
            TransportEvent::GatewayMounted {
                route,
                gateway_id,
                external_target,
            } => format!(
                "[VOIDNET][GATEWAY] GatewayMounted route={route} gateway_id={gateway_id} external_target={external_target}"
            ),
            TransportEvent::GatewayBridgePrepared {
                route,
                gateway_id,
                protocol_stack,
                external_target,
            } => format!(
                "[VOIDNET][GATEWAY] BridgePrepared route={route} gateway_id={gateway_id} protocols={} external_target={external_target}",
                protocol_stack.join(",")
            ),
            TransportEvent::BridgeSessionStarted {
                route,
                session_id,
                gateway_id,
                external_target,
            } => format!(
                "[VOIDNET][GATEWAY] BridgeSessionStarted route={route} session={session_id} gateway_id={gateway_id} target={external_target}"
            ),
            TransportEvent::GatewayFetchDispatched {
                route,
                gateway_id,
                request_id,
                method,
                external_target,
            } => format!(
                "[VOIDNET][GATEWAY] FetchDispatched route={route} gateway_id={gateway_id} request_id={request_id} method={method} target={external_target}"
            ),
            TransportEvent::GatewayResponseReceived {
                route,
                gateway_id,
                response_id,
                status,
                content_type,
                bytes,
                latency_ms,
            } => format!(
                "[VOIDNET][GATEWAY] ResponseReceived route={route} gateway_id={gateway_id} response_id={response_id} status={status} content_type={} bytes={bytes} latency={}ms",
                content_type.as_deref().unwrap_or("application/octet-stream"),
                latency_ms,
            ),
            TransportEvent::GatewayStreamUpdated {
                route,
                gateway_id,
                chunks,
                bytes,
                final_chunk,
            } => format!(
                "[VOIDNET][GATEWAY] StreamUpdated route={route} gateway_id={gateway_id} chunks={chunks} bytes={bytes} final_chunk={}",
                if *final_chunk { "yes" } else { "no" }
            ),
            TransportEvent::ExternalResourceMounted {
                route,
                gateway_id,
                snapshot_id,
                content_type,
                bytes,
                cache_state,
            } => format!(
                "[VOIDNET][GATEWAY] ExternalResourceMounted route={route} gateway_id={gateway_id} snapshot={snapshot_id} content_type={} bytes={bytes} cache_state={cache_state}",
                content_type.as_deref().unwrap_or("application/octet-stream")
            ),
            TransportEvent::ResponseSurfaceRendered {
                route,
                gateway_id,
                bytes,
                render_mode,
            } => format!(
                "[VOIDNET][GATEWAY] ResponseSurfaceRendered route={route} gateway_id={gateway_id} bytes={bytes} render_mode={render_mode}"
            ),
            TransportEvent::GatewayBridgeFailed {
                route,
                gateway_id,
                error,
            } => format!(
                "[VOIDNET][GATEWAY] BridgeFailed route={route} gateway_id={gateway_id} error={error}"
            ),
            TransportEvent::SurfaceHotReloaded {
                route,
                surface_id,
                generation,
                preserved_session,
            } => format!(
                "[VOIDNET][SURFACE] SurfaceHotReloaded route={route} surface={surface_id} generation={generation} preserved_session={}"
                ,
                if *preserved_session { "yes" } else { "no" }
            ),
            TransportEvent::SurfaceUnmounted {
                route,
                surface_id,
                session_id,
            } => format!(
                "[VOIDNET][RUNTIME] SurfaceUnmounted route={route} surface={surface_id} session={session_id}"
            ),
            TransportEvent::RuntimeShutdown { peer_id, runtime } => format!(
                "[VOIDNET][RUNTIME] RuntimeShutdown peer={peer_id} uptime={}s state={}",
                runtime.uptime_secs,
                runtime.node_state,
            ),
            TransportEvent::CapabilityRequested {
                route,
                capability,
                peer_owner,
            } => format!(
                "[VOIDNET][RUNTIME] CapabilityRequested route={route} capability={capability} peer={peer_owner}"
            ),
            TransportEvent::CapabilityGranted {
                peer_id,
                capability,
                scope,
            } => format!(
                "[VOIDNET][RUNTIME] CapabilityGranted peer={peer_id} capability={capability} scope={scope}"
            ),
            TransportEvent::CapabilityRejected {
                peer_id,
                capability,
                reason,
            } => format!(
                "[VOIDNET][RUNTIME] CapabilityRejected peer={peer_id} capability={capability} reason={reason}"
            ),
            TransportEvent::BootstrapConfigured { peers } => {
                format!("[VOIDNET][NETWORK] BootstrapConfigured peers={peers}")
            }
            TransportEvent::BootstrapDialAttempt {
                peer_id,
                address,
                attempt,
            } => format!(
                "[VOIDNET][NETWORK] BootstrapDialAttempt peer={} address={address} attempt={attempt}",
                peer_id.as_deref().unwrap_or("unknown")
            ),
            TransportEvent::BootstrapConnected { peer_id, address } => format!(
                "[VOIDNET][NETWORK] BootstrapConnected peer={peer_id} address={}",
                address.as_deref().unwrap_or("unknown")
            ),
            TransportEvent::NatStatusChanged {
                status,
                public_address,
                confidence,
            } => format!(
                "[VOIDNET][NETWORK] NatStatusChanged status={status} public_address={} confidence={confidence}",
                public_address.as_deref().unwrap_or("unknown")
            ),
            TransportEvent::RelayReservationAttempted {
                relay_peer_id,
                address,
            } => format!(
                "[VOIDNET][NETWORK] RelayReservationAttempted relay_peer={relay_peer_id} address={address}"
            ),
            TransportEvent::RelayReservationAccepted {
                relay_peer_id,
                renewal,
            } => format!(
                "[VOIDNET][NETWORK] RelayReservationAccepted relay_peer={relay_peer_id} renewal={renewal}"
            ),
            TransportEvent::RelayReservationFailed {
                relay_peer_id,
                address,
                error,
            } => format!(
                "[VOIDNET][NETWORK] RelayReservationFailed relay_peer={} address={} error={error}",
                relay_peer_id.as_deref().unwrap_or("unknown"),
                address.as_deref().unwrap_or("unknown")
            ),
            TransportEvent::RelayFallbackActivated {
                peer_id,
                relay_peer_id,
                reason,
            } => format!(
                "[VOIDNET][NETWORK] RelayFallbackActivated peer={peer_id} relay_peer={} reason={reason}",
                relay_peer_id.as_deref().unwrap_or("unknown")
            ),
            TransportEvent::RelayCircuitEstablished {
                peer_id,
                relay_peer_id,
                address,
            } => format!(
                "[VOIDNET][NETWORK] RelayCircuitEstablished peer={peer_id} relay_peer={} address={}",
                relay_peer_id.as_deref().unwrap_or("unknown"),
                address.as_deref().unwrap_or("unknown")
            ),
            TransportEvent::RelaySessionEstablished {
                peer_id,
                relay_peer_id,
                address,
            } => format!(
                "[VOIDNET][NETWORK] RelaySessionEstablished peer={peer_id} relay_peer={} address={}",
                relay_peer_id.as_deref().unwrap_or("unknown"),
                address.as_deref().unwrap_or("unknown")
            ),
            TransportEvent::HolePunchAttempt {
                peer_id,
                relay_peer_id,
            } => format!(
                "[VOIDNET][NETWORK] HolePunchAttempt peer={peer_id} relay_peer={}",
                relay_peer_id.as_deref().unwrap_or("unknown")
            ),
            TransportEvent::HolePunchSucceeded { peer_id } => format!(
                "[VOIDNET][NETWORK] HolePunchSucceeded peer={peer_id}"
            ),
            TransportEvent::HolePunchFailed { peer_id, error } => format!(
                "[VOIDNET][NETWORK] HolePunchFailed peer={peer_id} error={error}"
            ),
            TransportEvent::DirectUpgradeSucceeded { peer_id } => format!(
                "[VOIDNET][NETWORK] DirectUpgradeSucceeded peer={peer_id}"
            ),
            TransportEvent::DirectUpgradeFailed { peer_id, error } => format!(
                "[VOIDNET][NETWORK] DirectUpgradeFailed peer={peer_id} error={error}"
            ),
            TransportEvent::ReachabilityChanged {
                reachability,
                observed_address,
                detail,
            } => format!(
                "[VOIDNET][NETWORK] ReachabilityChanged reachability={reachability} observed_address={} detail={detail}",
                observed_address.as_deref().unwrap_or("unknown")
            ),
            TransportEvent::Listening { address } => {
                format!("[VOIDNET][TRANSPORT] Listening address={address} transport=quic-v1")
            }
            TransportEvent::PeerDiscovered {
                peer_id,
                addresses,
                source,
            } => format!(
                "[VOIDNET][TRANSPORT] PeerDiscovered peer={peer_id} source={source:?} addresses={}",
                addresses.join(",")
            ),
            TransportEvent::PeerAuthenticated {
                peer_id,
                agent,
                protocols,
            } => format!(
                "[VOIDNET][IDENTITY] PeerAuthenticated peer={peer_id} agent={agent} protocols={}",
                protocols.join(",")
            ),
            TransportEvent::PeerRuntimeDiscovered { peer_id, runtime } => format!(
                "[VOIDNET][RUNTIME] PeerRuntimeDiscovered peer={peer_id} version={} state={} uptime={}s health={} ready={}",
                runtime.runtime_version,
                runtime.node_state,
                runtime.uptime_secs,
                runtime.transport_health,
                runtime.runtime_ready,
            ),
            TransportEvent::PeerDisconnected { peer_id, reason } => {
                format!("[VOIDNET][TRANSPORT] PeerDisconnected peer={peer_id} reason={reason}")
            }
            TransportEvent::DirectConnectionEstablished { peer_id, address } => format!(
                "[VOIDNET][NETWORK] DirectSessionEstablished peer={peer_id} address={}",
                address.as_deref().unwrap_or("unknown")
            ),
            TransportEvent::TransportConnected {
                peer_id,
                address,
                transport,
            } => format!(
                "[VOIDNET][TRANSPORT] TransportConnected peer={peer_id} address={} transport={transport}",
                address.as_deref().unwrap_or("unknown")
            ),
            TransportEvent::TransportFailed {
                peer_id,
                address,
                error,
            } => format!(
                "[VOIDNET][TRANSPORT] TransportFailed peer={} address={} error={error}",
                peer_id.as_deref().unwrap_or("unknown"),
                address.as_deref().unwrap_or("unknown")
            ),
            TransportEvent::SessionEncrypted {
                peer_id,
                transport,
                cipher,
            } => format!(
                "[VOIDNET][TRANSPORT] SessionEncrypted peer={peer_id} transport={transport} cipher={cipher}"
            ),
            TransportEvent::EncryptedSessionEstablished {
                peer_id,
                transport,
                cipher,
            } => format!(
                "[VOIDNET][RUNTIME] EncryptedSessionEstablished peer={peer_id} transport={transport} cipher={cipher}"
            ),
            TransportEvent::DnsRecordPublished {
                domain,
                owner_peer_id,
                target_peer_id,
                runtime_surface,
                ttl_secs,
            } => format!(
                "[VOIDNET][DNS] RecordPublished domain={domain} owner={owner_peer_id} target={target_peer_id} surface={runtime_surface} ttl={}s",
                ttl_secs
            ),
            TransportEvent::DnsRecordUpdated {
                domain,
                owner_peer_id,
                target_peer_id,
                runtime_surface,
                ttl_secs,
            } => format!(
                "[VOIDNET][DNS] RecordUpdated domain={domain} owner={owner_peer_id} target={target_peer_id} surface={runtime_surface} ttl={}s",
                ttl_secs
            ),
            TransportEvent::DnsRecordExpired {
                domain,
                owner_peer_id,
            } => format!(
                "[VOIDNET][DNS] RecordExpired domain={domain} owner={owner_peer_id}"
            ),
            TransportEvent::DnsRecordRejected { domain, reason } => {
                format!("[VOIDNET][DNS] RecordRejected domain={domain} reason={reason}")
            }
            TransportEvent::DnsConflictDetected {
                domain,
                active_owner_peer_id,
                conflicting_owner_peer_id,
            } => format!(
                "[VOIDNET][DNS] ConflictDetected domain={domain} active_owner={active_owner_peer_id} conflicting_owner={conflicting_owner_peer_id}"
            ),
            TransportEvent::DnsResolutionSucceeded {
                domain,
                target_peer_id,
                runtime_surface,
                latency_ms,
            } => format!(
                "[VOIDNET][DNS] ResolutionSucceeded domain={domain} peer={target_peer_id} surface={runtime_surface} latency={}ms",
                latency_ms
            ),
            TransportEvent::DnsResolutionFailed { domain, reason } => {
                format!("[VOIDNET][DNS] ResolutionFailed domain={domain} reason={reason}")
            }
            TransportEvent::RuntimeSessionStarted {
                route,
                session_id,
                peer_owner,
                surface_id,
            } => format!(
                "[VOIDNET][RUNTIME] RuntimeSessionStarted route={route} session={session_id} peer={peer_owner} surface={surface_id}"
            ),
            TransportEvent::RuntimeSessionClosed {
                route,
                session_id,
                peer_owner,
                surface_id,
            } => format!(
                "[VOIDNET][RUNTIME] RuntimeSessionClosed route={route} session={session_id} peer={peer_owner} surface={surface_id}"
            ),
            TransportEvent::SessionNegotiationStarted {
                peer_id,
                session_id,
                transport,
            } => format!(
                "[VOIDNET][CHAT] SessionNegotiationStarted peer={peer_id} session={session_id} transport={transport}"
            ),
            TransportEvent::PayloadVerified {
                peer_id,
                session_id,
                message_id,
            } => format!(
                "[VOIDNET][CHAT] PayloadVerified peer={peer_id} session={session_id} message={message_id}"
            ),
            TransportEvent::EncryptedMessageDelivered {
                peer_id,
                session_id,
                direction,
                size_bytes,
            } => format!(
                "[VOIDNET][CHAT] EncryptedMessageDelivered peer={peer_id} session={session_id} direction={direction} bytes={size_bytes}"
            ),
            TransportEvent::InvalidSignatureRejected { peer_id, context } => format!(
                "[VOIDNET][CHAT] InvalidSignatureRejected peer={} context={context}",
                peer_id.as_deref().unwrap_or("unknown")
            ),
            TransportEvent::ReplayRejected {
                peer_id,
                context,
                nonce,
            } => format!(
                "[VOIDNET][CHAT] ReplayRejected peer={peer_id} context={context} nonce={nonce}"
            ),
            TransportEvent::DecryptionFailed {
                peer_id,
                session_id,
                reason,
            } => format!(
                "[VOIDNET][CHAT] DecryptionFailed peer={peer_id} session={session_id} reason={reason}"
            ),
            TransportEvent::SessionRecovered {
                peer_id,
                active_rooms,
                inbox_messages,
                unread_messages,
            } => format!(
                "[VOIDNET][CHAT] SessionRecovered peer={peer_id} rooms={active_rooms} inbox={inbox_messages} unread={unread_messages}"
            ),
            TransportEvent::MessageReceived {
                peer_id,
                room,
                session_id,
            } => format!(
                "[VOIDNET][CHAT] MessageReceived peer={peer_id} room={} session={session_id}",
                room.as_deref().unwrap_or("direct")
            ),
            TransportEvent::InboxSynchronized {
                messages,
                unread_messages,
                room,
            } => format!(
                "[VOIDNET][CHAT] InboxSynchronized messages={messages} unread={unread_messages} room={}",
                room.as_deref().unwrap_or("all")
            ),
            TransportEvent::PresenceUpdated {
                peer_id,
                room,
                presence,
                last_seen_unix_ms,
            } => format!(
                "[VOIDNET][CHAT] PresenceUpdated peer={peer_id} room={} presence={presence} last_seen={last_seen_unix_ms}",
                room.as_deref().unwrap_or("-")
            ),
            TransportEvent::NotificationRaised {
                kind,
                room,
                peer_id,
                unread_notifications,
            } => format!(
                "[VOIDNET][CHAT] NotificationRaised kind={kind} room={} peer={} unread={unread_notifications}",
                room.as_deref().unwrap_or("-"),
                peer_id.as_deref().unwrap_or("-")
            ),
            TransportEvent::RoomJoined { room, peer_id } => {
                format!("[VOIDNET][CHAT] RoomJoined room={room} peer={peer_id}")
            }
            TransportEvent::RoomLeft { room, peer_id } => {
                format!("[VOIDNET][CHAT] RoomLeft room={room} peer={peer_id}")
            }
            TransportEvent::RoomStateSynchronized {
                room,
                peer_id,
                members,
                events,
                reason,
            } => format!(
                "[VOIDNET][CHAT] RoomStateSynchronized room={room} peer={peer_id} members={members} events={events} reason={reason}"
            ),
            TransportEvent::RoomMembershipChanged {
                room,
                peer_id,
                action,
            } => format!(
                "[VOIDNET][CHAT] RoomMembershipChanged room={room} peer={peer_id} action={action}"
            ),
            TransportEvent::MeshPartitionDetected {
                affected_peers,
                reason,
            } => format!(
                "[VOIDNET][MESH] MeshPartitionDetected affected_peers={affected_peers} reason={reason}"
            ),
            TransportEvent::MeshStateChanged { state, reason } => {
                format!("[VOIDNET][MESH] MeshStateChanged state={state} reason={reason}")
            }
            TransportEvent::PartitionDetected {
                affected_peers,
                reason,
            } => format!(
                "[VOIDNET][MESH] PartitionDetected affected_peers={affected_peers} reason={reason}"
            ),
            TransportEvent::PartitionRecovered {
                recovered_peers,
                reason,
            } => format!(
                "[VOIDNET][MESH] PartitionRecovered recovered_peers={recovered_peers} reason={reason}"
            ),
            TransportEvent::Ping {
                peer_id,
                latency_ms,
            } => format!("[VOIDNET][TRANSPORT] Ping peer={peer_id} latency={latency_ms}ms"),
            TransportEvent::PeerStateChanged { peer_id, state } => {
                format!("[VOIDNET][MESH] PeerStateChanged peer={peer_id} state={state}")
            }
            TransportEvent::LifecycleTransition { from, to, reason } => format!(
                "[VOIDNET][LIFECYCLE] Transition from={from} to={to} reason={reason}"
            ),
            TransportEvent::TopologyPersisted { path } => {
                format!("[VOIDNET][MESH] TopologyPersisted path={}", path.display())
            }
            TransportEvent::Shutdown { state } => {
                format!("[VOIDNET][LIFECYCLE] Shutdown state={state}")
            }
        }
    }
}
