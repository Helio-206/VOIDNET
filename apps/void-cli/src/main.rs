use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
mod console;
use std::{
    collections::BTreeMap,
    io::{self, IsTerminal, Write},
    path::PathBuf,
    str::FromStr,
    sync::Arc,
    sync::mpsc,
    thread,
    time::Duration,
};
use void_chat::{
    enqueue_local_command, load_chat_inbox, load_chat_notifications, load_chat_rooms,
    load_chat_sessions, unread_count, ChatLocalCommand,
};
use void_dns::{
    enqueue_dns_command, DnsCommand, PersistentVoidDns, VoidDnsResolver, VoidDomain,
};
use void_identity::{default_node_dir, NodeIdentity, PersistentNodeIdentity};
use void_protocol::VoidUri;
use void_runtime::{
    ui::RuntimeActionRequest, GatewayTrustLevel, GatewayTrustState, RuntimeConfig, RuntimeShell,
    VoidRuntime,
};
use void_transport::{event::TransportEvent, network_channels, PeerTopology, RuntimeShellTopologyInfo};

#[derive(Debug, Parser)]
#[command(name = "voidnet", about = "VOIDNET operator CLI")]
struct Args {
    #[arg(long)]
    data_dir: Option<PathBuf>,

    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    Console,
    Identity {
        #[arg(long)]
        persistent: bool,
    },
    Uri {
        raw: String,
    },
    Domain {
        raw: String,
    },
    Open {
        uri: String,
    },
    Peers,
    Topology,
    Diagnostics,
    Runtime {
        #[command(subcommand)]
        command: RuntimeCliCommand,
    },
    Sessions,
    Dns {
        #[command(subcommand)]
        command: DnsCliCommand,
    },
    Gateway {
        #[command(subcommand)]
        command: GatewayCommand,
    },
    Chat {
        #[command(subcommand)]
        command: ChatCommand,
    },
}

#[derive(Debug, Subcommand)]
enum RuntimeCliCommand {
    Sessions,
    Mounts,
    Permissions {
        #[command(subcommand)]
        action: Option<RuntimePermissionAction>,
    },
    Registry,
}

#[derive(Debug, Subcommand)]
enum RuntimePermissionAction {
    Grant {
        surface_id: String,
        peer_owner: String,
        capability: String,
    },
    Deny {
        surface_id: String,
        peer_owner: String,
        capability: String,
    },
}

#[derive(Debug, Subcommand)]
enum DnsCliCommand {
    Publish {
        domain: String,
        #[arg(long)]
        surface: Option<String>,
        #[arg(long = "capability")]
        capabilities: Vec<String>,
        #[arg(long, default_value_t = 300)]
        ttl_secs: u64,
        #[arg(long)]
        target_peer_id: Option<String>,
    },
    Resolve {
        domain: String,
    },
    List,
    Cache,
    Inspect {
        domain: String,
    },
}

#[derive(Debug, Subcommand)]
enum GatewayCommand {
    Register {
        domain: String,
        #[arg(long)]
        external_base: Option<String>,
        #[arg(long = "protocol")]
        protocols: Vec<String>,
        #[arg(long = "capability")]
        capabilities: Vec<String>,
        #[arg(long, default_value = "restricted")]
        trust: String,
    },
    List,
    Inspect {
        domain: String,
    },
    Allow {
        domain: String,
        capability: String,
    },
    Deny {
        domain: String,
        capability: String,
    },
}

#[derive(Debug, Subcommand)]
enum ChatCommand {
    Peers,
    Sessions,
    Send {
        peer_id: String,
        message: String,
    },
    RoomSend {
        room: String,
        message: String,
    },
    Inbox,
    Rooms,
    Join {
        room: String,
    },
    Leave {
        room: String,
    },
    Switch {
        room: String,
    },
    MarkRead {
        room: Option<String>,
    },
}

pub fn main() -> Result<()> {
    let args = Args::parse();
    let data_dir = args.data_dir.unwrap_or_else(default_node_dir);

    match args.command {
        Command::Console => {
            console::run_console(data_dir.clone())?;
        }
        Command::Identity { persistent } => {
            if persistent {
                let identity = PersistentNodeIdentity::load_or_create_dir(&data_dir)
                    .with_context(|| format!("failed to load identity in {}", data_dir.display()))?;
                println!("peer_id={}", identity.peer_id_string());
                println!("fingerprint={}", identity.fingerprint());
                println!("identity={}", identity.path().display());
            } else {
                let identity = NodeIdentity::generate();
                println!("peer_id={}", identity.peer_id());
                println!("public_key={}", identity.public_key_base64());
            }
        }
        Command::Uri { raw } => {
            let uri = VoidUri::from_str(&raw)?;
            println!("scheme=void");
            println!("authority={}", uri.authority());
            println!("path={}", uri.path());
            if let Some(query) = uri.query() {
                println!("query={query}");
            }
            println!("domain={}", uri.is_void_domain());
        }
        Command::Domain { raw } => {
            let domain = VoidDomain::from_str(&raw)?;
            println!("domain={domain}");
        }
        Command::Open { uri } => {
            let uri = parse_void_uri_input(&uri)?;
            let persistent_identity = PersistentNodeIdentity::load_or_create_dir(&data_dir)
                .with_context(|| format!("failed to load persistent identity in {}", data_dir.display()))?;
            let dns = Arc::new(PersistentVoidDns::load_or_create(&data_dir)
                .with_context(|| format!("failed to load dns cache in {}", data_dir.display()))?);
            let (network, _inbox) = network_channels(16);
            let runtime = VoidRuntime::new(NodeIdentity::generate(), dns, network, RuntimeConfig::default());
            let mut shell = RuntimeShell::load_or_create(&data_dir, runtime)?;
            shell.reconcile_registry_owner(&persistent_identity.peer_id_string())?;
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()?
                .block_on(shell.synchronize_registry_dns(&persistent_identity))?;
            let open_result = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()?
                .block_on(shell.open_uri(uri.clone()));
            if open_result.is_err() {
                persist_runtime_shell_topology(&data_dir, shell.state())?;
            }
            let result = open_result?;
            for event in &result.events {
                println!("{}", event.log_line());
            }
            println!("route={}", result.route.uri);
            println!("surface_id={}", result.mount.surface_id);
            println!("peer_owner={}", result.mount.owner_peer);
            println!("session_id={}", result.session.session_id);
            println!("capabilities={}", result.mount.capabilities.join(","));
            println!("mount_state={:?}", result.mount.mount_state);
            println!();
            println!("{}", result.rendered_surface.output);

            let mut rendered = result.rendered_surface;
            let mut input_state = shell
                .state()
                .ui_surfaces
                .iter()
                .find(|surface| surface.route == uri.to_string())
                .map(|surface| surface.input_state.clone())
                .unwrap_or_default();
            persist_runtime_shell_topology(&data_dir, shell.state())?;

            if io::stdin().is_terminal() {
                interactive_surface_loop(&data_dir, &uri.to_string(), &mut shell, &mut rendered, &mut input_state)?;
            }
        }
        Command::Peers => {
            let topology = load_topology(&data_dir)?;
            println!("{}", topology.render_table());
        }
        Command::Topology => {
            let topology = load_topology(&data_dir)?;
            println!("{}", topology.render_ascii());
        }
        Command::Diagnostics => {
            let topology = load_topology(&data_dir)?;
            println!("{}", topology.render_diagnostics());
            for line in render_gateway_diagnostics(&data_dir)? {
                println!("{line}");
            }
            for line in render_chat_diagnostics(&data_dir)? {
                println!("{line}");
            }
        }
        Command::Runtime { command } => {
            let persistent_identity = PersistentNodeIdentity::load_or_create_dir(&data_dir)
                .with_context(|| format!("failed to load persistent identity in {}", data_dir.display()))?;
            let dns = Arc::new(PersistentVoidDns::load_or_create(&data_dir)
                .with_context(|| format!("failed to load dns cache in {}", data_dir.display()))?);
            let (network, _inbox) = network_channels(16);
            let runtime = VoidRuntime::new(NodeIdentity::generate(), dns.clone(), network, RuntimeConfig::default());
            let mut shell = RuntimeShell::load_or_create(&data_dir, runtime)?;
            shell.reconcile_registry_owner(&persistent_identity.peer_id_string())?;
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()?
                .block_on(shell.synchronize_registry_dns(&persistent_identity))?;
            match command {
                RuntimeCliCommand::Sessions => {
                    if shell.state().sessions.is_empty() {
                        println!("no runtime sessions observed");
                    } else {
                        println!("SESSION ID            SURFACE         PEER OWNER                                         STATE        STARTED         LAST ACTIVITY   CAPABILITIES");
                        for session in &shell.state().sessions {
                            println!(
                                "{:<20} {:<15} {:<50} {:<12} {:<15} {:<15} {}",
                                session.session_id,
                                session.mounted_surface,
                                session.peer_owner,
                                format!("{:?}", session.session_state),
                                session.started_at_unix_ms,
                                session.last_activity_unix_ms,
                                session.active_capabilities.join(","),
                            );
                        }
                    }
                }
                RuntimeCliCommand::Mounts => {
                    if shell.state().mounts.is_empty() {
                        println!("no mounted runtime surfaces");
                    } else {
                        println!("ROUTE                       SURFACE         OWNER                                              RUNTIME      MOUNT       SESSION      LATENCY");
                        for mount in &shell.state().mounts {
                            println!(
                                "{:<27} {:<15} {:<50} {:<12} {:<11} {:<12} {}ms",
                                mount.route,
                                mount.surface_id,
                                mount.owner_peer,
                                format!("{:?}", mount.runtime_state),
                                format!("{:?}", mount.mount_state),
                                format!("{:?}", mount.session_state),
                                mount.mount_latency_ms,
                            );
                        }
                    }
                }
                RuntimeCliCommand::Permissions { action } => match action {
                    Some(RuntimePermissionAction::Grant {
                        surface_id,
                        peer_owner,
                        capability,
                    }) => {
                        shell.grant_permission(&surface_id, &peer_owner, &capability, true)?;
                        persist_runtime_shell_topology(&data_dir, shell.state())?;
                        println!("permission=granted surface={} peer={} capability={}", surface_id, peer_owner, capability);
                    }
                    Some(RuntimePermissionAction::Deny {
                        surface_id,
                        peer_owner,
                        capability,
                    }) => {
                        shell.grant_permission(&surface_id, &peer_owner, &capability, false)?;
                        persist_runtime_shell_topology(&data_dir, shell.state())?;
                        println!("permission=denied surface={} peer={} capability={}", surface_id, peer_owner, capability);
                    }
                    None => {
                        if shell.state().permissions.is_empty() {
                            println!("no runtime permissions persisted");
                        } else {
                            println!("SURFACE         PEER OWNER                                         CAPABILITY              ALLOWED PERSISTED");
                            for grant in &shell.state().permissions {
                                println!(
                                    "{:<15} {:<50} {:<22} {:<7} {}",
                                    grant.surface_id,
                                    grant.peer_owner,
                                    grant.capability,
                                    if grant.allowed { "yes" } else { "no" },
                                    if grant.persisted { "yes" } else { "no" },
                                );
                            }
                        }
                    }
                },
                RuntimeCliCommand::Registry => {
                    if shell.state().registry.is_empty() {
                        println!("no runtime surfaces registered");
                    } else {
                        println!("DOMAIN                      SURFACE         OWNER                                              HANDLER        PUBLISHED CAPABILITIES");
                        for entry in &shell.state().registry {
                            let published = tokio::runtime::Builder::new_current_thread()
                                .enable_all()
                                .build()?
                                .block_on(dns.resolve(&VoidDomain::new(entry.domain.clone())?))?
                                .is_some();
                            println!(
                                "{:<27} {:<15} {:<50} {:<14} {:<9} {}",
                                entry.domain,
                                entry.surface_id,
                                entry.owner_peer_id,
                                entry.handler,
                                if published { "yes" } else { "no" },
                                entry.capabilities.join(","),
                            );
                        }
                    }
                }
            }
            persist_runtime_shell_topology(&data_dir, shell.state())?;
        }
        Command::Sessions => {
            let topology = load_topology(&data_dir)?;
            println!("{}", topology.render_sessions());
        }
        Command::Dns { command } => {
            let dns = PersistentVoidDns::load_or_create(&data_dir)
                .with_context(|| format!("failed to load dns cache in {}", data_dir.display()))?;
            match command {
                DnsCliCommand::Publish {
                    domain,
                    surface,
                    capabilities,
                    ttl_secs,
                    target_peer_id,
                } => {
                    let domain = VoidDomain::new(domain)?;
                    let path = enqueue_dns_command(
                        &data_dir,
                        DnsCommand::Publish {
                            domain,
                            runtime_surface: surface,
                            target_peer_id,
                            capabilities,
                            ttl_secs,
                        },
                    )?;
                    println!("queued={}", path.display());
                }
                DnsCliCommand::Resolve { domain } => {
                    let domain = VoidDomain::new(domain)?;
                    match tokio::runtime::Builder::new_current_thread()
                        .enable_all()
                        .build()?
                        .block_on(dns.resolve(&domain))?
                    {
                        Some(record) => {
                            println!("domain={}", record.domain);
                            println!("owner_peer_id={}", record.owner_peer_id);
                            println!("target_peer_id={}", record.target_peer_id);
                            println!("runtime_surface={}", record.runtime_surface);
                            println!("capabilities={}", record.capabilities.join(","));
                            println!("ttl_remaining_secs={}", record.ttl_remaining_secs(now_unix_ms()));
                            println!("signature_state=verified");
                        }
                        None => {
                            println!("domain={}", domain);
                            println!("resolution=not-found");
                        }
                    }
                }
                DnsCliCommand::List => {
                    let records = tokio::runtime::Builder::new_current_thread()
                        .enable_all()
                        .build()?
                        .block_on(dns.list_records());
                    if records.is_empty() {
                        println!("no dns records cached");
                    } else {
                        println!("DOMAIN                      OWNER                                              TARGET                                             SURFACE          TTL VERIFIED SOURCE");
                        for entry in records {
                            println!(
                                "{:<27} {:<50} {:<50} {:<15} {:<3} {:<8} {:?}",
                                entry.record.domain,
                                entry.record.owner_peer_id,
                                entry.record.target_peer_id,
                                entry.record.runtime_surface,
                                entry.record.ttl_remaining_secs(now_unix_ms()),
                                if entry.verified { "yes" } else { "no" },
                                entry.source,
                            );
                        }
                    }
                }
                DnsCliCommand::Cache => {
                    let records = tokio::runtime::Builder::new_current_thread()
                        .enable_all()
                        .build()?
                        .block_on(dns.list_records());
                    let conflicts = tokio::runtime::Builder::new_current_thread()
                        .enable_all()
                        .build()?
                        .block_on(dns.list_conflicts());
                    println!("cache_entries={}", records.len());
                    println!("conflicts={}", conflicts.len());
                    for entry in records {
                        println!(
                            "cached domain={} owner={} surface={} ttl_remaining_secs={} source={:?}",
                            entry.record.domain,
                            entry.record.owner_peer_id,
                            entry.record.runtime_surface,
                            entry.record.ttl_remaining_secs(now_unix_ms()),
                            entry.source,
                        );
                    }
                    for conflict in conflicts {
                        println!(
                            "conflict domain={} active_owner={} conflicting_owner={} reason={}",
                            conflict.domain,
                            conflict.active_owner_peer_id,
                            conflict.conflicting_owner_peer_id,
                            conflict.reason,
                        );
                    }
                }
                DnsCliCommand::Inspect { domain } => {
                    let domain = VoidDomain::new(domain)?;
                    let inspection = tokio::runtime::Builder::new_current_thread()
                        .enable_all()
                        .build()?
                        .block_on(dns.inspect(&domain))?;
                    println!("domain={}", inspection.domain);
                    if let Some(active) = inspection.active_record {
                        println!("owner_peer_id={}", active.record.owner_peer_id);
                        println!("target_peer_id={}", active.record.target_peer_id);
                        println!("runtime_surface={}", active.record.runtime_surface);
                        println!("capabilities={}", active.record.capabilities.join(","));
                        println!("ttl_remaining_secs={}", inspection.ttl_remaining_secs.unwrap_or_default());
                        println!("signature_state={}", if active.verified { "verified" } else { "unknown" });
                    } else {
                        println!("active_record=none");
                    }
                    println!("conflict_state={}", if inspection.conflicts.is_empty() { "clean" } else { "conflicted" });
                    for conflict in inspection.conflicts {
                        println!(
                            "conflict active_owner={} conflicting_owner={} reason={}",
                            conflict.active_owner_peer_id,
                            conflict.conflicting_owner_peer_id,
                            conflict.reason,
                        );
                    }
                }
            }
        }
        Command::Gateway { command } => {
            let persistent_identity = PersistentNodeIdentity::load_or_create_dir(&data_dir)
                .with_context(|| format!("failed to load persistent identity in {}", data_dir.display()))?;
            let dns = Arc::new(PersistentVoidDns::load_or_create(&data_dir)
                .with_context(|| format!("failed to load dns cache in {}", data_dir.display()))?);
            let (network, _inbox) = network_channels(16);
            let runtime = VoidRuntime::new(NodeIdentity::generate(), dns, network, RuntimeConfig::default());
            let mut shell = RuntimeShell::load_or_create(&data_dir, runtime)?;
            shell.reconcile_registry_owner(&persistent_identity.peer_id_string())?;
            match command {
                GatewayCommand::Register {
                    domain,
                    external_base,
                    protocols,
                    capabilities,
                    trust,
                } => {
                    let trust_level = parse_gateway_trust_level(&trust)?;
                    let registration = shell.register_gateway(
                        &domain,
                        protocols,
                        capabilities,
                        external_base,
                        trust_level,
                    )?;
                    tokio::runtime::Builder::new_current_thread()
                        .enable_all()
                        .build()?
                        .block_on(shell.synchronize_registry_dns(&persistent_identity))?;
                    persist_runtime_shell_topology(&data_dir, shell.state())?;
                    println!("[VOIDNET][GATEWAY] GatewayRegistered route={}", registration.domain);
                    println!("gateway_id={}", registration.surface_id);
                    println!("owner_peer={}", registration.owner_peer_id);
                    println!("protocols={}", registration.supported_protocols.join(","));
                    println!("capabilities={}", registration.capabilities.join(","));
                    println!("external_base={}", registration.external_route_base.as_deref().unwrap_or("-"));
                    println!("trust_level={:?}", registration.trust_level.unwrap_or_default());
                }
                GatewayCommand::List => {
                    let gateways = shell.gateway_registrations();
                    if gateways.is_empty() {
                        println!("no gateways registered");
                    } else {
                        println!("DOMAIN                      GATEWAY ID                     TRUST       PROTOCOLS          CAPABILITIES");
                        for gateway in gateways {
                            println!(
                                "{:<27} {:<30} {:<11} {:<18} {}",
                                gateway.domain,
                                gateway.surface_id,
                                format!("{:?}", gateway.trust_level.unwrap_or_default()),
                                if gateway.supported_protocols.is_empty() {
                                    "-".to_string()
                                } else {
                                    gateway.supported_protocols.join(",")
                                },
                                gateway.capabilities.join(","),
                            );
                        }
                    }
                }
                GatewayCommand::Inspect { domain } => {
                    let registration = shell
                        .gateway_registrations()
                        .into_iter()
                        .find(|gateway| gateway.domain == domain)
                        .with_context(|| format!("gateway not registered: {domain}"))?;
                    let trust = shell.gateway_trust_policy(&domain);
                    println!("domain={}", registration.domain);
                    println!("gateway_id={}", registration.surface_id);
                    println!("owner_peer={}", registration.owner_peer_id);
                    println!("runtime_surface={}", registration.runtime_surface);
                    println!("protocols={}", registration.supported_protocols.join(","));
                    println!("capabilities={}", registration.capabilities.join(","));
                    println!("external_base={}", registration.external_route_base.as_deref().unwrap_or("-"));
                    if let Some(trust) = trust {
                        println!("trust_level={:?}", trust.trust_level);
                        println!("trust_state={:?}", trust.trust_state);
                        println!("runtime_restrictions={}", if trust.runtime_restrictions.is_empty() { "-".to_string() } else { trust.runtime_restrictions.join(",") });
                        println!("last_warning={}", trust.last_warning.as_deref().unwrap_or("-"));
                        println!("permission_history={}", if trust.permission_history.is_empty() { "empty".to_string() } else { trust.permission_history.iter().map(|entry| format!("{}:{}:{}", entry.capability, entry.allowed, entry.reason)).collect::<Vec<_>>().join("|") });
                    }
                }
                GatewayCommand::Allow { domain, capability } => {
                    let registration = shell
                        .gateway_registrations()
                        .into_iter()
                        .find(|gateway| gateway.domain == domain)
                        .with_context(|| format!("gateway not registered: {domain}"))?;
                    shell.grant_permission(&registration.surface_id, &registration.owner_peer_id, &capability, true)?;
                    shell.record_gateway_permission_decision(&domain, &capability, true, "cli allow")?;
                    shell.set_gateway_trust(&domain, GatewayTrustState::Trusted, GatewayTrustLevel::Trusted, "gateway capability allowed")?;
                    persist_runtime_shell_topology(&data_dir, shell.state())?;
                    println!("[VOIDNET][GATEWAY] PermissionGranted capability={capability} domain={domain}");
                }
                GatewayCommand::Deny { domain, capability } => {
                    let registration = shell
                        .gateway_registrations()
                        .into_iter()
                        .find(|gateway| gateway.domain == domain)
                        .with_context(|| format!("gateway not registered: {domain}"))?;
                    shell.grant_permission(&registration.surface_id, &registration.owner_peer_id, &capability, false)?;
                    shell.record_gateway_permission_decision(&domain, &capability, false, "cli deny")?;
                    shell.set_gateway_trust(&domain, GatewayTrustState::Denied, GatewayTrustLevel::Untrusted, "gateway capability denied")?;
                    persist_runtime_shell_topology(&data_dir, shell.state())?;
                    println!("[VOIDNET][GATEWAY] PermissionDenied capability={capability} domain={domain}");
                }
            }
        }
        Command::Chat { command } => match command {
            ChatCommand::Peers => {
                let topology = load_topology(&data_dir)?;
                println!("{}", topology.render_table());
            }
            ChatCommand::Sessions => {
                let sessions = load_chat_sessions(&data_dir)?;
                if sessions.sessions.is_empty() {
                    println!("no chat sessions observed");
                } else {
                    println!("PEER ID                                            SESSION ID           STATE         LAST ACTIVITY   TRANSPORT         LAST ERROR");
                    for session in sessions.sessions {
                        println!(
                            "{:<50} {:<20} {:<13} {:<15} {:<17} {}",
                            session.peer_id,
                            session.session_id,
                            session.encryption_state,
                            session.last_activity_unix_ms,
                            session.transport_state,
                            session.last_error.as_deref().unwrap_or("-"),
                        );
                    }
                }
            }
            ChatCommand::Send { peer_id, message } => {
                let path = enqueue_local_command(
                    &data_dir,
                    ChatLocalCommand::SendDirect { peer_id, message },
                )?;
                println!("queued={}", path.display());
            }
            ChatCommand::RoomSend { room, message } => {
                let path = enqueue_local_command(
                    &data_dir,
                    ChatLocalCommand::SendRoom { room, message },
                )?;
                println!("queued={}", path.display());
            }
            ChatCommand::Inbox => {
                let inbox = load_chat_inbox(&data_dir)?;
                if inbox.messages.is_empty() {
                    println!("inbox empty");
                } else {
                    for message in inbox.messages {
                        println!(
                            "from={} room={} session={} at={} unread={} body={}",
                            message.from_peer_id,
                            message.room.as_deref().unwrap_or("direct"),
                            message.session_id,
                            message.received_at_unix_ms,
                            if message.unread { "yes" } else { "no" },
                            message.body,
                        );
                    }
                }
            }
            ChatCommand::Rooms => {
                let rooms = load_chat_rooms(&data_dir)?;
                if rooms.rooms.is_empty() {
                    println!("no rooms observed");
                } else {
                    for room in rooms.rooms {
                        let members = room
                            .members
                            .iter()
                            .map(|member| format!("{}:{}", member.peer_id, member.presence))
                            .collect::<Vec<_>>()
                            .join(",");
                        println!(
                            "room={} current={} joined={} active_members={} members={} events={}",
                            room.room,
                            if rooms.current_room.as_deref() == Some(room.room.as_str()) { "yes" } else { "no" },
                            room.joined,
                            room.active_members,
                            if members.is_empty() { "-" } else { members.as_str() },
                            room.event_history.len(),
                        );
                    }
                }
            }
            ChatCommand::Join { room } => {
                let path = enqueue_local_command(&data_dir, ChatLocalCommand::Join { room })?;
                println!("queued={}", path.display());
            }
            ChatCommand::Leave { room } => {
                let path = enqueue_local_command(&data_dir, ChatLocalCommand::Leave { room })?;
                println!("queued={}", path.display());
            }
            ChatCommand::Switch { room } => {
                let path = enqueue_local_command(&data_dir, ChatLocalCommand::SwitchRoom { room })?;
                println!("queued={}", path.display());
            }
            ChatCommand::MarkRead { room } => {
                let path = enqueue_local_command(&data_dir, ChatLocalCommand::MarkRead { room })?;
                println!("queued={}", path.display());
            }
        },
    }

    Ok(())
}

pub(crate) fn load_topology(data_dir: &PathBuf) -> Result<PeerTopology> {
    let topology_file = data_dir.join("topology.json");
    if !topology_file.exists() {
        return Ok(PeerTopology::new("unknown-local-peer"));
    }

    PeerTopology::load(&topology_file)
        .with_context(|| format!("failed to read topology at {}", topology_file.display()))
}

pub(crate) fn persist_runtime_shell_topology(
    data_dir: &PathBuf,
    state: &void_runtime::RuntimeShellState,
) -> Result<()> {
    let topology_file = data_dir.join("topology.json");
    let local_peer_id = PersistentNodeIdentity::load_or_create_dir(data_dir)
        .map(|identity| identity.peer_id_string())
        .unwrap_or_else(|_| "unknown-local-peer".to_string());
    let mut topology = if topology_file.exists() {
        PeerTopology::load(&topology_file)?
    } else {
        PeerTopology::new(local_peer_id)
    };
    let last_mount_latency_ms = state
        .mounts
        .iter()
        .map(|mount| mount.mount_latency_ms)
        .max();
    let active_permissions = state.permissions.iter().filter(|grant| grant.allowed).count();
    let failed_mounts = state
        .mounts
        .iter()
        .filter(|mount| matches!(mount.mount_state, void_runtime::MountState::Failed))
        .count();
    let last_render_duration_ms = state
        .ui_surfaces
        .iter()
        .map(|surface| surface.last_render_duration_ms)
        .max();
    let gateway_registrations = state
        .registry
        .iter()
        .filter(|entry| entry.surface_kind == void_runtime::RuntimeSurfaceKind::Gateway)
        .count();
    let gateway_mounts = state
        .mounts
        .iter()
        .filter(|mount| mount.surface_kind == void_runtime::RuntimeSurfaceKind::Gateway)
        .count();
    let gateway_active_routes = state
        .gateway_routes
        .iter()
        .filter(|route| route.active)
        .count();
    let gateway_permission_grants = state
        .permissions
        .iter()
        .filter(|grant| grant.allowed && grant.capability.starts_with("gateway."))
        .count();
    let gateway_bridge_failures = state
        .gateway_routes
        .iter()
        .filter(|route| route.last_error.is_some())
        .count();
    let gateway_bridge_sessions = state.gateway_bridge_sessions.len();
    let gateway_snapshot_entries = count_gateway_snapshot_entries(data_dir)?;
    let state_revisions = state
        .ui_surfaces
        .iter()
        .map(|surface| surface.state_revision)
        .sum();
    let rerender_count = state
        .ui_surfaces
        .iter()
        .map(|surface| surface.rerender_count)
        .sum();
    let hot_reload_count = state
        .ui_surfaces
        .iter()
        .map(|surface| surface.hot_reload_count)
        .sum();
    let sync_count = state
        .ui_surfaces
        .iter()
        .map(|surface| surface.sync_count)
        .sum();
    let permission_denials = state
        .ui_surfaces
        .iter()
        .map(|surface| surface.permission_denials)
        .sum();
    let last_action = state
        .ui_surfaces
        .iter()
        .filter_map(|surface| surface.last_action.clone())
        .last();
    let last_error = state
        .ui_surfaces
        .iter()
        .filter_map(|surface| surface.last_error.clone())
        .last();
    let gateway_last_route = state
        .gateway_routes
        .iter()
        .max_by_key(|route| route.updated_unix_ms)
        .map(|route| route.route.clone());
    let latest_gateway_route = state
        .gateway_routes
        .iter()
        .max_by_key(|route| route.updated_unix_ms);
    let gateway_last_external_target = latest_gateway_route
        .map(|route| route.bridge.external_target.clone());
    let gateway_last_bridge_state = latest_gateway_route
        .map(|route| format!("{:?}", route.bridge.lifecycle_state));
    let gateway_last_cache_state = latest_gateway_route
        .and_then(|route| route.bridge.cache_state.clone());
    let gateway_last_fetch_latency_ms = latest_gateway_route
        .and_then(|route| route.bridge.fetch_latency_ms);
    let gateway_last_response_size = latest_gateway_route
        .and_then(|route| route.bridge.response_size);
    let active_sessions = state
        .sessions
        .iter()
        .filter(|session| matches!(session.session_state, void_runtime::RuntimeSessionState::Active))
        .count();
    topology.set_runtime_shell_state(RuntimeShellTopologyInfo::new(
        state.mounts.len(),
        active_sessions,
        active_permissions,
        failed_mounts,
        state.registry.len(),
        state.ui_surfaces.len(),
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
    ));
    topology.save(&topology_file)?;
    Ok(())
}

fn interactive_surface_loop(
    data_dir: &PathBuf,
    route: &str,
    shell: &mut RuntimeShell<PersistentVoidDns>,
    rendered: &mut void_runtime::ui::TerminalRenderedSurface,
    input_state: &mut BTreeMap<String, String>,
) -> Result<()> {
    let (command_tx, command_rx) = mpsc::channel::<io::Result<String>>();
    thread::spawn(move || {
        let stdin = io::stdin();
        loop {
            let mut line = String::new();
            match stdin.read_line(&mut line) {
                Ok(0) => break,
                Ok(_) => {
                    if command_tx.send(Ok(line)).is_err() {
                        break;
                    }
                }
                Err(error) => {
                    let _ = command_tx.send(Err(error));
                    break;
                }
            }
        }
    });
    let mut prompt_visible = false;

    loop {
        if !prompt_visible {
            print!("> ");
            io::stdout().flush()?;
            prompt_visible = true;
        }

        let command = match command_rx.recv_timeout(Duration::from_millis(500)) {
            Ok(Ok(line)) => {
                prompt_visible = false;
                line.trim().to_string()
            }
            Ok(Err(error)) => return Err(error.into()),
            Err(mpsc::RecvTimeoutError::Timeout) => {
                match shell.synchronize_surface(route) {
                    Ok(Some(result)) => {
                        apply_surface_sync_result(data_dir, shell, rendered, result)?;
                        prompt_visible = false;
                    }
                    Ok(None) => {}
                    Err(error) => {
                        println!("surface sync error: {error}");
                        persist_runtime_shell_topology(data_dir, shell.state())?;
                        prompt_visible = false;
                    }
                }
                continue;
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        };

        if command.is_empty() {
            continue;
        }

        if command == "quit" {
            break;
        }

        if command == "refresh" {
            match shell.render_surface_once(route, input_state.clone()) {
                Ok((next_render, events)) => {
                    apply_surface_render(data_dir, shell, rendered, next_render, events)?;
                }
                Err(error) => {
                    println!("surface refresh error: {error}");
                    persist_runtime_shell_topology(data_dir, shell.state())?;
                }
            }
            continue;
        }

        if let Some(rest) = command.strip_prefix("set ") {
            let mut parts = rest.splitn(2, ' ');
            let input_id = parts.next().unwrap_or_default().trim();
            let value = parts.next().unwrap_or_default().trim();
            if input_id.is_empty() {
                println!("usage: set <input_id> <value>");
                continue;
            }
            input_state.insert(input_id.to_string(), value.to_string());
            println!(
                "{}",
                TransportEvent::InputUpdated {
                    route: route.to_string(),
                    surface_id: current_surface_id(shell, route),
                    input_id: input_id.to_string(),
                }
                .log_line()
            );
            match shell.render_surface_once(route, input_state.clone()) {
                Ok((next_render, events)) => {
                    apply_surface_render(data_dir, shell, rendered, next_render, events)?;
                }
                Err(error) => {
                    println!("surface render error: {error}");
                    persist_runtime_shell_topology(data_dir, shell.state())?;
                }
            }
            continue;
        }

        if let Some(rest) = command.strip_prefix("press ") {
            let index = match rest.trim().parse::<usize>() {
                Ok(index) => index,
                Err(_) => {
                    println!("usage: press <index>");
                    continue;
                }
            };
            let Some(action) = rendered.actions.iter().find(|action| action.index == index).cloned() else {
                println!("unknown button index {index}");
                continue;
            };
            match shell.dispatch_surface_action(
                route,
                RuntimeActionRequest {
                    action: action.action,
                    input_state: input_state.clone(),
                },
            ) {
                Ok(result) => {
                    for event in &result.events {
                        println!("{}", event.log_line());
                    }
                    println!("{}", result.summary);
                }
                Err(error) => {
                    println!("surface action error: {error}");
                    persist_runtime_shell_topology(data_dir, shell.state())?;
                    continue;
                }
            }
            match shell.render_surface_once(route, input_state.clone()) {
                Ok((next_render, events)) => {
                    apply_surface_render(data_dir, shell, rendered, next_render, events)?;
                }
                Err(error) => {
                    println!("surface render error: {error}");
                    persist_runtime_shell_topology(data_dir, shell.state())?;
                }
            }
            continue;
        }

        println!("commands: set <input_id> <value> | press <index> | refresh | quit");
    }

    Ok(())
}

fn apply_surface_render(
    data_dir: &PathBuf,
    shell: &RuntimeShell<PersistentVoidDns>,
    rendered: &mut void_runtime::ui::TerminalRenderedSurface,
    next_render: void_runtime::ui::TerminalRenderedSurface,
    events: Vec<TransportEvent>,
) -> Result<()> {
    for event in &events {
        println!("{}", event.log_line());
    }
    *rendered = next_render;
    println!();
    println!("{}", rendered.output);
    persist_runtime_shell_topology(data_dir, shell.state())?;
    Ok(())
}

fn apply_surface_sync_result(
    data_dir: &PathBuf,
    shell: &RuntimeShell<PersistentVoidDns>,
    rendered: &mut void_runtime::ui::TerminalRenderedSurface,
    result: void_runtime::RuntimeSurfaceSyncResult,
) -> Result<()> {
    for event in &result.events {
        println!("{}", event.log_line());
    }
    if let Some(next_render) = result.rendered_surface {
        *rendered = next_render;
        println!();
        println!("{}", rendered.output);
    }
    persist_runtime_shell_topology(data_dir, shell.state())?;
    Ok(())
}

fn current_surface_id(shell: &RuntimeShell<PersistentVoidDns>, route: &str) -> String {
    shell
        .state()
        .ui_surfaces
        .iter()
        .find(|surface| surface.route == route)
        .map(|surface| surface.surface_id.clone())
        .unwrap_or_else(|| "unknown".to_string())
}

pub(crate) fn now_unix_ms() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default()
}

pub(crate) fn parse_void_uri_input(raw: &str) -> Result<VoidUri> {
    let trimmed = raw.trim();
    if trimmed.starts_with("void://") {
        return VoidUri::from_str(trimmed).map_err(Into::into);
    }
    let authority = trimmed.trim_start_matches('/').trim_end_matches('/');
    VoidUri::from_str(&format!("void://{}", authority)).map_err(Into::into)
}

pub(crate) fn render_chat_diagnostics(data_dir: &PathBuf) -> Result<Vec<String>> {
    let inbox = load_chat_inbox(data_dir)?;
    let notifications = load_chat_notifications(data_dir)?;
    let rooms = load_chat_rooms(data_dir)?;
    let sessions = load_chat_sessions(data_dir)?;
    let joined_rooms = rooms.rooms.iter().filter(|room| room.joined).count();
    let active_members = rooms
        .rooms
        .iter()
        .map(|room| room.active_members)
        .sum::<usize>();
    let last_room_event = rooms
        .rooms
        .iter()
        .flat_map(|room| room.event_history.iter().map(move |event| (room.room.as_str(), event)))
        .max_by_key(|(_, event)| event.timestamp_unix_ms)
        .map(|(room, event)| format!("{}:{}:{}", room, event.event_type, event.peer_id))
        .unwrap_or_else(|| "-".to_string());

    Ok(vec![
        format!("chat_inbox_messages={}", inbox.messages.len()),
        format!("chat_unread_messages={}", unread_count(&inbox, None)),
        format!(
            "chat_unread_notifications={}",
            notifications.notifications.iter().filter(|entry| entry.unread).count()
        ),
        format!("chat_rooms={}", rooms.rooms.len()),
        format!("chat_joined_rooms={joined_rooms}"),
        format!("chat_current_room={}", rooms.current_room.as_deref().unwrap_or("-")),
        format!("chat_room_sync_revision={}", rooms.sync_revision),
        format!("chat_room_active_members={active_members}"),
        format!("chat_sessions={}", sessions.sessions.len()),
        format!("chat_last_room_event={last_room_event}"),
    ])
}

pub(crate) fn render_gateway_diagnostics(data_dir: &PathBuf) -> Result<Vec<String>> {
    let shell_state_path = data_dir.join("runtime").join("shell.json");
    if !shell_state_path.exists() {
        return Ok(Vec::new());
    }

    let state: void_runtime::RuntimeShellState = serde_json::from_slice(&std::fs::read(&shell_state_path)?)?;
    let gateway_registrations = state
        .registry
        .iter()
        .filter(|entry| entry.surface_kind == void_runtime::RuntimeSurfaceKind::Gateway)
        .collect::<Vec<_>>();
    let gateway_mounts = state
        .mounts
        .iter()
        .filter(|mount| mount.surface_kind == void_runtime::RuntimeSurfaceKind::Gateway)
        .count();
    let active_routes = state.gateway_routes.iter().filter(|route| route.active).count();
    let bridge_failures = state.gateway_routes.iter().filter(|route| route.last_error.is_some()).count();
    let bridge_sessions = state.gateway_bridge_sessions.len();
    let snapshot_entries = count_gateway_snapshot_entries(data_dir)?;
    let trust_warnings = state
        .gateway_trust
        .iter()
        .filter(|policy| policy.last_warning.is_some())
        .count();
    let last_route = state
        .gateway_routes
        .iter()
        .max_by_key(|route| route.updated_unix_ms);

    Ok(vec![
        format!("gateway_registrations={}", gateway_registrations.len()),
        format!("gateway_mounts={gateway_mounts}"),
        format!("gateway_active_routes={active_routes}"),
        format!("gateway_bridge_failures={bridge_failures}"),
        format!("gateway_bridge_sessions={bridge_sessions}"),
        format!("gateway_snapshot_entries={snapshot_entries}"),
        format!("gateway_trust_warnings={trust_warnings}"),
        format!(
            "gateway_last_external_target={}",
            last_route
                .map(|route| route.bridge.external_target.clone())
                .unwrap_or_else(|| "-".to_string())
        ),
        format!(
            "gateway_last_bridge_state={}",
            last_route
                .map(|route| format!("{:?}", route.bridge.lifecycle_state))
                .unwrap_or_else(|| "-".to_string())
        ),
        format!(
            "gateway_last_fetch_latency_ms={}",
            last_route
                .and_then(|route| route.bridge.fetch_latency_ms)
                .map(|latency| latency.to_string())
                .unwrap_or_else(|| "-".to_string())
        ),
        format!(
            "gateway_last_response_size={}",
            last_route
                .and_then(|route| route.bridge.response_size)
                .map(|size| size.to_string())
                .unwrap_or_else(|| "-".to_string())
        ),
        format!(
            "gateway_last_cache_state={}",
            last_route
                .and_then(|route| route.bridge.cache_state.clone())
                .unwrap_or_else(|| "-".to_string())
        ),
        format!(
            "gateway_domains={}",
            if gateway_registrations.is_empty() {
                "-".to_string()
            } else {
                gateway_registrations
                    .iter()
                    .map(|entry| entry.domain.clone())
                    .collect::<Vec<_>>()
                    .join(",")
            }
        ),
    ])
}

fn count_gateway_snapshot_entries(data_dir: &PathBuf) -> Result<usize> {
    let gateway_dir = data_dir.join("gateway");
    if !gateway_dir.exists() {
        return Ok(0);
    }

    fn count_dir(path: &std::path::Path) -> Result<usize> {
        let mut count = 0;
        for entry in std::fs::read_dir(path)? {
            let entry = entry?;
            let entry_path = entry.path();
            if entry_path.is_dir() {
                count += count_dir(&entry_path)?;
            } else if entry_path.extension().and_then(|extension| extension.to_str()) == Some("json") {
                count += 1;
            }
        }
        Ok(count)
    }

    count_dir(&gateway_dir)
}

fn parse_gateway_trust_level(raw: &str) -> Result<GatewayTrustLevel> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "trusted" => Ok(GatewayTrustLevel::Trusted),
        "restricted" => Ok(GatewayTrustLevel::Restricted),
        "untrusted" | "denied" => Ok(GatewayTrustLevel::Untrusted),
        other => anyhow::bail!("invalid gateway trust level: {other}"),
    }
}
