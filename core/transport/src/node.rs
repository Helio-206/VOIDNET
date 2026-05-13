use crate::{
    event::{DiscoverySource, EventBus, RuntimeMeshAnnouncement, TransportEvent},
    is_quic_address,
    lifecycle::{LifecycleEngine, NodeLifecycleState},
    topology::{DnsTopologyInfo, MeshState, PeerRuntimeInfo, PeerTopology, TransportHealth},
    NetworkConfig, TransportError, RUNTIME_MESH_TOPIC, VOID_AGENT_VERSION, VOID_DNS_TOPIC,
    VOID_IDENTIFY_PROTOCOL,
};
use futures::StreamExt;
use libp2p::{
    gossipsub, identify, identity, mdns, ping,
    swarm::{NetworkBehaviour, SwarmEvent},
    Multiaddr, PeerId, Swarm, SwarmBuilder,
};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::OpenOptions,
    io::Write,
    path::PathBuf,
    time::{Duration, Instant},
};
use tokio::{select, signal, time};
use void_chat::{
    decrypt_payload, deserialize_payload, direct_peer_from_topic, direct_topic, drain_local_commands,
    encrypt_payload, is_room_topic, load_chat_inbox, load_chat_notifications, load_chat_rooms, now_unix_ms,
    mark_inbox_read, mark_notifications_read, merge_room_snapshot,
    public_key_from_secret, random_ephemeral_secret, room_from_topic, room_topic, save_chat_inbox,
    save_chat_notifications, save_chat_rooms, save_chat_sessions, serialize_payload, set_current_room,
    set_local_room_joined, unread_count, upsert_room_member, ChatInboxEntry, ChatInboxState,
    ChatLocalCommand, ChatMessageType, ChatNotificationsState, ChatRoomSnapshot,
    ChatRoomsState, ChatSessionSnapshot, ChatSessionState, ChatSessionsState, ChatTextPayload,
    ChatWireMessage, ReplayProtector, RoomMembershipAction, RoomMembershipEvent, RoomStateSnapshot,
    SessionAck, SessionOffer, SignedEncryptedEnvelope, CHAT_REPLAY_WINDOW_SECS, push_notification,
    record_room_event,
};
use void_dns::{
    drain_dns_commands, DnsApplyOutcome, DnsCommand, DnsRecordSource, DnsPropagationMessage,
    PersistentVoidDns, VoidDomain,
};
use void_identity::PersistentNodeIdentity;
use void_protocol::VoidUri;
use x25519_dalek::EphemeralSecret;

const CHAT_COMMAND_INTERVAL: Duration = Duration::from_millis(250);
const CHAT_ROOM_SYNC_INTERVAL: Duration = Duration::from_secs(2);
const DNS_COMMAND_INTERVAL: Duration = Duration::from_millis(250);
const DNS_EVENT_TTL: Duration = Duration::from_secs(600);

#[derive(Debug, Clone)]
pub struct TransportNodeConfig {
    pub data_dir: PathBuf,
    pub network: NetworkConfig,
    pub topology_file: PathBuf,
    pub event_log_file: PathBuf,
    pub event_buffer: usize,
    pub reconnect_interval: Duration,
    pub partition_after: Duration,
    pub exit_after: Option<Duration>,
    pub enable_mdns: bool,
}

impl TransportNodeConfig {
    pub fn new(data_dir: PathBuf, network: NetworkConfig) -> Self {
        let topology_file = data_dir.join("topology.json");
        let event_log_file = data_dir.join("events.log");
        Self {
            data_dir,
            network,
            topology_file,
            event_log_file,
            event_buffer: 1024,
            reconnect_interval: Duration::from_secs(8),
            partition_after: Duration::from_secs(30),
            exit_after: None,
            enable_mdns: true,
        }
    }
}

#[derive(NetworkBehaviour)]
#[behaviour(to_swarm = "VoidBehaviourEvent")]
struct VoidBehaviour {
    gossipsub: gossipsub::Behaviour,
    identify: identify::Behaviour,
    ping: ping::Behaviour,
    mdns: mdns::tokio::Behaviour,
}

impl VoidBehaviour {
    fn new(keypair: &identity::Keypair) -> Result<Self, TransportError> {
        let local_peer_id = keypair.public().to_peer_id();
        let mut gossipsub = gossipsub::Behaviour::new(
            gossipsub::MessageAuthenticity::Signed(keypair.clone()),
            gossipsub::Config::default(),
        )
        .map_err(|error| TransportError::Backend(error.to_string()))?;
        gossipsub
            .subscribe(&gossipsub::IdentTopic::new(RUNTIME_MESH_TOPIC))
            .map_err(|error| TransportError::Backend(error.to_string()))?;
        gossipsub
            .subscribe(&gossipsub::IdentTopic::new(VOID_DNS_TOPIC))
            .map_err(|error| TransportError::Backend(error.to_string()))?;
        gossipsub
            .subscribe(&gossipsub::IdentTopic::new(direct_topic(&local_peer_id.to_string())))
            .map_err(|error| TransportError::Backend(error.to_string()))?;

        let identify_config =
            identify::Config::new(VOID_IDENTIFY_PROTOCOL.to_string(), keypair.public())
                .with_agent_version(VOID_AGENT_VERSION.to_string())
                .with_push_listen_addr_updates(true)
                .with_interval(Duration::from_secs(20));

        let mdns = mdns::tokio::Behaviour::new(mdns::Config::default(), local_peer_id)
            .map_err(|error| TransportError::Backend(error.to_string()))?;

        Ok(Self {
            gossipsub,
            identify: identify::Behaviour::new(identify_config),
            ping: ping::Behaviour::default(),
            mdns,
        })
    }
}

#[derive(Debug)]
enum VoidBehaviourEvent {
    Gossipsub(gossipsub::Event),
    Identify(identify::Event),
    Ping(ping::Event),
    Mdns(mdns::Event),
}

impl From<gossipsub::Event> for VoidBehaviourEvent {
    fn from(event: gossipsub::Event) -> Self {
        Self::Gossipsub(event)
    }
}

impl From<identify::Event> for VoidBehaviourEvent {
    fn from(event: identify::Event) -> Self {
        Self::Identify(event)
    }
}

impl From<ping::Event> for VoidBehaviourEvent {
    fn from(event: ping::Event) -> Self {
        Self::Ping(event)
    }
}

impl From<mdns::Event> for VoidBehaviourEvent {
    fn from(event: mdns::Event) -> Self {
        Self::Mdns(event)
    }
}

pub async fn run_transport_node(config: TransportNodeConfig) -> Result<(), TransportError> {
    for address in &config.network.listen {
        if !is_quic_address(address) {
            return Err(TransportError::NonQuicAddress(address.to_string()));
        }
    }

    let event_bus = EventBus::new(config.event_buffer);
    spawn_event_logger(event_bus.clone(), config.event_log_file.clone());
    let started_at = Instant::now();

    let mut lifecycle = LifecycleEngine::new();
    emit_transition(
        &event_bus,
        &mut lifecycle,
        NodeLifecycleState::Bootstrap,
        "loading persistent identity",
    );

    let identity = PersistentNodeIdentity::load_or_create_dir(&config.data_dir)
        .map_err(|error| TransportError::Backend(error.to_string()))?;
    let identity_event = TransportEvent::IdentityLoaded {
        peer_id: identity.peer_id_string(),
        fingerprint: identity.fingerprint().to_string(),
        path: identity.path().to_path_buf(),
    };
    event_bus.emit(identity_event);
    if identity.generated() {
        event_bus.emit(TransportEvent::IdentityPersisted {
            peer_id: identity.peer_id_string(),
            fingerprint: identity.fingerprint().to_string(),
            path: identity.path().to_path_buf(),
        });
    }

    let presence = identity
        .sign_presence(VOID_AGENT_VERSION)
        .map_err(|error| TransportError::Backend(error.to_string()))?;
    presence
        .verify()
        .map_err(|error| TransportError::Backend(error.to_string()))?;

    let local_peer_id = identity.peer_id();
    let local_peer_id_string = local_peer_id.to_string();
    let keypair = identity.keypair().clone();
    let mut topology = PeerTopology::new(local_peer_id.to_string());
    let capabilities = default_runtime_capabilities(config.enable_mdns);
    let mounted_runtime = build_local_runtime_snapshot(
        lifecycle.state(),
        started_at,
        capabilities.clone(),
        TransportHealth::Healthy,
        false,
    );
    topology.set_local_runtime(mounted_runtime.clone());
    event_bus.emit(TransportEvent::RuntimeMounted {
        peer_id: local_peer_id_string.clone(),
        runtime: mounted_runtime,
    });
    for capability in &capabilities {
        event_bus.emit(TransportEvent::CapabilityGranted {
            peer_id: local_peer_id_string.clone(),
            capability: capability.clone(),
            scope: "node".to_string(),
        });
    }
    if !config.enable_mdns {
        event_bus.emit(TransportEvent::CapabilityRejected {
            peer_id: local_peer_id_string.clone(),
            capability: "discovery/mdns".to_string(),
            reason: "disabled by operator".to_string(),
        });
    }

    let mut swarm = SwarmBuilder::with_existing_identity(keypair)
        .with_tokio()
        .with_quic()
        .with_behaviour(|keypair| {
            VoidBehaviour::new(keypair)
                .map_err(|error| -> Box<dyn std::error::Error + Send + Sync> { Box::new(error) })
        })
        .map_err(|error| TransportError::Backend(error.to_string()))?
        .build();

    let mut dns = DnsRuntimeState::new(config.data_dir.clone(), local_peer_id_string.clone())
        .await
        .map_err(|error| TransportError::Backend(error.to_string()))?;
    refresh_dns_topology(&mut topology, &dns).await?;

    let mut chat = ChatRuntimeState::new(config.data_dir.clone(), local_peer_id_string.clone())
        .map_err(|error| TransportError::Backend(error.to_string()))?;
    for topic in chat.subscription_topics() {
        swarm
            .behaviour_mut()
            .gossipsub
            .subscribe(&gossipsub::IdentTopic::new(topic))
            .map_err(|error| TransportError::Backend(error.to_string()))?;
    }
    chat.persist()
        .map_err(|error| TransportError::Backend(error.to_string()))?;
    event_bus.emit(TransportEvent::SessionRecovered {
        peer_id: local_peer_id_string.clone(),
        active_rooms: chat.joined_rooms.len(),
        inbox_messages: chat.inbox.messages.len(),
        unread_messages: unread_count(&chat.inbox, None),
    });
    event_bus.emit(TransportEvent::InboxSynchronized {
        messages: chat.inbox.messages.len(),
        unread_messages: unread_count(&chat.inbox, None),
        room: None,
    });

    for address in &config.network.listen {
        swarm
            .listen_on(address.clone())
            .map_err(|error| TransportError::Backend(error.to_string()))?;
    }

    emit_transition(
        &event_bus,
        &mut lifecycle,
        NodeLifecycleState::Discovering,
        "transport listener active",
    );
    let initial_transport_health = local_transport_health(lifecycle.state(), &topology, false);
    refresh_local_runtime(
        &mut topology,
        lifecycle.state(),
        started_at,
        &capabilities,
        initial_transport_health,
        true,
    );
    if let Some(runtime) = topology.local_runtime.clone() {
        event_bus.emit(TransportEvent::RuntimeReady {
            peer_id: local_peer_id_string.clone(),
            runtime,
        });
    }

    let mut known_dial_addrs: BTreeSet<String> = BTreeSet::new();
    for address in &config.network.bootstrap {
        known_dial_addrs.insert(address.to_string());
        event_bus.emit(TransportEvent::PeerDiscovered {
            peer_id: peer_id_from_addr(address)
                .map(|peer| peer.to_string())
                .unwrap_or_else(|| "unknown".to_string()),
            addresses: vec![address.to_string()],
            source: DiscoverySource::Bootstrap,
        });
        if let Err(error) = swarm.dial(address.clone()) {
            event_bus.emit(TransportEvent::TransportFailed {
                peer_id: peer_id_from_addr(address).map(|peer| peer.to_string()),
                address: Some(address.to_string()),
                error: error.to_string(),
            });
        }
    }

    let mut reconnect_tick = time::interval(config.reconnect_interval);
    reconnect_tick.set_missed_tick_behavior(time::MissedTickBehavior::Delay);
    let mut dns_tick = time::interval(DNS_COMMAND_INTERVAL);
    dns_tick.set_missed_tick_behavior(time::MissedTickBehavior::Delay);
    let mut chat_tick = time::interval(CHAT_COMMAND_INTERVAL);
    chat_tick.set_missed_tick_behavior(time::MissedTickBehavior::Delay);
    let mut runtime_tick = time::interval(Duration::from_secs(5));
    runtime_tick.set_missed_tick_behavior(time::MissedTickBehavior::Delay);
    let mut partition_tick = time::interval(Duration::from_secs(5));
    partition_tick.set_missed_tick_behavior(time::MissedTickBehavior::Delay);
    let mut exit_sleep = config.exit_after.map(|duration| Box::pin(time::sleep(duration)));
    let mut last_active_peer = Instant::now();
    let mut has_seen_mesh = false;

    loop {
        select! {
            _ = signal::ctrl_c() => {
                refresh_local_runtime(
                    &mut topology,
                    NodeLifecycleState::Offline,
                    started_at,
                    &capabilities,
                    TransportHealth::Offline,
                    false,
                );
                emit_transition(&event_bus, &mut lifecycle, NodeLifecycleState::Offline, "shutdown signal");
                if let Some(runtime) = topology.local_runtime.clone() {
                    event_bus.emit(TransportEvent::RuntimeShutdown {
                        peer_id: local_peer_id_string.clone(),
                        runtime,
                    });
                }
                event_bus.emit(TransportEvent::Shutdown { state: lifecycle.state() });
                persist_topology(&event_bus, &topology, &config.topology_file)?;
                break;
            }
            _ = async {
                if let Some(sleep) = exit_sleep.as_mut() {
                    sleep.as_mut().await;
                } else {
                    futures::future::pending::<()>().await;
                }
            } => {
                refresh_local_runtime(
                    &mut topology,
                    NodeLifecycleState::Offline,
                    started_at,
                    &capabilities,
                    TransportHealth::Offline,
                    false,
                );
                emit_transition(&event_bus, &mut lifecycle, NodeLifecycleState::Offline, "exit-after elapsed");
                if let Some(runtime) = topology.local_runtime.clone() {
                    event_bus.emit(TransportEvent::RuntimeShutdown {
                        peer_id: local_peer_id_string.clone(),
                        runtime,
                    });
                }
                event_bus.emit(TransportEvent::Shutdown { state: lifecycle.state() });
                persist_topology(&event_bus, &topology, &config.topology_file)?;
                break;
            }
            _ = runtime_tick.tick() => {
                handle_dns_maintenance_tick(&event_bus, &mut topology, &config.topology_file, &mut dns).await?;
                let transport_health = local_transport_health(lifecycle.state(), &topology, has_seen_mesh);
                refresh_local_runtime(
                    &mut topology,
                    lifecycle.state(),
                    started_at,
                    &capabilities,
                    transport_health,
                    lifecycle.state() != NodeLifecycleState::Offline,
                );
                if let Some(runtime) = topology.local_runtime.clone() {
                    let announcement = RuntimeMeshAnnouncement {
                        peer_id: local_peer_id_string.clone(),
                        runtime,
                        latency_ms: None,
                        encrypted_session_established: topology.encrypted_session_count() > 0,
                    };
                    publish_runtime_announcement(&mut swarm, &announcement);
                }
                persist_topology(&event_bus, &topology, &config.topology_file)?;
            }
            _ = reconnect_tick.tick() => {
                for address in known_dial_addrs.clone() {
                    if let Ok(address) = address.parse::<Multiaddr>() {
                        if let Err(error) = swarm.dial(address.clone()) {
                            let peer_id = peer_id_from_addr(&address).map(|peer| peer.to_string());
                            topology.observe_failure(peer_id.clone(), error.to_string());
                            event_bus.emit(TransportEvent::TransportFailed {
                                peer_id,
                                address: Some(address.to_string()),
                                error: error.to_string(),
                            });
                        } else {
                            event_bus.emit(TransportEvent::PeerDiscovered {
                                peer_id: peer_id_from_addr(&address).map(|peer| peer.to_string()).unwrap_or_else(|| "unknown".to_string()),
                                addresses: vec![address.to_string()],
                                source: DiscoverySource::Reconnect,
                            });
                        }
                    }
                }
            }
            _ = dns_tick.tick() => {
                handle_dns_command_tick(
                    &config.data_dir,
                    &identity,
                    &event_bus,
                    &mut swarm,
                    &mut topology,
                    &config.topology_file,
                    &capabilities,
                    &mut dns,
                ).await?;
            }
            _ = chat_tick.tick() => {
                handle_chat_command_tick(
                    &config.data_dir,
                    &identity,
                    &event_bus,
                    &mut swarm,
                    &mut topology,
                    &config.topology_file,
                    &mut chat,
                )?;
            }
            _ = partition_tick.tick() => {
                if has_seen_mesh && topology.active_peer_count() == 0 && last_active_peer.elapsed() >= config.partition_after {
                    topology.mark_partitioned();
                    emit_mesh_state_change(&event_bus, &mut topology, MeshState::Partitioned, "active peer set empty beyond partition threshold");
                    emit_transition(&event_bus, &mut lifecycle, NodeLifecycleState::Partitioned, "active peer set empty beyond partition threshold");
                    event_bus.emit(TransportEvent::PartitionDetected {
                        affected_peers: topology.known_peer_count(),
                        reason: "active peer set empty".to_string(),
                    });
                    event_bus.emit(TransportEvent::MeshPartitionDetected {
                        affected_peers: topology.known_peer_count(),
                        reason: "active peer set empty".to_string(),
                    });
                    persist_topology(&event_bus, &topology, &config.topology_file)?;
                } else if !has_seen_mesh {
                    emit_mesh_state_change(&event_bus, &mut topology, MeshState::Bootstrapping, "awaiting first active peer");
                    emit_transition(&event_bus, &mut lifecycle, NodeLifecycleState::Active, "local node active, awaiting mesh peers");
                } else if topology.mesh_state == MeshState::Recovering {
                    emit_mesh_state_change(&event_bus, &mut topology, MeshState::Stable, "mesh heartbeat stabilised");
                }
            }
            swarm_event = swarm.select_next_some() => {
                handle_swarm_event(
                    swarm_event,
                    &event_bus,
                    &mut swarm,
                    &mut lifecycle,
                    &mut topology,
                    &mut known_dial_addrs,
                    &config.topology_file,
                    local_peer_id,
                    &local_peer_id_string,
                    &identity,
                    &mut dns,
                    &mut chat,
                    config.enable_mdns,
                    &mut last_active_peer,
                    &mut has_seen_mesh,
                ).await?;
            }
        }
    }

    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn handle_swarm_event(
    swarm_event: SwarmEvent<VoidBehaviourEvent>,
    event_bus: &EventBus,
    swarm: &mut Swarm<VoidBehaviour>,
    lifecycle: &mut LifecycleEngine,
    topology: &mut PeerTopology,
    known_dial_addrs: &mut BTreeSet<String>,
    topology_file: &PathBuf,
    local_peer_id: PeerId,
    local_peer_id_string: &str,
    identity: &PersistentNodeIdentity,
    dns: &mut DnsRuntimeState,
    chat: &mut ChatRuntimeState,
    enable_mdns: bool,
    last_active_peer: &mut Instant,
    has_seen_mesh: &mut bool,
) -> Result<(), TransportError> {
    match swarm_event {
        SwarmEvent::NewListenAddr { address, .. } => {
            event_bus.emit(TransportEvent::Listening {
                address: address.to_string(),
            });
        }
        SwarmEvent::ConnectionEstablished {
            peer_id, endpoint, ..
        } => {
            let address = Some(endpoint.get_remote_address().to_string());
            topology.observe_connected(peer_id.to_string(), address.clone(), "quic-v1");
            event_bus.emit(TransportEvent::TransportConnected {
                peer_id: peer_id.to_string(),
                address,
                transport: "quic-v1".to_string(),
            });
            event_bus.emit(TransportEvent::SessionEncrypted {
                peer_id: peer_id.to_string(),
                transport: "quic-v1".to_string(),
                cipher: "libp2p-quic".to_string(),
            });
            topology.observe_transport_encryption(peer_id.to_string(), "libp2p-quic");
            event_bus.emit(TransportEvent::EncryptedSessionEstablished {
                peer_id: peer_id.to_string(),
                transport: "quic-v1".to_string(),
                cipher: "libp2p-quic".to_string(),
            });
            event_bus.emit(TransportEvent::PeerStateChanged {
                peer_id: peer_id.to_string(),
                state: crate::topology::PeerConnectionState::Authenticating,
            });
            emit_transition(
                event_bus,
                lifecycle,
                NodeLifecycleState::Authenticating,
                "transport connection established",
            );
            persist_topology(event_bus, topology, topology_file)?;
        }
        SwarmEvent::ConnectionClosed { peer_id, cause, .. } => {
            let reason = cause
                .map(|cause| cause.to_string())
                .unwrap_or_else(|| "connection closed".to_string());
            topology.observe_disconnected(peer_id.to_string());
            event_bus.emit(TransportEvent::PeerDisconnected {
                peer_id: peer_id.to_string(),
                reason,
            });
            persist_topology(event_bus, topology, topology_file)?;
        }
        SwarmEvent::OutgoingConnectionError {
            peer_id,
            error,
            ..
        } => {
            let peer_id = peer_id.map(|peer| peer.to_string());
            topology.observe_failure(peer_id.clone(), error.to_string());
            event_bus.emit(TransportEvent::TransportFailed {
                peer_id,
                address: None,
                error: error.to_string(),
            });
            persist_topology(event_bus, topology, topology_file)?;
        }
        SwarmEvent::IncomingConnectionError { error, .. } => {
            event_bus.emit(TransportEvent::TransportFailed {
                peer_id: None,
                address: None,
                error: error.to_string(),
            });
        }
        SwarmEvent::Behaviour(VoidBehaviourEvent::Identify(identify::Event::Received {
            peer_id,
            info,
            ..
        })) => {
            if info.public_key.to_peer_id() != peer_id {
                topology.observe_failure(Some(peer_id.to_string()), "identify public key mismatch");
                emit_transition(
                    event_bus,
                    lifecycle,
                    NodeLifecycleState::Quarantined,
                    "identify public key did not match peer id",
                );
                event_bus.emit(TransportEvent::TransportFailed {
                    peer_id: Some(peer_id.to_string()),
                    address: None,
                    error: "identify public key mismatch".to_string(),
                });
                return Ok(());
            }

            let addresses: Vec<String> = info.listen_addrs.iter().map(ToString::to_string).collect();
            let runtime = PeerRuntimeInfo::new(
                info.agent_version.clone(),
                NodeLifecycleState::Syncing,
                0,
                info.protocols.iter().map(ToString::to_string).collect(),
                TransportHealth::Healthy,
                false,
            );
            topology.observe_runtime_state(peer_id.to_string(), runtime.clone());
            if !addresses.is_empty() {
                topology.observe_discovered(peer_id.to_string(), addresses.clone());
                for address in &info.listen_addrs {
                    known_dial_addrs.insert(address.to_string());
                }
                event_bus.emit(TransportEvent::PeerDiscovered {
                    peer_id: peer_id.to_string(),
                    addresses,
                    source: DiscoverySource::Identify,
                });
            }
            topology.observe_authenticated(peer_id.to_string());
            event_bus.emit(TransportEvent::PeerAuthenticated {
                peer_id: peer_id.to_string(),
                agent: info.agent_version,
                protocols: info.protocols.iter().map(ToString::to_string).collect(),
            });
            event_bus.emit(TransportEvent::PeerRuntimeDiscovered {
                peer_id: peer_id.to_string(),
                runtime,
            });
            event_bus.emit(TransportEvent::PeerStateChanged {
                peer_id: peer_id.to_string(),
                state: crate::topology::PeerConnectionState::Syncing,
            });
            emit_transition(
                event_bus,
                lifecycle,
                NodeLifecycleState::Syncing,
                "peer identity authenticated",
            );
            persist_topology(event_bus, topology, topology_file)?;
        }
        SwarmEvent::Behaviour(VoidBehaviourEvent::Ping(event)) => {
            match event.result {
                Ok(latency) => {
                    topology.observe_latency(event.peer.to_string(), latency.as_millis());
                    topology.observe_active(event.peer.to_string());
                    *last_active_peer = Instant::now();
                    if topology.mesh_state == MeshState::Partitioned || topology.mesh_state == MeshState::Degraded {
                        emit_mesh_state_change(event_bus, topology, MeshState::Recovering, "active peer heartbeat restored");
                        event_bus.emit(TransportEvent::PartitionRecovered {
                            recovered_peers: topology.active_peer_count(),
                            reason: "active peer heartbeat restored".to_string(),
                        });
                    } else {
                        emit_mesh_state_change(event_bus, topology, MeshState::Stable, "active peer heartbeat observed");
                    }
                    *has_seen_mesh = true;
                    event_bus.emit(TransportEvent::Ping {
                        peer_id: event.peer.to_string(),
                        latency_ms: latency.as_millis(),
                    });
                    event_bus.emit(TransportEvent::PeerStateChanged {
                        peer_id: event.peer.to_string(),
                        state: crate::topology::PeerConnectionState::Active,
                    });
                    emit_transition(
                        event_bus,
                        lifecycle,
                        NodeLifecycleState::Active,
                        "ping observed active peer",
                    );
                    persist_topology(event_bus, topology, topology_file)?;
                }
                Err(error) => {
                    topology.observe_failure(Some(event.peer.to_string()), error.to_string());
                    emit_mesh_state_change(event_bus, topology, MeshState::Degraded, "peer ping failed");
                    event_bus.emit(TransportEvent::TransportFailed {
                        peer_id: Some(event.peer.to_string()),
                        address: None,
                        error: error.to_string(),
                    });
                    persist_topology(event_bus, topology, topology_file)?;
                }
            }
        }
        SwarmEvent::Behaviour(VoidBehaviourEvent::Mdns(mdns::Event::Discovered(peers))) => {
            if !enable_mdns {
                return Ok(());
            }
            for (peer_id, address) in peers {
                if peer_id == local_peer_id {
                    continue;
                }
                topology.observe_discovered(peer_id.to_string(), vec![address.to_string()]);
                known_dial_addrs.insert(address.to_string());
                if let Err(error) = swarm.dial(address.clone()) {
                    topology.observe_failure(Some(peer_id.to_string()), error.to_string());
                    event_bus.emit(TransportEvent::TransportFailed {
                        peer_id: Some(peer_id.to_string()),
                        address: Some(address.to_string()),
                        error: error.to_string(),
                    });
                }
                event_bus.emit(TransportEvent::PeerDiscovered {
                    peer_id: peer_id.to_string(),
                    addresses: vec![address.to_string()],
                    source: DiscoverySource::Mdns,
                });
            }
            persist_topology(event_bus, topology, topology_file)?;
        }
        SwarmEvent::Behaviour(VoidBehaviourEvent::Mdns(mdns::Event::Expired(peers))) => {
            if !enable_mdns {
                return Ok(());
            }
            for (peer_id, _) in peers {
                topology.observe_disconnected(peer_id.to_string());
                event_bus.emit(TransportEvent::PeerDisconnected {
                    peer_id: peer_id.to_string(),
                    reason: "mdns record expired".to_string(),
                });
            }
            persist_topology(event_bus, topology, topology_file)?;
        }
        SwarmEvent::Behaviour(VoidBehaviourEvent::Gossipsub(gossipsub::Event::Message {
            propagation_source,
            message,
            ..
        })) => {
            let topic = message.topic.to_string();
            if topic == VOID_DNS_TOPIC {
                handle_dns_wire_message(
                    &message.data,
                    propagation_source,
                    event_bus,
                    topology,
                    topology_file,
                    dns,
                    local_peer_id_string,
                ).await?;
                return Ok(());
            }
            if topic != RUNTIME_MESH_TOPIC {
                handle_chat_wire_message(
                    &topic,
                    &message.data,
                    propagation_source,
                    identity,
                    event_bus,
                    swarm,
                    topology,
                    topology_file,
                    chat,
                    local_peer_id_string,
                )?;
                return Ok(());
            }

            let announcement = match serde_json::from_slice::<RuntimeMeshAnnouncement>(&message.data) {
                Ok(announcement) => announcement,
                Err(error) => {
                    topology.observe_failure(Some(propagation_source.to_string()), error.to_string());
                    event_bus.emit(TransportEvent::TransportFailed {
                        peer_id: Some(propagation_source.to_string()),
                        address: None,
                        error: format!("invalid runtime mesh announcement: {error}"),
                    });
                    return Ok(());
                }
            };

            if announcement.peer_id == local_peer_id_string {
                return Ok(());
            }

            let peer_id = announcement.peer_id.clone();
            if let Some(latency_ms) = announcement.latency_ms {
                topology.observe_latency(peer_id.clone(), latency_ms);
            }
            if announcement.encrypted_session_established {
                topology.observe_transport_encryption(peer_id.clone(), "mesh-runtime");
            }
            topology.observe_runtime_state(peer_id.clone(), announcement.runtime.clone());
            event_bus.emit(TransportEvent::PeerRuntimeDiscovered {
                peer_id,
                runtime: announcement.runtime,
            });
            persist_topology(event_bus, topology, topology_file)?;
        }
        SwarmEvent::Behaviour(VoidBehaviourEvent::Gossipsub(
            gossipsub::Event::GossipsubNotSupported { peer_id },
        )) => {
            topology.observe_failure(Some(peer_id.to_string()), "peer does not support gossipsub runtime topic");
            event_bus.emit(TransportEvent::CapabilityRejected {
                peer_id: peer_id.to_string(),
                capability: "mesh/runtime-events".to_string(),
                reason: "peer does not support gossipsub runtime topic".to_string(),
            });
            persist_topology(event_bus, topology, topology_file)?;
        }
        SwarmEvent::Behaviour(VoidBehaviourEvent::Identify(identify::Event::Error {
            peer_id,
            error,
            ..
        })) => {
            topology.observe_failure(Some(peer_id.to_string()), error.to_string());
            event_bus.emit(TransportEvent::TransportFailed {
                peer_id: Some(peer_id.to_string()),
                address: None,
                error: error.to_string(),
            });
            persist_topology(event_bus, topology, topology_file)?;
        }
        _ => {}
    }

    Ok(())
}

fn emit_transition(
    event_bus: &EventBus,
    lifecycle: &mut LifecycleEngine,
    to: NodeLifecycleState,
    reason: impl Into<String>,
) {
    if let Some(transition) = lifecycle.transition(to, reason) {
        event_bus.emit(TransportEvent::LifecycleTransition {
            from: transition.from,
            to: transition.to,
            reason: transition.reason,
        });
    }
}

fn persist_topology(
    event_bus: &EventBus,
    topology: &PeerTopology,
    topology_file: &PathBuf,
) -> Result<(), TransportError> {
    topology
        .save(topology_file)
        .map_err(|error| TransportError::Backend(error.to_string()))?;
    event_bus.emit(TransportEvent::TopologyPersisted {
        path: topology_file.clone(),
    });
    Ok(())
}

fn build_local_runtime_snapshot(
    node_state: NodeLifecycleState,
    started_at: Instant,
    capabilities: Vec<String>,
    transport_health: TransportHealth,
    runtime_ready: bool,
) -> PeerRuntimeInfo {
    PeerRuntimeInfo::new(
        VOID_AGENT_VERSION,
        node_state,
        started_at.elapsed().as_secs(),
        capabilities,
        transport_health,
        runtime_ready,
    )
}

fn default_runtime_capabilities(enable_mdns: bool) -> Vec<String> {
    let mut capabilities = vec![
        "chat/direct-e2ee".to_string(),
        "chat/room-membership".to_string(),
        "transport/quic-v1".to_string(),
        "transport/identify".to_string(),
        "transport/ping".to_string(),
        "dns/cache".to_string(),
        "dns/propagation".to_string(),
        "mesh/runtime-events".to_string(),
        "mesh/topology-persistence".to_string(),
        "routing/void-uri".to_string(),
        "runtime/mount".to_string(),
        "runtime/diagnostics".to_string(),
        "security/encrypted-transport".to_string(),
    ];
    if enable_mdns {
        capabilities.push("discovery/mdns".to_string());
    }
    capabilities
}

fn local_transport_health(
    lifecycle_state: NodeLifecycleState,
    topology: &PeerTopology,
    has_seen_mesh: bool,
) -> TransportHealth {
    match lifecycle_state {
        NodeLifecycleState::Offline => TransportHealth::Offline,
        NodeLifecycleState::Partitioned => TransportHealth::Partitioned,
        NodeLifecycleState::Quarantined => TransportHealth::Degraded,
        _ if has_seen_mesh && topology.active_peer_count() == 0 => TransportHealth::Degraded,
        _ => TransportHealth::Healthy,
    }
}

fn refresh_local_runtime(
    topology: &mut PeerTopology,
    lifecycle_state: NodeLifecycleState,
    started_at: Instant,
    capabilities: &[String],
    transport_health: TransportHealth,
    runtime_ready: bool,
) {
    if topology.local_runtime.is_none() {
        topology.set_local_runtime(build_local_runtime_snapshot(
            lifecycle_state,
            started_at,
            capabilities.to_vec(),
            transport_health,
            runtime_ready,
        ));
        return;
    }

    topology.refresh_local_runtime(
        started_at.elapsed().as_secs(),
        lifecycle_state,
        transport_health,
        runtime_ready,
    );
}

fn emit_mesh_state_change(
    event_bus: &EventBus,
    topology: &mut PeerTopology,
    state: MeshState,
    reason: impl Into<String>,
) {
    let reason = reason.into();
    if topology.set_mesh_state(state) {
        event_bus.emit(TransportEvent::MeshStateChanged { state, reason });
    }
}

fn publish_runtime_announcement(
    swarm: &mut Swarm<VoidBehaviour>,
    announcement: &RuntimeMeshAnnouncement,
) {
    let payload = match serde_json::to_vec(announcement) {
        Ok(payload) => payload,
        Err(_) => return,
    };

    let _ = swarm
        .behaviour_mut()
        .gossipsub
        .publish(gossipsub::IdentTopic::new(RUNTIME_MESH_TOPIC), payload);
}

fn spawn_event_logger(event_bus: EventBus, event_log_file: PathBuf) {
    let mut events = event_bus.subscribe();
    tokio::spawn(async move {
        let mut event_log = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&event_log_file)
            .ok();
        loop {
            match events.recv().await {
                Ok(event) => {
                    let line = event.log_line();
                    println!("{}", line);
                    if let Some(file) = event_log.as_mut() {
                        let _ = writeln!(file, "{}|{}", now_unix_ms(), line);
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                    let line = format!("[VOIDNET][EVENT] EventLag skipped={skipped}");
                    println!("{}", line);
                    if let Some(file) = event_log.as_mut() {
                        let _ = writeln!(file, "{}|{}", now_unix_ms(), line);
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            }
        }
    });
}

fn peer_id_from_addr(address: &Multiaddr) -> Option<PeerId> {
    address.iter().find_map(|protocol| match protocol {
        libp2p::multiaddr::Protocol::P2p(peer_id) => Some(peer_id),
        _ => None,
    })
}

struct DnsRuntimeState {
    store: PersistentVoidDns,
    published_domains: BTreeSet<String>,
    seen_events: BTreeMap<String, u128>,
    last_resolution_latency_ms: Option<u128>,
}

impl DnsRuntimeState {
    async fn new(data_dir: PathBuf, local_peer_id: String) -> Result<Self, void_dns::VoidDnsError> {
        let store = PersistentVoidDns::load_or_create(data_dir)?;
        let published_domains = store
            .list_records()
            .await
            .into_iter()
            .filter(|entry| entry.record.owner_peer_id == local_peer_id)
            .map(|entry| entry.record.domain.to_string())
            .collect();
        Ok(Self {
            store,
            published_domains,
            seen_events: BTreeMap::new(),
            last_resolution_latency_ms: None,
        })
    }

    fn remember_event(&mut self, event_id: &str) -> bool {
        let now = now_unix_ms();
        let lower = now.saturating_sub(DNS_EVENT_TTL.as_millis());
        self.seen_events.retain(|_, seen_at| *seen_at >= lower);
        if self.seen_events.contains_key(event_id) {
            return false;
        }
        self.seen_events.insert(event_id.to_string(), now);
        true
    }
}

async fn handle_dns_command_tick(
    data_dir: &PathBuf,
    identity: &PersistentNodeIdentity,
    event_bus: &EventBus,
    swarm: &mut Swarm<VoidBehaviour>,
    topology: &mut PeerTopology,
    topology_file: &PathBuf,
    runtime_capabilities: &[String],
    dns: &mut DnsRuntimeState,
) -> Result<(), TransportError> {
    let commands = drain_dns_commands(data_dir)
        .map_err(|error| TransportError::Backend(error.to_string()))?;
    if commands.is_empty() {
        return Ok(());
    }

    for command in commands {
        match command.command {
            DnsCommand::Publish {
                domain,
                runtime_surface,
                target_peer_id,
                capabilities,
                ttl_secs,
            } => {
                let runtime_surface = runtime_surface.unwrap_or_else(|| derive_runtime_surface(&domain));
                let capabilities = if capabilities.is_empty() {
                    default_dns_service_capabilities(&runtime_surface, runtime_capabilities)
                } else {
                    capabilities
                };
                let target_peer_id = target_peer_id.unwrap_or_else(|| identity.peer_id_string());
                match dns
                    .store
                    .publish_record(
                        identity,
                        domain.clone(),
                        target_peer_id.clone(),
                        runtime_surface.clone(),
                        capabilities,
                        Duration::from_secs(ttl_secs),
                    )
                    .await
                {
                    Ok((record, outcome)) => {
                        dns.published_domains.insert(record.domain.to_string());
                        emit_dns_apply_outcome(event_bus, &outcome);
                        if matches!(outcome, DnsApplyOutcome::Published(_) | DnsApplyOutcome::Updated(_)) {
                            let propagation = PersistentVoidDns::propagation_message(record.clone())
                                .map_err(|error| TransportError::Backend(error.to_string()))?;
                            dns.remember_event(&propagation.event_id);
                            publish_dns_propagation(swarm, &propagation)?;
                            emit_dns_resolution_probe(event_bus, dns, &record.domain).await?;
                        }
                    }
                    Err(error) => {
                        event_bus.emit(TransportEvent::DnsRecordRejected {
                            domain: domain.to_string(),
                            reason: error.to_string(),
                        });
                    }
                }
            }
        }
    }

    refresh_dns_topology(topology, dns).await?;
    persist_topology(event_bus, topology, topology_file)?;
    Ok(())
}

async fn handle_dns_maintenance_tick(
    event_bus: &EventBus,
    topology: &mut PeerTopology,
    topology_file: &PathBuf,
    dns: &mut DnsRuntimeState,
) -> Result<(), TransportError> {
    let expired = dns
        .store
        .purge_expired()
        .await
        .map_err(|error| TransportError::Backend(error.to_string()))?;
    if expired.is_empty() {
        return Ok(());
    }

    for record in expired {
        dns.published_domains.remove(record.domain.as_str());
        event_bus.emit(TransportEvent::DnsRecordExpired {
            domain: record.domain.to_string(),
            owner_peer_id: record.owner_peer_id,
        });
    }
    refresh_dns_topology(topology, dns).await?;
    persist_topology(event_bus, topology, topology_file)?;
    Ok(())
}

async fn handle_dns_wire_message(
    data: &[u8],
    propagation_source: PeerId,
    event_bus: &EventBus,
    topology: &mut PeerTopology,
    topology_file: &PathBuf,
    dns: &mut DnsRuntimeState,
    local_peer_id_string: &str,
) -> Result<(), TransportError> {
    let message = match serde_json::from_slice::<DnsPropagationMessage>(data) {
        Ok(message) => message,
        Err(error) => {
            event_bus.emit(TransportEvent::DnsRecordRejected {
                domain: "unknown".to_string(),
                reason: format!("invalid dns propagation payload from {propagation_source}: {error}"),
            });
            return Ok(());
        }
    };

    if message.record.owner_peer_id == local_peer_id_string || !dns.remember_event(&message.event_id) {
        return Ok(());
    }

    let domain = message.record.domain.clone();
    match dns
        .store
        .apply_record(message.record, DnsRecordSource::MeshPropagation)
        .await
    {
        Ok(outcome) => {
            emit_dns_apply_outcome(event_bus, &outcome);
            if matches!(outcome, DnsApplyOutcome::Published(_) | DnsApplyOutcome::Updated(_)) {
                emit_dns_resolution_probe(event_bus, dns, &domain).await?;
            }
        }
        Err(error) => {
            event_bus.emit(TransportEvent::DnsRecordRejected {
                domain: domain.to_string(),
                reason: error.to_string(),
            });
        }
    }
    refresh_dns_topology(topology, dns).await?;
    persist_topology(event_bus, topology, topology_file)?;
    Ok(())
}

async fn refresh_dns_topology(
    topology: &mut PeerTopology,
    dns: &DnsRuntimeState,
) -> Result<(), TransportError> {
    let records = dns.store.list_records().await;
    let conflicts = dns.store.list_conflicts().await;
    let now = now_unix_ms();
    let active_records = records
        .iter()
        .filter(|entry| !entry.record.is_expired(now))
        .count();
    let registrations = dns.published_domains.iter().cloned().collect::<Vec<_>>();
    topology.set_dns_state(DnsTopologyInfo::new(
        records.len(),
        active_records,
        conflicts.len(),
        registrations,
        dns.last_resolution_latency_ms,
    ));
    Ok(())
}

async fn emit_dns_resolution_probe(
    event_bus: &EventBus,
    dns: &mut DnsRuntimeState,
    domain: &VoidDomain,
) -> Result<(), TransportError> {
    let uri = VoidUri::new(domain.to_string(), "/", None)
        .map_err(|error| TransportError::Backend(error.to_string()))?;
    match dns
        .store
        .resolve_route(&uri)
        .await
        .map_err(|error| TransportError::Backend(error.to_string()))?
    {
        Some(route) => {
            dns.last_resolution_latency_ms = Some(route.resolution_latency_ms);
            event_bus.emit(TransportEvent::DnsResolutionSucceeded {
                domain: domain.to_string(),
                target_peer_id: route.target_peer_id,
                runtime_surface: route.runtime_surface,
                latency_ms: route.resolution_latency_ms,
            });
        }
        None => {
            event_bus.emit(TransportEvent::DnsResolutionFailed {
                domain: domain.to_string(),
                reason: "no active route found in local dns cache".to_string(),
            });
        }
    }
    Ok(())
}

fn emit_dns_apply_outcome(event_bus: &EventBus, outcome: &DnsApplyOutcome) {
    match outcome {
        DnsApplyOutcome::Published(record) => event_bus.emit(TransportEvent::DnsRecordPublished {
            domain: record.domain.to_string(),
            owner_peer_id: record.owner_peer_id.clone(),
            target_peer_id: record.target_peer_id.clone(),
            runtime_surface: record.runtime_surface.clone(),
            ttl_secs: record.ttl_remaining_secs(now_unix_ms()),
        }),
        DnsApplyOutcome::Updated(record) => event_bus.emit(TransportEvent::DnsRecordUpdated {
            domain: record.domain.to_string(),
            owner_peer_id: record.owner_peer_id.clone(),
            target_peer_id: record.target_peer_id.clone(),
            runtime_surface: record.runtime_surface.clone(),
            ttl_secs: record.ttl_remaining_secs(now_unix_ms()),
        }),
        DnsApplyOutcome::Duplicate(_) => {}
        DnsApplyOutcome::Conflict { conflict, .. } => {
            event_bus.emit(TransportEvent::DnsConflictDetected {
                domain: conflict.domain.to_string(),
                active_owner_peer_id: conflict.active_owner_peer_id.clone(),
                conflicting_owner_peer_id: conflict.conflicting_owner_peer_id.clone(),
            });
        }
    }
}

fn publish_dns_propagation(
    swarm: &mut Swarm<VoidBehaviour>,
    propagation: &DnsPropagationMessage,
) -> Result<(), TransportError> {
    let payload = serde_json::to_vec(propagation)
        .map_err(|error| TransportError::Backend(error.to_string()))?;
    swarm
        .behaviour_mut()
        .gossipsub
        .publish(gossipsub::IdentTopic::new(VOID_DNS_TOPIC), payload)
        .map_err(|error| TransportError::Backend(error.to_string()))?;
    Ok(())
}

fn derive_runtime_surface(domain: &VoidDomain) -> String {
    domain.as_str().trim_end_matches(".void").to_string()
}

fn default_dns_service_capabilities(
    runtime_surface: &str,
    runtime_capabilities: &[String],
) -> Vec<String> {
    let mut capabilities = vec![
        "dns/addressable".to_string(),
        "routing/void-uri".to_string(),
        format!("service/{runtime_surface}"),
    ];
    if runtime_capabilities.iter().any(|capability| capability == "chat/direct-e2ee") {
        capabilities.push("chat/direct-e2ee".to_string());
    }
    capabilities.sort();
    capabilities.dedup();
    capabilities
}

struct PendingChatSession {
    peer_id: String,
    session_id: String,
    secret: EphemeralSecret,
    started_at_unix_ms: u128,
}

#[derive(Debug, Clone)]
struct PendingOutboundMessage {
    room: Option<String>,
    body: String,
}

#[derive(Debug, Clone)]
struct ActiveChatSession {
    key: [u8; 32],
    snapshot: ChatSessionSnapshot,
}

struct ChatRuntimeState {
    data_dir: PathBuf,
    local_peer_id: String,
    joined_rooms: BTreeSet<String>,
    inbox: ChatInboxState,
    notifications: ChatNotificationsState,
    rooms: ChatRoomsState,
    pending_sessions: BTreeMap<String, PendingChatSession>,
    active_sessions: BTreeMap<String, ActiveChatSession>,
    pending_messages: BTreeMap<String, Vec<PendingOutboundMessage>>,
    replay: ReplayProtector,
    last_room_sync_unix_ms: u128,
}

#[derive(Debug, Clone)]
enum ChatSwarmAction {
    Subscribe(String),
    Unsubscribe(String),
    Publish { topic: String, message: ChatWireMessage },
}

impl ChatRuntimeState {
    fn new(data_dir: PathBuf, local_peer_id: String) -> Result<Self, void_chat::ChatError> {
        let rooms = load_chat_rooms(&data_dir)?;
        let inbox = load_chat_inbox(&data_dir)?;
        let notifications = load_chat_notifications(&data_dir)?;
        let joined_rooms = rooms
            .rooms
            .iter()
            .filter(|room| room.joined)
            .map(|room| room.room.clone())
            .collect();
        Ok(Self {
            data_dir,
            local_peer_id,
            joined_rooms,
            inbox,
            notifications,
            rooms,
            pending_sessions: BTreeMap::new(),
            active_sessions: BTreeMap::new(),
            pending_messages: BTreeMap::new(),
            replay: ReplayProtector::new(Duration::from_secs(CHAT_REPLAY_WINDOW_SECS)),
            last_room_sync_unix_ms: 0,
        })
    }

    fn subscription_topics(&self) -> Vec<String> {
        self.joined_rooms.iter().map(|room| room_topic(room)).collect()
    }

    fn persist(&self) -> Result<(), void_chat::ChatError> {
        save_chat_inbox(&self.data_dir, &self.inbox)?;
        save_chat_notifications(&self.data_dir, &self.notifications)?;
        save_chat_rooms(&self.data_dir, &self.rooms)?;
        save_chat_sessions(
            &self.data_dir,
            &ChatSessionsState {
                sessions: self.snapshot_sessions(),
            },
        )?;
        Ok(())
    }

    fn snapshot_sessions(&self) -> Vec<ChatSessionSnapshot> {
        let mut snapshots = self
            .active_sessions
            .values()
            .map(|session| session.snapshot.clone())
            .collect::<Vec<_>>();
        snapshots.extend(self.pending_sessions.values().map(|pending| ChatSessionSnapshot {
            peer_id: pending.peer_id.clone(),
            session_id: pending.session_id.clone(),
            established_at_unix_ms: pending.started_at_unix_ms,
            encryption_state: ChatSessionState::Negotiating,
            last_activity_unix_ms: pending.started_at_unix_ms,
            transport_state: "GOSSIPSUB-DIRECT".to_string(),
            last_error: None,
        }));
        snapshots.sort_by(|left, right| left.peer_id.cmp(&right.peer_id));
        snapshots
    }
}

fn handle_chat_command_tick(
    data_dir: &PathBuf,
    identity: &PersistentNodeIdentity,
    event_bus: &EventBus,
    swarm: &mut Swarm<VoidBehaviour>,
    topology: &mut PeerTopology,
    topology_file: &PathBuf,
    chat: &mut ChatRuntimeState,
) -> Result<(), TransportError> {
    let commands = drain_local_commands(data_dir)
        .map_err(|error| TransportError::Backend(error.to_string()))?;
    let mut actions = Vec::new();
    let mut changed = false;

    for command in commands {
        actions.extend(process_chat_command(identity, event_bus, topology, chat, command.command)?);
        changed = true;
    }
    let room_sync_actions = build_room_sync_actions(identity, event_bus, chat)?;
    if !room_sync_actions.is_empty() {
        actions.extend(room_sync_actions);
        changed = true;
    }

    if !changed && actions.is_empty() {
        return Ok(());
    }

    apply_chat_actions(swarm, event_bus, topology, chat, actions)?;

    chat.persist()
        .map_err(|error| TransportError::Backend(error.to_string()))?;
    persist_topology(event_bus, topology, topology_file)?;
    Ok(())
}

fn process_chat_command(
    identity: &PersistentNodeIdentity,
    event_bus: &EventBus,
    topology: &mut PeerTopology,
    chat: &mut ChatRuntimeState,
    command: ChatLocalCommand,
) -> Result<Vec<ChatSwarmAction>, TransportError> {
    match command {
        ChatLocalCommand::SendDirect { peer_id, message } => {
            append_local_history(chat, None, message.clone(), "local-direct");
            dispatch_or_queue_outbound_message(
                identity,
                event_bus,
                topology,
                chat,
                &peer_id,
                PendingOutboundMessage {
                    room: None,
                    body: message,
                },
            )
        }
        ChatLocalCommand::SendRoom { room, message } => {
            set_current_room(&mut chat.rooms, Some(room.clone()));
            append_local_history(chat, Some(room.clone()), message.clone(), "local-room");
            record_room_event(
                &mut chat.rooms,
                &room,
                "room.message",
                &chat.local_peer_id,
                Some(message.clone()),
            );
            let recipients = room_recipients(chat, &room);
            let mut actions = Vec::new();
            for peer_id in recipients {
                actions.extend(dispatch_or_queue_outbound_message(
                    identity,
                    event_bus,
                    topology,
                    chat,
                    &peer_id,
                    PendingOutboundMessage {
                        room: Some(room.clone()),
                        body: message.clone(),
                    },
                )?);
            }
            event_bus.emit(TransportEvent::InboxSynchronized {
                messages: chat.inbox.messages.len(),
                unread_messages: unread_count(&chat.inbox, None),
                room: Some(room),
            });
            Ok(actions)
        }
        ChatLocalCommand::Join { room } => {
            if chat.joined_rooms.insert(room.clone()) {
                set_local_room_joined(&mut chat.rooms, &room, true);
                upsert_room_member(&mut chat.rooms, &room, &chat.local_peer_id, true);
            }
            record_room_event(&mut chat.rooms, &room, "room.join", &chat.local_peer_id, None);
            event_bus.emit(TransportEvent::RoomMembershipChanged {
                room: room.clone(),
                peer_id: chat.local_peer_id.clone(),
                action: "join".to_string(),
            });
            event_bus.emit(TransportEvent::RoomJoined {
                room: room.clone(),
                peer_id: chat.local_peer_id.clone(),
            });
            let unread_notifications = push_notification(
                &mut chat.notifications,
                "room.join",
                Some(&room),
                Some(&chat.local_peer_id),
                format!("joined room {room}"),
            );
            event_bus.emit(TransportEvent::NotificationRaised {
                kind: "room.join".to_string(),
                room: Some(room.clone()),
                peer_id: Some(chat.local_peer_id.clone()),
                unread_notifications,
            });
            let membership = RoomMembershipEvent::new(identity, room.clone(), RoomMembershipAction::Join)
                .map_err(|error| TransportError::Backend(error.to_string()))?;
            let mut actions = vec![
                ChatSwarmAction::Subscribe(room_topic(&room)),
                ChatSwarmAction::Publish {
                    topic: room_topic(&room),
                    message: ChatWireMessage::RoomMembership(membership),
                },
            ];
            if let Some(action) = build_room_state_action(identity, event_bus, chat, &room, "join")? {
                actions.push(action);
            }
            Ok(actions)
        }
        ChatLocalCommand::Leave { room } => {
            chat.joined_rooms.remove(&room);
            set_local_room_joined(&mut chat.rooms, &room, false);
            upsert_room_member(&mut chat.rooms, &room, &chat.local_peer_id, false);
            record_room_event(&mut chat.rooms, &room, "room.leave", &chat.local_peer_id, None);
            event_bus.emit(TransportEvent::RoomMembershipChanged {
                room: room.clone(),
                peer_id: chat.local_peer_id.clone(),
                action: "leave".to_string(),
            });
            event_bus.emit(TransportEvent::RoomLeft {
                room: room.clone(),
                peer_id: chat.local_peer_id.clone(),
            });
            let unread_notifications = push_notification(
                &mut chat.notifications,
                "room.leave",
                Some(&room),
                Some(&chat.local_peer_id),
                format!("left room {room}"),
            );
            event_bus.emit(TransportEvent::NotificationRaised {
                kind: "room.leave".to_string(),
                room: Some(room.clone()),
                peer_id: Some(chat.local_peer_id.clone()),
                unread_notifications,
            });
            let membership = RoomMembershipEvent::new(identity, room.clone(), RoomMembershipAction::Leave)
                .map_err(|error| TransportError::Backend(error.to_string()))?;
            Ok(vec![
                ChatSwarmAction::Publish {
                    topic: room_topic(&room),
                    message: ChatWireMessage::RoomMembership(membership),
                },
                ChatSwarmAction::Unsubscribe(room_topic(&room)),
            ])
        }
        ChatLocalCommand::SwitchRoom { room } => {
            set_current_room(&mut chat.rooms, Some(room.clone()));
            let cleared_messages = mark_inbox_read(&mut chat.inbox, Some(&room));
            let cleared_notifications = mark_notifications_read(&mut chat.notifications, Some(&room));
            event_bus.emit(TransportEvent::InboxSynchronized {
                messages: chat.inbox.messages.len(),
                unread_messages: unread_count(&chat.inbox, None),
                room: Some(room.clone()),
            });
            if cleared_notifications > 0 {
                event_bus.emit(TransportEvent::NotificationRaised {
                    kind: "room.switch".to_string(),
                    room: Some(room.clone()),
                    peer_id: Some(chat.local_peer_id.clone()),
                    unread_notifications: chat.notifications.notifications.iter().filter(|entry| entry.unread).count(),
                });
            }
            Ok(if cleared_messages > 0 { build_room_sync_actions(identity, event_bus, chat)? } else { Vec::new() })
        }
        ChatLocalCommand::MarkRead { room } => {
            let room_ref = room.as_deref();
            mark_inbox_read(&mut chat.inbox, room_ref);
            mark_notifications_read(&mut chat.notifications, room_ref);
            event_bus.emit(TransportEvent::InboxSynchronized {
                messages: chat.inbox.messages.len(),
                unread_messages: unread_count(&chat.inbox, None),
                room,
            });
            Ok(Vec::new())
        }
    }
}

fn build_direct_message_action(
    identity: &PersistentNodeIdentity,
    peer_id: &str,
    outbound: &PendingOutboundMessage,
    session: &mut ActiveChatSession,
) -> Result<ChatSwarmAction, TransportError> {
    let payload = ChatTextPayload {
        room: outbound.room.clone(),
        body: outbound.body.clone(),
    };
    let encrypted_payload = encrypt_payload(
        &session.key,
        &serde_json::to_vec(&payload).map_err(|error| TransportError::Backend(error.to_string()))?,
    )
    .map_err(|error| TransportError::Backend(error.to_string()))?;
    let envelope = SignedEncryptedEnvelope::new(
        identity,
        peer_id.to_string(),
        session.snapshot.session_id.clone(),
        if outbound.room.is_some() {
            ChatMessageType::RoomText
        } else {
            ChatMessageType::DirectText
        },
        &serialize_payload(&encrypted_payload)
            .map_err(|error| TransportError::Backend(error.to_string()))?,
    )
    .map_err(|error| TransportError::Backend(error.to_string()))?;
    session.snapshot.last_activity_unix_ms = now_unix_ms();
    Ok(ChatSwarmAction::Publish {
        topic: direct_topic(peer_id),
        message: ChatWireMessage::DirectMessage(envelope),
    })
}

#[allow(clippy::too_many_arguments)]
fn handle_chat_wire_message(
    topic: &str,
    data: &[u8],
    propagation_source: PeerId,
    identity: &PersistentNodeIdentity,
    event_bus: &EventBus,
    swarm: &mut Swarm<VoidBehaviour>,
    topology: &mut PeerTopology,
    topology_file: &PathBuf,
    chat: &mut ChatRuntimeState,
    local_peer_id_string: &str,
) -> Result<(), TransportError> {
    let message = match serde_json::from_slice::<ChatWireMessage>(data) {
        Ok(message) => message,
        Err(error) => {
            topology.observe_failure(Some(propagation_source.to_string()), error.to_string());
            event_bus.emit(TransportEvent::TransportFailed {
                peer_id: Some(propagation_source.to_string()),
                address: None,
                error: format!("invalid chat wire payload: {error}"),
            });
            return Ok(());
        }
    };

    let actions = match message {
        ChatWireMessage::SessionOffer(offer) => {
            if offer.recipient_peer_id != local_peer_id_string {
                Vec::new()
            } else {
                match process_session_offer(identity, event_bus, topology, chat, offer) {
                    Ok(actions) => actions,
                    Err(error) => {
                        topology.observe_session_failure(propagation_source.to_string(), error.to_string());
                        Vec::new()
                    }
                }
            }
        }
        ChatWireMessage::SessionAck(ack) => {
            if ack.recipient_peer_id != local_peer_id_string {
                Vec::new()
            } else {
                process_session_ack(identity, event_bus, topology, chat, ack)?
            }
        }
        ChatWireMessage::DirectMessage(envelope) => {
            if envelope.recipient_peer_id != local_peer_id_string {
                Vec::new()
            } else {
                process_direct_message(event_bus, topology, chat, envelope)?;
                Vec::new()
            }
        }
        ChatWireMessage::RoomMembership(event) => {
            if !is_room_topic(topic) {
                Vec::new()
            } else {
                process_room_membership(identity, event_bus, chat, event, room_from_topic(topic).as_deref())?
            }
        }
        ChatWireMessage::RoomStateSnapshot(snapshot) => {
            if !is_room_topic(topic) {
                Vec::new()
            } else {
                process_room_state_snapshot(event_bus, chat, snapshot, room_from_topic(topic).as_deref())?;
                Vec::new()
            }
        }
    };

    apply_chat_actions(swarm, event_bus, topology, chat, actions)?;
    chat.persist()
        .map_err(|error| TransportError::Backend(error.to_string()))?;
    persist_topology(event_bus, topology, topology_file)?;
    Ok(())
}

fn process_session_offer(
    identity: &PersistentNodeIdentity,
    event_bus: &EventBus,
    topology: &mut PeerTopology,
    chat: &mut ChatRuntimeState,
    offer: SessionOffer,
) -> Result<Vec<ChatSwarmAction>, TransportError> {
    if let Err(error) = offer.verify() {
        event_bus.emit(TransportEvent::InvalidSignatureRejected {
            peer_id: Some(offer.sender_peer_id.clone()),
            context: format!("session-offer: {error}"),
        });
        return Ok(Vec::new());
    }
    if let Err(error) = chat.replay.check_and_record(
        &format!("offer:{}", offer.sender_peer_id),
        &offer.nonce,
        offer.timestamp_unix_ms,
    ) {
        event_bus.emit(TransportEvent::ReplayRejected {
            peer_id: offer.sender_peer_id.clone(),
            context: format!("session-offer: {error}"),
            nonce: offer.nonce.clone(),
        });
        return Ok(Vec::new());
    }

    let secret = random_ephemeral_secret();
    let public_key = public_key_from_secret(&secret);
    let session_key = void_chat::derive_session_key(
        secret,
        offer.peer_public_key()
            .map_err(|error| TransportError::Backend(error.to_string()))?,
        &offer.session_id,
    );
    let established_at = now_unix_ms();
    chat.active_sessions.insert(
        offer.sender_peer_id.clone(),
        ActiveChatSession {
            key: session_key,
            snapshot: ChatSessionSnapshot {
                peer_id: offer.sender_peer_id.clone(),
                session_id: offer.session_id.clone(),
                established_at_unix_ms: established_at,
                encryption_state: ChatSessionState::Established,
                last_activity_unix_ms: established_at,
                transport_state: "GOSSIPSUB-DIRECT".to_string(),
                last_error: None,
            },
        },
    );
    topology.observe_encrypted_session(
        offer.sender_peer_id.clone(),
        Some(offer.session_id.clone()),
        "aes-256-gcm",
    );
    event_bus.emit(TransportEvent::EncryptedSessionEstablished {
        peer_id: offer.sender_peer_id.clone(),
        transport: "gossipsub/direct".to_string(),
        cipher: "aes-256-gcm".to_string(),
    });

    let ack = SessionAck::new(
        identity,
        offer.sender_peer_id.clone(),
        offer.session_id.clone(),
        public_key.to_bytes(),
    )
    .map_err(|error| TransportError::Backend(error.to_string()))?;
    let mut actions = vec![ChatSwarmAction::Publish {
        topic: direct_topic(&offer.sender_peer_id),
        message: ChatWireMessage::SessionAck(ack),
    }];
    if let Some(messages) = chat.pending_messages.remove(&offer.sender_peer_id) {
        for message in messages {
            if let Some(session) = chat.active_sessions.get_mut(&offer.sender_peer_id) {
                actions.push(build_direct_message_action(identity, &offer.sender_peer_id, &message, session)?);
            }
        }
    }

    Ok(actions)
}

fn process_session_ack(
    identity: &PersistentNodeIdentity,
    event_bus: &EventBus,
    topology: &mut PeerTopology,
    chat: &mut ChatRuntimeState,
    ack: SessionAck,
) -> Result<Vec<ChatSwarmAction>, TransportError> {
    if let Err(error) = ack.verify() {
        event_bus.emit(TransportEvent::InvalidSignatureRejected {
            peer_id: Some(ack.sender_peer_id.clone()),
            context: format!("session-ack: {error}"),
        });
        return Ok(Vec::new());
    }
    if let Err(error) = chat.replay.check_and_record(
        &format!("ack:{}", ack.sender_peer_id),
        &ack.nonce,
        ack.timestamp_unix_ms,
    ) {
        event_bus.emit(TransportEvent::ReplayRejected {
            peer_id: ack.sender_peer_id.clone(),
            context: format!("session-ack: {error}"),
            nonce: ack.nonce.clone(),
        });
        return Ok(Vec::new());
    }

    let Some(pending) = chat.pending_sessions.remove(&ack.sender_peer_id) else {
        return Ok(Vec::new());
    };
    if pending.session_id != ack.session_id {
        topology.observe_session_failure(ack.sender_peer_id.clone(), "session ack id mismatch");
        return Ok(Vec::new());
    }

    let session_key = void_chat::derive_session_key(
        pending.secret,
        ack.peer_public_key()
            .map_err(|error| TransportError::Backend(error.to_string()))?,
        &ack.session_id,
    );
    let established_at = now_unix_ms();
    chat.active_sessions.insert(
        ack.sender_peer_id.clone(),
        ActiveChatSession {
            key: session_key,
            snapshot: ChatSessionSnapshot {
                peer_id: ack.sender_peer_id.clone(),
                session_id: ack.session_id.clone(),
                established_at_unix_ms: established_at,
                encryption_state: ChatSessionState::Established,
                last_activity_unix_ms: established_at,
                transport_state: "GOSSIPSUB-DIRECT".to_string(),
                last_error: None,
            },
        },
    );
    topology.observe_encrypted_session(
        ack.sender_peer_id.clone(),
        Some(ack.session_id.clone()),
        "aes-256-gcm",
    );
    event_bus.emit(TransportEvent::EncryptedSessionEstablished {
        peer_id: ack.sender_peer_id.clone(),
        transport: "gossipsub/direct".to_string(),
        cipher: "aes-256-gcm".to_string(),
    });

    let mut actions = Vec::new();
    if let Some(messages) = chat.pending_messages.remove(&ack.sender_peer_id) {
        for message in messages {
            if let Some(session) = chat.active_sessions.get_mut(&ack.sender_peer_id) {
                actions.push(build_direct_message_action(identity, &ack.sender_peer_id, &message, session)?);
            }
        }
    }
    Ok(actions)
}

fn process_direct_message(
    event_bus: &EventBus,
    topology: &mut PeerTopology,
    chat: &mut ChatRuntimeState,
    envelope: SignedEncryptedEnvelope,
) -> Result<(), TransportError> {
    if let Err(error) = envelope.verify() {
        event_bus.emit(TransportEvent::InvalidSignatureRejected {
            peer_id: Some(envelope.sender_peer_id.clone()),
            context: format!("direct-message: {error}"),
        });
        return Ok(());
    }
    if let Err(error) = chat.replay.check_and_record(
        &format!("message:{}", envelope.sender_peer_id),
        &envelope.nonce,
        envelope.timestamp_unix_ms,
    ) {
        event_bus.emit(TransportEvent::ReplayRejected {
            peer_id: envelope.sender_peer_id.clone(),
            context: format!("direct-message: {error}"),
            nonce: envelope.nonce.clone(),
        });
        return Ok(());
    }

    let Some(session) = chat.active_sessions.get_mut(&envelope.sender_peer_id) else {
        topology.observe_session_failure(envelope.sender_peer_id.clone(), "missing active session for inbound payload");
        return Ok(());
    };
    if session.snapshot.session_id != envelope.session_id {
        topology.observe_session_failure(envelope.sender_peer_id.clone(), "session id mismatch for inbound payload");
        return Ok(());
    }

    let encrypted_payload = deserialize_payload(
        &envelope
            .payload_bytes()
            .map_err(|error| TransportError::Backend(error.to_string()))?,
    )
    .map_err(|error| TransportError::Backend(error.to_string()))?;
    let plaintext = match decrypt_payload(&session.key, &encrypted_payload) {
        Ok(plaintext) => plaintext,
        Err(error) => {
            event_bus.emit(TransportEvent::DecryptionFailed {
                peer_id: envelope.sender_peer_id.clone(),
                session_id: envelope.session_id.clone(),
                reason: error.to_string(),
            });
            topology.observe_session_failure(envelope.sender_peer_id.clone(), error.to_string());
            return Ok(());
        }
    };
    let payload: ChatTextPayload = serde_json::from_slice(&plaintext)
        .map_err(|error| TransportError::Backend(error.to_string()))?;
    chat.inbox.messages.push(ChatInboxEntry {
        message_id: envelope.nonce.clone(),
        from_peer_id: envelope.sender_peer_id.clone(),
        room: payload.room.clone(),
        body: payload.body.clone(),
        received_at_unix_ms: now_unix_ms(),
        session_id: envelope.session_id.clone(),
        signature_verified: true,
        unread: true,
        room_name: payload.room.clone(),
    });
    if let Some(room) = &payload.room {
        upsert_room_member(&mut chat.rooms, room, &envelope.sender_peer_id, true);
        record_room_event(
            &mut chat.rooms,
            room,
            "room.message",
            &envelope.sender_peer_id,
            Some(payload.body.clone()),
        );
    }
    let unread_notifications = push_notification(
        &mut chat.notifications,
        "message.received",
        payload.room.as_deref(),
        Some(&envelope.sender_peer_id),
        if let Some(room) = &payload.room {
            format!("new message in {room} from {}", envelope.sender_peer_id)
        } else {
            format!("new direct message from {}", envelope.sender_peer_id)
        },
    );
    session.snapshot.last_activity_unix_ms = now_unix_ms();
    topology.observe_session_activity(envelope.sender_peer_id.clone());
    event_bus.emit(TransportEvent::PayloadVerified {
        peer_id: envelope.sender_peer_id.clone(),
        session_id: envelope.session_id.clone(),
        message_id: envelope.nonce.clone(),
    });
    event_bus.emit(TransportEvent::EncryptedMessageDelivered {
        peer_id: envelope.sender_peer_id,
        session_id: envelope.session_id,
        direction: "inbound".to_string(),
        size_bytes: payload.body.len(),
    });
    event_bus.emit(TransportEvent::MessageReceived {
        peer_id: chat
            .inbox
            .messages
            .last()
            .map(|entry| entry.from_peer_id.clone())
            .unwrap_or_default(),
        room: payload.room.clone(),
        session_id: chat
            .inbox
            .messages
            .last()
            .map(|entry| entry.session_id.clone())
            .unwrap_or_default(),
    });
    event_bus.emit(TransportEvent::InboxSynchronized {
        messages: chat.inbox.messages.len(),
        unread_messages: unread_count(&chat.inbox, None),
        room: payload.room.clone(),
    });
    event_bus.emit(TransportEvent::NotificationRaised {
        kind: "message.received".to_string(),
        room: payload.room,
        peer_id: chat.inbox.messages.last().map(|entry| entry.from_peer_id.clone()),
        unread_notifications,
    });
    Ok(())
}

fn process_room_membership(
    identity: &PersistentNodeIdentity,
    event_bus: &EventBus,
    chat: &mut ChatRuntimeState,
    event: RoomMembershipEvent,
    room_from_topic: Option<&str>,
) -> Result<Vec<ChatSwarmAction>, TransportError> {
    if let Err(error) = event.verify() {
        event_bus.emit(TransportEvent::InvalidSignatureRejected {
            peer_id: Some(event.peer_id.clone()),
            context: format!("room-membership: {error}"),
        });
        return Ok(Vec::new());
    }
    if let Err(error) = chat.replay.check_and_record(
        &format!("room:{}", event.peer_id),
        &event.nonce,
        event.timestamp_unix_ms,
    ) {
        event_bus.emit(TransportEvent::ReplayRejected {
            peer_id: event.peer_id.clone(),
            context: format!("room-membership: {error}"),
            nonce: event.nonce.clone(),
        });
        return Ok(Vec::new());
    }

    let room_name = room_from_topic.unwrap_or(event.room.as_str());
    let joined = matches!(event.action, RoomMembershipAction::Join);
    upsert_room_member(&mut chat.rooms, room_name, &event.peer_id, joined);
    record_room_event(
        &mut chat.rooms,
        room_name,
        if joined { "room.join" } else { "room.leave" },
        &event.peer_id,
        None,
    );
    event_bus.emit(TransportEvent::RoomMembershipChanged {
        room: room_name.to_string(),
        peer_id: event.peer_id.clone(),
        action: if joined { "join" } else { "leave" }.to_string(),
    });
    event_bus.emit(TransportEvent::PresenceUpdated {
        peer_id: event.peer_id.clone(),
        room: Some(room_name.to_string()),
        presence: if joined { "ONLINE" } else { "OFFLINE" }.to_string(),
        last_seen_unix_ms: event.timestamp_unix_ms,
    });
    if joined {
        event_bus.emit(TransportEvent::RoomJoined {
            room: room_name.to_string(),
            peer_id: event.peer_id.clone(),
        });
    } else {
        event_bus.emit(TransportEvent::RoomLeft {
            room: room_name.to_string(),
            peer_id: event.peer_id.clone(),
        });
    }
    if event.peer_id != chat.local_peer_id && chat.joined_rooms.contains(room_name) {
        let unread_notifications = push_notification(
            &mut chat.notifications,
            if joined { "room.peer-join" } else { "room.peer-leave" },
            Some(room_name),
            Some(&event.peer_id),
            if joined {
                format!("{} joined {room_name}", event.peer_id)
            } else {
                format!("{} left {room_name}", event.peer_id)
            },
        );
        event_bus.emit(TransportEvent::NotificationRaised {
            kind: if joined { "room.peer-join" } else { "room.peer-leave" }.to_string(),
            room: Some(room_name.to_string()),
            peer_id: Some(event.peer_id.clone()),
            unread_notifications,
        });
    }
    if joined && chat.joined_rooms.contains(room_name) && event.peer_id != chat.local_peer_id {
        if let Some(action) = build_room_state_action(identity, event_bus, chat, room_name, "peer-join")? {
            return Ok(vec![action]);
        }
    }
    Ok(Vec::new())
}

fn process_room_state_snapshot(
    event_bus: &EventBus,
    chat: &mut ChatRuntimeState,
    snapshot: RoomStateSnapshot,
    room_from_topic: Option<&str>,
) -> Result<(), TransportError> {
    if let Err(error) = snapshot.verify() {
        event_bus.emit(TransportEvent::InvalidSignatureRejected {
            peer_id: Some(snapshot.peer_id.clone()),
            context: format!("room-state: {error}"),
        });
        return Ok(());
    }
    if let Err(error) = chat.replay.check_and_record(
        &format!("room-state:{}", snapshot.peer_id),
        &snapshot.nonce,
        snapshot.timestamp_unix_ms,
    ) {
        event_bus.emit(TransportEvent::ReplayRejected {
            peer_id: snapshot.peer_id.clone(),
            context: format!("room-state: {error}"),
            nonce: snapshot.nonce.clone(),
        });
        return Ok(());
    }

    let room_name = room_from_topic.unwrap_or(snapshot.room.as_str()).to_string();
    let remote_snapshot = ChatRoomSnapshot {
        room: room_name.clone(),
        joined: false,
        members: snapshot.members.clone(),
        room_id: room_name.clone(),
        room_name: room_name.clone(),
        active_members: snapshot.members.iter().filter(|member| member.presence == "ONLINE").count(),
        event_history: snapshot.recent_events.clone(),
        last_changed_unix_ms: snapshot.timestamp_unix_ms,
    };
    merge_room_snapshot(&mut chat.rooms, &remote_snapshot);
    event_bus.emit(TransportEvent::RoomStateSynchronized {
        room: room_name.clone(),
        peer_id: snapshot.peer_id.clone(),
        members: remote_snapshot.active_members,
        events: remote_snapshot.event_history.len(),
        reason: "remote-snapshot".to_string(),
    });
    event_bus.emit(TransportEvent::PresenceUpdated {
        peer_id: snapshot.peer_id,
        room: Some(room_name),
        presence: "ONLINE".to_string(),
        last_seen_unix_ms: snapshot.timestamp_unix_ms,
    });
    Ok(())
}

fn apply_chat_actions(
    swarm: &mut Swarm<VoidBehaviour>,
    event_bus: &EventBus,
    topology: &mut PeerTopology,
    chat: &mut ChatRuntimeState,
    actions: Vec<ChatSwarmAction>,
) -> Result<(), TransportError> {
    for action in actions {
        match action {
            ChatSwarmAction::Subscribe(topic) => {
                swarm
                    .behaviour_mut()
                    .gossipsub
                    .subscribe(&gossipsub::IdentTopic::new(topic.clone()))
                    .map_err(|error| TransportError::Backend(error.to_string()))?;
                if let Some(room) = room_from_topic(&topic) {
                    chat.joined_rooms.insert(room);
                }
            }
            ChatSwarmAction::Unsubscribe(topic) => {
                let _ = swarm
                    .behaviour_mut()
                    .gossipsub
                    .unsubscribe(&gossipsub::IdentTopic::new(topic.clone()));
                if let Some(room) = room_from_topic(&topic) {
                    chat.joined_rooms.remove(&room);
                }
            }
            ChatSwarmAction::Publish { topic, message } => {
                let payload = serde_json::to_vec(&message)
                    .map_err(|error| TransportError::Backend(error.to_string()))?;
                if let Err(error) = swarm
                    .behaviour_mut()
                    .gossipsub
                    .publish(gossipsub::IdentTopic::new(topic.clone()), payload)
                {
                    let peer_id = direct_peer_from_topic(&topic);
                    topology.observe_failure(peer_id.clone(), error.to_string());
                    event_bus.emit(TransportEvent::TransportFailed {
                        peer_id,
                        address: None,
                        error: format!("failed to publish chat topic {topic}: {error}"),
                    });
                }
            }
        }
    }
    Ok(())
}

fn dispatch_or_queue_outbound_message(
    identity: &PersistentNodeIdentity,
    event_bus: &EventBus,
    topology: &mut PeerTopology,
    chat: &mut ChatRuntimeState,
    peer_id: &str,
    outbound: PendingOutboundMessage,
) -> Result<Vec<ChatSwarmAction>, TransportError> {
    if let Some(session) = chat.active_sessions.get_mut(peer_id) {
        let action = build_direct_message_action(identity, peer_id, &outbound, session)?;
        topology.observe_session_activity(peer_id.to_string());
        event_bus.emit(TransportEvent::EncryptedMessageDelivered {
            peer_id: peer_id.to_string(),
            session_id: session.snapshot.session_id.clone(),
            direction: "outbound".to_string(),
            size_bytes: outbound.body.len(),
        });
        return Ok(vec![action]);
    }

    chat.pending_messages
        .entry(peer_id.to_string())
        .or_default()
        .push(outbound);
    if chat.pending_sessions.contains_key(peer_id) {
        return Ok(Vec::new());
    }

    let secret = random_ephemeral_secret();
    let public_key = public_key_from_secret(&secret);
    let offer = SessionOffer::new(identity, peer_id.to_string(), public_key.to_bytes())
        .map_err(|error| TransportError::Backend(error.to_string()))?;
    topology.observe_session_negotiating(peer_id.to_string(), offer.session_id.clone());
    event_bus.emit(TransportEvent::SessionNegotiationStarted {
        peer_id: peer_id.to_string(),
        session_id: offer.session_id.clone(),
        transport: "gossipsub/direct".to_string(),
    });
    chat.pending_sessions.insert(
        peer_id.to_string(),
        PendingChatSession {
            peer_id: peer_id.to_string(),
            session_id: offer.session_id.clone(),
            secret,
            started_at_unix_ms: now_unix_ms(),
        },
    );
    Ok(vec![ChatSwarmAction::Publish {
        topic: direct_topic(peer_id),
        message: ChatWireMessage::SessionOffer(offer),
    }])
}

fn build_room_sync_actions(
    identity: &PersistentNodeIdentity,
    event_bus: &EventBus,
    chat: &mut ChatRuntimeState,
) -> Result<Vec<ChatSwarmAction>, TransportError> {
    let now = now_unix_ms();
    if chat.joined_rooms.is_empty()
        || now.saturating_sub(chat.last_room_sync_unix_ms) < CHAT_ROOM_SYNC_INTERVAL.as_millis()
    {
        return Ok(Vec::new());
    }
    chat.last_room_sync_unix_ms = now;

    let mut actions = Vec::new();
    for room in chat.joined_rooms.iter().cloned().collect::<Vec<_>>() {
        upsert_room_member(&mut chat.rooms, &room, &chat.local_peer_id, true);
        if let Some(action) = build_room_state_action(identity, event_bus, chat, &room, "heartbeat")? {
            actions.push(action);
        }
    }
    Ok(actions)
}

fn build_room_state_action(
    identity: &PersistentNodeIdentity,
    event_bus: &EventBus,
    chat: &ChatRuntimeState,
    room: &str,
    reason: &str,
) -> Result<Option<ChatSwarmAction>, TransportError> {
    let Some(snapshot) = chat.rooms.rooms.iter().find(|entry| entry.room == room) else {
        return Ok(None);
    };
    let snapshot = RoomStateSnapshot::new(identity, snapshot)
        .map_err(|error| TransportError::Backend(error.to_string()))?;
    event_bus.emit(TransportEvent::RoomStateSynchronized {
        room: room.to_string(),
        peer_id: chat.local_peer_id.clone(),
        members: snapshot.members.iter().filter(|member| member.presence == "ONLINE").count(),
        events: snapshot.recent_events.len(),
        reason: reason.to_string(),
    });
    Ok(Some(ChatSwarmAction::Publish {
        topic: room_topic(room),
        message: ChatWireMessage::RoomStateSnapshot(snapshot),
    }))
}

fn room_recipients(chat: &ChatRuntimeState, room: &str) -> Vec<String> {
    chat.rooms
        .rooms
        .iter()
        .find(|entry| entry.room == room)
        .map(|snapshot| {
            snapshot
                .members
                .iter()
                .filter(|member| member.peer_id != chat.local_peer_id && member.presence == "ONLINE")
                .map(|member| member.peer_id.clone())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

fn append_local_history(
    chat: &mut ChatRuntimeState,
    room: Option<String>,
    body: String,
    session_id: &str,
) {
    let room_name = room.clone();
    chat.inbox.messages.push(ChatInboxEntry {
        message_id: format!("{}-{}", chat.local_peer_id, now_unix_ms()),
        from_peer_id: chat.local_peer_id.clone(),
        room,
        body,
        received_at_unix_ms: now_unix_ms(),
        session_id: session_id.to_string(),
        signature_verified: true,
        unread: false,
        room_name,
    });
}
