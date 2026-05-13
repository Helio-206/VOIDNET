use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::Arc,
};
#[cfg(feature = "desktop-shell")]
use tauri::State;
#[cfg(feature = "desktop-shell")]
use tokio::sync::Mutex;
use void_dns::PersistentVoidDns;
use void_identity::{default_node_dir, NodeIdentity, PersistentNodeIdentity};
use void_protocol::VoidUri;
use void_runtime::{
    ui::RuntimeActionRequest, GatewayTrustLevel, GatewayTrustState, RuntimeConfig,
    RuntimeError, RuntimeSessionState, RuntimeShell, RuntimeSurfaceKind, RuntimeSurfaceRegistration,
    RuntimeSurfaceView, VoidRuntime,
};
use void_transport::{
    event::TransportEvent, network_channels, PeerConnectionState, PeerTopology,
};

#[cfg(feature = "desktop-shell")]
pub struct BrowserAppState {
    browser: Mutex<BrowserSurface>,
}

pub struct BrowserSurface {
    data_dir: PathBuf,
    identity: PersistentNodeIdentity,
    shell: RuntimeShell<PersistentVoidDns>,
    current_uri: Option<String>,
    mounted_route: Option<String>,
    pending_prompts: Vec<BrowserPermissionPrompt>,
    event_log: Vec<BrowserEventRecord>,
    next_prompt_id: u64,
    last_error: Option<String>,
}

impl BrowserSurface {
    pub fn cold_start(data_dir: Option<PathBuf>) -> Result<Self, BrowserError> {
        let data_dir = data_dir.unwrap_or_else(default_node_dir);
        let identity = PersistentNodeIdentity::load_or_create_dir(&data_dir)?;
        let dns = Arc::new(PersistentVoidDns::load_or_create(&data_dir)?);
        let (network, _inbox) = network_channels(32);
        let runtime = VoidRuntime::new(NodeIdentity::generate(), dns, network, RuntimeConfig::default());
        let mut shell = RuntimeShell::load_or_create(&data_dir, runtime)?;
        shell.reconcile_registry_owner(&identity.peer_id_string())?;

        Ok(Self {
            data_dir,
            identity,
            shell,
            current_uri: None,
            mounted_route: None,
            pending_prompts: Vec::new(),
            event_log: Vec::new(),
            next_prompt_id: 1,
            last_error: None,
        })
    }

    pub fn snapshot(&self) -> Result<BrowserSnapshot, BrowserError> {
        let surface = self
            .mounted_route
            .as_ref()
            .map(|route| self.shell.surface_view(route))
            .transpose()?;
        Ok(BrowserSnapshot {
            current_uri: self.current_uri.clone(),
            mounted_route: self.mounted_route.clone(),
            surface,
            diagnostics: self.diagnostics()?,
            pending_prompts: self.pending_prompts.clone(),
            events: self.event_log.clone(),
            last_error: self.last_error.clone(),
        })
    }

    pub async fn navigate(&mut self, raw: &str) -> Result<BrowserSnapshot, BrowserError> {
        let uri = raw.parse::<VoidUri>()?;
        self.current_uri = Some(uri.to_string());
        self.last_error = None;
        self.shell.synchronize_registry_dns(&self.identity).await?;

        match self.shell.open_uri(uri.clone()).await {
            Ok(result) => {
                self.mounted_route = Some(result.route.uri.to_string());
                self.pending_prompts.clear();
                self.record_events(&result.events);
                self.last_error = None;
                self.snapshot()
            }
            Err(error) => {
                self.capture_permission_prompts(&uri, &error)?;
                self.record_runtime_error(&uri, &error);
                self.last_error = Some(error.to_string());
                self.snapshot()
            }
        }
    }

    pub fn dispatch_action(
        &mut self,
        action: String,
        input_state: BTreeMap<String, String>,
    ) -> Result<BrowserSnapshot, BrowserError> {
        let Some(route) = self.mounted_route.clone() else {
            return self.snapshot();
        };

        match self.shell.dispatch_surface_action(
            &route,
            RuntimeActionRequest {
                action,
                input_state: input_state.clone(),
            },
        ) {
            Ok(result) => {
                self.record_events(&result.events);
                if let Ok((_, render_events)) = self.shell.render_surface_once(&route, input_state) {
                    self.record_events(&render_events);
                }
                self.last_error = None;
            }
            Err(error) => {
                self.capture_permission_prompts_from_route(&route, &error)?;
                self.record_runtime_error_for_route(&route, &error);
                self.last_error = Some(error.to_string());
            }
        }

        self.snapshot()
    }

    pub fn sync(&mut self) -> Result<BrowserSnapshot, BrowserError> {
        if let Some(route) = self.mounted_route.clone() {
            if let Some(result) = self.shell.synchronize_surface(&route)? {
                self.record_events(&result.events);
            }
        }
        self.snapshot()
    }

    pub async fn resolve_prompt(
        &mut self,
        prompt_id: u64,
        allowed: bool,
    ) -> Result<BrowserSnapshot, BrowserError> {
        let Some(index) = self.pending_prompts.iter().position(|prompt| prompt.id == prompt_id) else {
            return self.snapshot();
        };
        let prompt = self.pending_prompts.remove(index);

        match prompt.kind {
            BrowserPermissionPromptKind::RuntimeCapability => {
                self.shell.grant_permission(
                    &prompt.surface_id,
                    &prompt.peer_owner,
                    &prompt.capability,
                    allowed,
                )?;
            }
            BrowserPermissionPromptKind::GatewayCapability => {
                self.shell.grant_permission(
                    &prompt.surface_id,
                    &prompt.peer_owner,
                    &prompt.capability,
                    allowed,
                )?;
                if let Some(domain) = &prompt.gateway_domain {
                    self.shell.record_gateway_permission_decision(
                        domain,
                        &prompt.capability,
                        allowed,
                        if allowed { "browser allow" } else { "browser deny" },
                    )?;
                    self.shell.set_gateway_trust(
                        domain,
                        if allowed {
                            GatewayTrustState::Trusted
                        } else {
                            GatewayTrustState::Denied
                        },
                        if allowed {
                            GatewayTrustLevel::Trusted
                        } else {
                            GatewayTrustLevel::Untrusted
                        },
                        if allowed {
                            "browser permission allowed"
                        } else {
                            "browser permission denied"
                        },
                    )?;
                }
            }
            BrowserPermissionPromptKind::GatewayTrust => {
                if let Some(domain) = &prompt.gateway_domain {
                    self.shell.record_gateway_permission_decision(
                        domain,
                        if allowed { "trusted" } else { "untrusted" },
                        allowed,
                        if allowed { "browser trust allow" } else { "browser trust deny" },
                    )?;
                    self.shell.set_gateway_trust(
                        domain,
                        if allowed {
                            GatewayTrustState::Trusted
                        } else {
                            GatewayTrustState::Denied
                        },
                        if allowed {
                            GatewayTrustLevel::Trusted
                        } else {
                            GatewayTrustLevel::Untrusted
                        },
                        if allowed {
                            "browser trust allowed"
                        } else {
                            "browser trust denied"
                        },
                    )?;
                }
            }
        }

        if let Some(route) = self.current_uri.clone() {
            return self.navigate(&route).await;
        }

        self.snapshot()
    }

    fn diagnostics(&self) -> Result<BrowserDiagnosticsSnapshot, BrowserError> {
        let state = self.shell.state();
        let topology = load_topology(&self.data_dir)?;
        let active_peers = topology
            .as_ref()
            .map(|topology| {
                topology
                    .peers
                    .values()
                    .filter(|peer| matches!(peer.state, PeerConnectionState::Active | PeerConnectionState::Syncing))
                    .count()
            })
            .unwrap_or_default();

        Ok(BrowserDiagnosticsSnapshot {
            mounted_surfaces: state.mounts.len(),
            active_sessions: state
                .sessions
                .iter()
                .filter(|session| session.session_state == RuntimeSessionState::Active)
                .count(),
            active_peers,
            active_gateways: state
                .registry
                .iter()
                .filter(|entry| entry.surface_kind == RuntimeSurfaceKind::Gateway)
                .count(),
            active_permissions: state.permissions.iter().filter(|grant| grant.allowed).count(),
            gateway_bridge_sessions: state.gateway_bridge_sessions.len(),
            gateway_fetch_failures: state
                .gateway_routes
                .iter()
                .filter(|route| route.last_error.is_some())
                .count(),
            topology_state: topology
                .as_ref()
                .map(|topology| topology.mesh_state.to_string())
                .unwrap_or_else(|| "BOOTSTRAPPING".to_string()),
            last_route: state
                .gateway_routes
                .iter()
                .max_by_key(|route| route.updated_unix_ms)
                .map(|route| route.route.clone()),
            last_external_target: state
                .gateway_routes
                .iter()
                .max_by_key(|route| route.updated_unix_ms)
                .map(|route| route.bridge.external_target.clone()),
            last_fetch_latency_ms: state
                .gateway_routes
                .iter()
                .max_by_key(|route| route.updated_unix_ms)
                .and_then(|route| route.bridge.fetch_latency_ms),
            mounts: state
                .mounts
                .iter()
                .map(|mount| BrowserMountedSurfaceInfo {
                    route: mount.route.clone(),
                    surface_id: mount.surface_id.clone(),
                    owner_peer: mount.owner_peer.clone(),
                    surface_kind: format!("{:?}", mount.surface_kind),
                    mount_state: format!("{:?}", mount.mount_state),
                    session_id: mount.session_id.clone(),
                })
                .collect(),
            sessions: state
                .sessions
                .iter()
                .map(|session| BrowserRuntimeSessionInfo {
                    session_id: session.session_id.clone(),
                    surface: session.mounted_surface.clone(),
                    peer_owner: session.peer_owner.clone(),
                    state: format!("{:?}", session.session_state),
                    active_capabilities: session.active_capabilities.clone(),
                    last_activity_unix_ms: session.last_activity_unix_ms,
                })
                .collect(),
            permissions: state
                .permissions
                .iter()
                .map(|grant| BrowserPermissionGrantInfo {
                    surface_id: grant.surface_id.clone(),
                    peer_owner: grant.peer_owner.clone(),
                    capability: grant.capability.clone(),
                    allowed: grant.allowed,
                })
                .collect(),
            gateways: state
                .gateway_trust
                .iter()
                .map(|gateway| BrowserGatewayInfo {
                    domain: gateway.gateway_domain.clone(),
                    gateway_id: gateway.gateway_id.clone(),
                    trust_state: format!("{:?}", gateway.trust_state),
                    trust_level: format!("{:?}", gateway.trust_level),
                    warning: gateway.last_warning.clone(),
                    active_routes: state
                        .gateway_routes
                        .iter()
                        .filter(|route| route.gateway_domain == gateway.gateway_domain && route.active)
                        .count(),
                    bridge_sessions: state
                        .gateway_bridge_sessions
                        .iter()
                        .filter(|session| session.gateway_domain == gateway.gateway_domain)
                        .count(),
                })
                .collect(),
            peers: topology
                .as_ref()
                .map(|topology| {
                    topology
                        .peers
                        .values()
                        .map(|peer| BrowserPeerInfo {
                            peer_id: peer.peer_id.clone(),
                            state: peer.state.to_string(),
                            latency_ms: peer.latency_ms,
                            encrypted: peer.session.encrypted,
                            transport_health: peer.transport_health.to_string(),
                        })
                        .collect()
                })
                .unwrap_or_default(),
            bridge_sessions: state
                .gateway_bridge_sessions
                .iter()
                .map(|session| BrowserGatewayBridgeSessionInfo {
                    session_id: session.session_id.clone(),
                    gateway_domain: session.gateway_domain.clone(),
                    external_target: session.external_target.clone(),
                    fetch_state: session.fetch_state.clone(),
                    response_state: session.response_state.clone(),
                    permission_state: session.permission_state.clone(),
                    mount_state: format!("{:?}", session.mount_state),
                    fetch_latency_ms: session.fetch_latency_ms,
                    response_size: session.response_size,
                })
                .collect(),
        })
    }

    fn record_events(&mut self, events: &[TransportEvent]) {
        for event in events {
            self.event_log.push(BrowserEventRecord {
                observed_at_unix_ms: now_unix_ms(),
                line: event.log_line(),
            });
        }
        if self.event_log.len() > 96 {
            let keep_from = self.event_log.len() - 96;
            self.event_log.drain(0..keep_from);
        }
    }

    fn record_runtime_error(&mut self, uri: &VoidUri, error: &RuntimeError) {
        self.event_log.push(BrowserEventRecord {
            observed_at_unix_ms: now_unix_ms(),
            line: format!("[VOIDNET][BROWSER] NavigationFailed route={} error={error}", uri),
        });
    }

    fn record_runtime_error_for_route(&mut self, route: &str, error: &RuntimeError) {
        self.event_log.push(BrowserEventRecord {
            observed_at_unix_ms: now_unix_ms(),
            line: format!("[VOIDNET][BROWSER] ActionFailed route={route} error={error}"),
        });
    }

    fn capture_permission_prompts(&mut self, uri: &VoidUri, error: &RuntimeError) -> Result<(), BrowserError> {
        self.capture_permission_prompts_from_route(&uri.to_string(), error)
    }

    fn capture_permission_prompts_from_route(
        &mut self,
        route: &str,
        error: &RuntimeError,
    ) -> Result<(), BrowserError> {
        match error {
            RuntimeError::PermissionDenied {
                surface_id,
                missing_capabilities,
            } => {
                let registration = find_registration_by_route(self.shell.state().registry.as_slice(), route)
                    .or_else(|| {
                        self.shell
                            .state()
                            .registry
                            .iter()
                            .find(|entry| entry.surface_id == *surface_id)
                    })
                    .cloned();
                let peer_owner = registration
                    .as_ref()
                    .map(|entry| entry.owner_peer_id.clone())
                    .unwrap_or_default();
                let gateway_domain = registration
                    .as_ref()
                    .filter(|entry| entry.surface_kind == RuntimeSurfaceKind::Gateway)
                    .map(|entry| entry.domain.clone());
                for capability in missing_capabilities {
                    let prompt_id = self.next_prompt_id();
                    self.push_prompt(BrowserPermissionPrompt {
                        id: prompt_id,
                        route: route.to_string(),
                        surface_id: surface_id.clone(),
                        peer_owner: peer_owner.clone(),
                        gateway_domain: gateway_domain.clone(),
                        capability: capability.clone(),
                        title: format!("Permission required: {capability}"),
                        description: format!(
                            "The runtime requested `{capability}` for surface `{surface_id}` while mounting `{route}`."
                        ),
                        kind: if gateway_domain.is_some() {
                            BrowserPermissionPromptKind::GatewayCapability
                        } else {
                            BrowserPermissionPromptKind::RuntimeCapability
                        },
                    });
                }
            }
            RuntimeError::GatewayTrustDenied(domain) => {
                let prompt_id = self.next_prompt_id();
                self.push_prompt(BrowserPermissionPrompt {
                    id: prompt_id,
                    route: route.to_string(),
                    surface_id: format!("gateway:{domain}"),
                    peer_owner: String::new(),
                    gateway_domain: Some(domain.clone()),
                    capability: "gateway.trust".to_string(),
                    title: format!("Gateway trust required: {domain}"),
                    description: format!(
                        "The runtime blocked `{route}` because gateway trust for `{domain}` is denied or untrusted."
                    ),
                    kind: BrowserPermissionPromptKind::GatewayTrust,
                });
            }
            _ => {}
        }
        Ok(())
    }

    fn push_prompt(&mut self, prompt: BrowserPermissionPrompt) {
        if self.pending_prompts.iter().any(|existing| {
            existing.route == prompt.route
                && existing.surface_id == prompt.surface_id
                && existing.capability == prompt.capability
                && existing.kind == prompt.kind
        }) {
            return;
        }
        self.pending_prompts.push(prompt);
    }

    fn next_prompt_id(&mut self) -> u64 {
        let next = self.next_prompt_id;
        self.next_prompt_id = self.next_prompt_id.saturating_add(1);
        next
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BrowserSnapshot {
    pub current_uri: Option<String>,
    pub mounted_route: Option<String>,
    pub surface: Option<RuntimeSurfaceView>,
    pub diagnostics: BrowserDiagnosticsSnapshot,
    pub pending_prompts: Vec<BrowserPermissionPrompt>,
    pub events: Vec<BrowserEventRecord>,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BrowserDiagnosticsSnapshot {
    pub mounted_surfaces: usize,
    pub active_sessions: usize,
    pub active_peers: usize,
    pub active_gateways: usize,
    pub active_permissions: usize,
    pub gateway_bridge_sessions: usize,
    pub gateway_fetch_failures: usize,
    pub topology_state: String,
    pub last_route: Option<String>,
    pub last_external_target: Option<String>,
    pub last_fetch_latency_ms: Option<u128>,
    pub mounts: Vec<BrowserMountedSurfaceInfo>,
    pub sessions: Vec<BrowserRuntimeSessionInfo>,
    pub permissions: Vec<BrowserPermissionGrantInfo>,
    pub gateways: Vec<BrowserGatewayInfo>,
    pub peers: Vec<BrowserPeerInfo>,
    pub bridge_sessions: Vec<BrowserGatewayBridgeSessionInfo>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BrowserMountedSurfaceInfo {
    pub route: String,
    pub surface_id: String,
    pub owner_peer: String,
    pub surface_kind: String,
    pub mount_state: String,
    pub session_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BrowserRuntimeSessionInfo {
    pub session_id: String,
    pub surface: String,
    pub peer_owner: String,
    pub state: String,
    pub active_capabilities: Vec<String>,
    pub last_activity_unix_ms: u128,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BrowserPermissionGrantInfo {
    pub surface_id: String,
    pub peer_owner: String,
    pub capability: String,
    pub allowed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BrowserGatewayInfo {
    pub domain: String,
    pub gateway_id: String,
    pub trust_state: String,
    pub trust_level: String,
    pub warning: Option<String>,
    pub active_routes: usize,
    pub bridge_sessions: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BrowserPeerInfo {
    pub peer_id: String,
    pub state: String,
    pub latency_ms: Option<u128>,
    pub encrypted: bool,
    pub transport_health: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BrowserGatewayBridgeSessionInfo {
    pub session_id: String,
    pub gateway_domain: String,
    pub external_target: String,
    pub fetch_state: String,
    pub response_state: String,
    pub permission_state: String,
    pub mount_state: String,
    pub fetch_latency_ms: Option<u128>,
    pub response_size: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BrowserEventRecord {
    pub observed_at_unix_ms: u128,
    pub line: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BrowserPermissionPrompt {
    pub id: u64,
    pub route: String,
    pub surface_id: String,
    pub peer_owner: String,
    pub gateway_domain: Option<String>,
    pub capability: String,
    pub title: String,
    pub description: String,
    pub kind: BrowserPermissionPromptKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BrowserPermissionPromptKind {
    RuntimeCapability,
    GatewayCapability,
    GatewayTrust,
}

#[derive(Debug, Deserialize)]
pub struct BrowserActionPayload {
    pub action: String,
    #[serde(default)]
    pub input_state: BTreeMap<String, String>,
}

#[cfg(feature = "desktop-shell")]
#[tauri::command]
async fn browser_snapshot(state: State<'_, BrowserAppState>) -> Result<BrowserSnapshot, String> {
    let browser = state.browser.lock().await;
    browser.snapshot().map_err(|error| error.to_string())
}

#[cfg(feature = "desktop-shell")]
#[tauri::command]
async fn browser_navigate(
    route: String,
    state: State<'_, BrowserAppState>,
) -> Result<BrowserSnapshot, String> {
    let mut browser = state.browser.lock().await;
    browser.navigate(&route).await.map_err(|error| error.to_string())
}

#[cfg(feature = "desktop-shell")]
#[tauri::command]
async fn browser_sync(state: State<'_, BrowserAppState>) -> Result<BrowserSnapshot, String> {
    let mut browser = state.browser.lock().await;
    browser.sync().map_err(|error| error.to_string())
}

#[cfg(feature = "desktop-shell")]
#[tauri::command]
async fn browser_dispatch_action(
    payload: BrowserActionPayload,
    state: State<'_, BrowserAppState>,
) -> Result<BrowserSnapshot, String> {
    let mut browser = state.browser.lock().await;
    browser
        .dispatch_action(payload.action, payload.input_state)
        .map_err(|error| error.to_string())
}

#[cfg(feature = "desktop-shell")]
#[tauri::command]
async fn browser_resolve_prompt(
    prompt_id: u64,
    allowed: bool,
    state: State<'_, BrowserAppState>,
) -> Result<BrowserSnapshot, String> {
    let mut browser = state.browser.lock().await;
    browser
        .resolve_prompt(prompt_id, allowed)
        .await
        .map_err(|error| error.to_string())
}

#[cfg(feature = "desktop-shell")]
pub fn run() -> Result<()> {
    let browser = BrowserSurface::cold_start(None)?;
    tauri::Builder::default()
        .manage(BrowserAppState {
            browser: Mutex::new(browser),
        })
        .invoke_handler(tauri::generate_handler![
            browser_snapshot,
            browser_navigate,
            browser_sync,
            browser_dispatch_action,
            browser_resolve_prompt,
        ])
        .run(tauri::generate_context!())?;
    Ok(())
}

#[cfg(not(feature = "desktop-shell"))]
pub fn run() -> Result<()> {
    anyhow::bail!(
        "VOIDBrowser desktop shell is disabled. Install Linux GTK/WebKit system libraries and run with --features desktop-shell."
    )
}

fn load_topology(data_dir: &Path) -> Result<Option<PeerTopology>, BrowserError> {
    let topology_path = data_dir.join("topology.json");
    if !topology_path.exists() {
        return Ok(None);
    }
    Ok(Some(PeerTopology::load(&topology_path)?))
}

fn find_registration_by_route<'a>(
    registry: &'a [RuntimeSurfaceRegistration],
    route: &str,
) -> Option<&'a RuntimeSurfaceRegistration> {
    let authority = route
        .strip_prefix("void://")
        .unwrap_or(route)
        .split('/')
        .next()
        .unwrap_or(route);
    registry
        .iter()
        .find(|entry| entry.domain == authority || entry.entry_uri.authority() == authority)
}

fn now_unix_ms() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default()
}

#[derive(Debug, thiserror::Error)]
pub enum BrowserError {
    #[error("browser IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("browser runtime error: {0}")]
    Runtime(#[from] RuntimeError),
    #[error("browser protocol error: {0}")]
    Protocol(#[from] void_protocol::ParseVoidUriError),
    #[error("browser DNS error: {0}")]
    Dns(#[from] void_dns::VoidDnsError),
    #[error("browser identity error: {0}")]
    Identity(#[from] void_identity::IdentityError),
    #[error("browser topology error: {0}")]
    Topology(#[from] void_transport::topology::TopologyError),
}
