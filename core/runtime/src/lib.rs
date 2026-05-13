pub mod ui;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs, io,
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use thiserror::Error;
use ui::{
    build_runtime_tree, count_bound_nodes, parse_surface_document, render_terminal_surface,
    RuntimeActionRequest, RuntimeActionResult, RuntimeUiTree, TerminalRenderedSurface,
};
use void_chat::{
    enqueue_local_command, load_chat_inbox, load_chat_notifications, load_chat_rooms,
    load_chat_sessions, unread_count, ChatError, ChatLocalCommand,
};
use void_dns::{
    DnsResolvedRoute, PersistentVoidDns, ResolutionTarget, VoidDnsResolver, VoidDomain,
};
use void_identity::{NodeIdentity, PersistentNodeIdentity};
use void_protocol::{Envelope, VoidUri};
use void_transport::{event::TransportEvent, NetworkHandle, TransportCommand, TransportError};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppManifest {
    pub id: String,
    pub entry: VoidUri,
    pub permissions: Vec<Permission>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeSurfaceRegistration {
    pub domain: String,
    pub surface_id: String,
    pub runtime_surface: String,
    pub entry_uri: VoidUri,
    pub owner_peer_id: String,
    pub capabilities: Vec<String>,
    pub handler: String,
    #[serde(default)]
    pub surface_kind: RuntimeSurfaceKind,
    #[serde(default)]
    pub supported_protocols: Vec<String>,
    #[serde(default)]
    pub external_route_base: Option<String>,
    #[serde(default)]
    pub trust_level: Option<GatewayTrustLevel>,
    #[serde(default)]
    pub surface_path: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum RuntimeSurfaceKind {
    #[default]
    Application,
    Gateway,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum GatewayTrustLevel {
    Trusted,
    #[default]
    Restricted,
    Untrusted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum GatewayTrustState {
    Trusted,
    Warning,
    #[default]
    Pending,
    Denied,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum GatewayBridgeLifecycleState {
    #[default]
    Prepared,
    Mounted,
    Streaming,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GatewayPermissionDecision {
    pub capability: String,
    pub allowed: bool,
    pub reason: String,
    pub decided_at_unix_ms: u128,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GatewayTrustPolicy {
    pub gateway_domain: String,
    pub gateway_id: String,
    pub owner_peer: String,
    pub trust_level: GatewayTrustLevel,
    pub trust_state: GatewayTrustState,
    pub capability_scope: Vec<String>,
    #[serde(default)]
    pub runtime_restrictions: Vec<String>,
    #[serde(default)]
    pub permission_history: Vec<GatewayPermissionDecision>,
    #[serde(default)]
    pub last_warning: Option<String>,
    pub updated_unix_ms: u128,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HttpBridgeRequest {
    pub method: String,
    pub target_path: String,
    #[serde(default)]
    pub url: String,
    pub headers: BTreeMap<String, String>,
    #[serde(default)]
    pub query: BTreeMap<String, String>,
    #[serde(default)]
    pub body: Option<Vec<u8>>,
    #[serde(default)]
    pub timeout_ms: u64,
    #[serde(default)]
    pub gateway_peer: String,
    #[serde(default)]
    pub request_id: String,
    pub issued_at_unix_ms: u128,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HttpBridgeResponse {
    pub status: u16,
    pub headers: BTreeMap<String, String>,
    #[serde(default)]
    pub body: Option<Vec<u8>>,
    #[serde(default)]
    pub content_type: Option<String>,
    #[serde(default)]
    pub body_bytes: Option<usize>,
    #[serde(default)]
    pub response_size: usize,
    #[serde(default)]
    pub fetched_at_unix_ms: Option<u128>,
    #[serde(default)]
    pub gateway_peer: String,
    #[serde(default)]
    pub response_id: String,
    #[serde(default)]
    pub body_truncated: bool,
    #[serde(default)]
    pub cacheable: bool,
    #[serde(default)]
    pub completed_at_unix_ms: Option<u128>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HttpBridgeStreamChunk {
    pub sequence: u64,
    pub bytes: usize,
    pub final_chunk: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GatewayBridgeContext {
    pub gateway_domain: String,
    pub gateway_id: String,
    pub owner_peer: String,
    pub supported_protocols: Vec<String>,
    pub active_external_route: String,
    pub external_target: String,
    pub lifecycle_state: GatewayBridgeLifecycleState,
    pub request: HttpBridgeRequest,
    #[serde(default)]
    pub response: Option<HttpBridgeResponse>,
    #[serde(default)]
    pub stream_chunks: Vec<HttpBridgeStreamChunk>,
    #[serde(default)]
    pub last_error: Option<String>,
    #[serde(default)]
    pub fetch_latency_ms: Option<u128>,
    #[serde(default)]
    pub response_size: Option<usize>,
    #[serde(default)]
    pub snapshot_id: Option<String>,
    #[serde(default)]
    pub cache_state: Option<String>,
    #[serde(default)]
    pub render_mode: Option<String>,
    pub created_at_unix_ms: u128,
    pub updated_at_unix_ms: u128,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GatewayRouteState {
    pub route: String,
    pub gateway_domain: String,
    pub gateway_id: String,
    pub trust_state: GatewayTrustState,
    pub session_id: String,
    pub active: bool,
    pub bridge: GatewayBridgeContext,
    #[serde(default)]
    pub last_error: Option<String>,
    pub updated_unix_ms: u128,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GatewayBridgeSessionRecord {
    pub session_id: String,
    pub gateway_domain: String,
    pub gateway_id: String,
    pub gateway_peer: String,
    pub external_target: String,
    pub fetch_state: String,
    pub response_state: String,
    pub permission_state: String,
    pub mount_state: MountState,
    pub started_at_unix_ms: u128,
    pub last_activity_unix_ms: u128,
    #[serde(default)]
    pub fetch_latency_ms: Option<u128>,
    #[serde(default)]
    pub response_size: Option<usize>,
    #[serde(default)]
    pub content_type: Option<String>,
    #[serde(default)]
    pub cache_state: Option<String>,
    #[serde(default)]
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GatewayResourceSnapshot {
    pub snapshot_id: String,
    pub request_id: String,
    pub response_id: String,
    pub route: String,
    pub gateway_domain: String,
    pub gateway_peer: String,
    pub external_target: String,
    pub status: u16,
    pub content_type: String,
    pub response_size: usize,
    pub body_preview: String,
    #[serde(default)]
    pub headers: BTreeMap<String, String>,
    pub fetched_at_unix_ms: u128,
    pub cache_state: String,
    pub cache_key: String,
    pub origin: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RuntimeLifecycleState {
    Unresolved,
    Resolving,
    Negotiating,
    Mounting,
    Active,
    Suspended,
    Failed,
    Unmounted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MountState {
    Pending,
    Mounted,
    Failed,
    Unmounted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RuntimeSessionState {
    Negotiating,
    Active,
    Suspended,
    Closed,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimePermissionGrant {
    pub surface_id: String,
    pub peer_owner: String,
    pub capability: String,
    pub allowed: bool,
    pub persisted: bool,
    pub requested_at_unix_ms: u128,
    pub decided_at_unix_ms: u128,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeSessionRecord {
    pub session_id: String,
    pub mounted_surface: String,
    pub peer_owner: String,
    pub active_capabilities: Vec<String>,
    pub encryption_state: String,
    pub started_at_unix_ms: u128,
    pub last_activity_unix_ms: u128,
    pub session_state: RuntimeSessionState,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MountedRuntimeSurface {
    pub route: String,
    pub surface_id: String,
    pub owner_peer: String,
    pub capabilities: Vec<String>,
    #[serde(default)]
    pub surface_kind: RuntimeSurfaceKind,
    pub runtime_state: RuntimeLifecycleState,
    pub mount_state: MountState,
    pub session_state: RuntimeSessionState,
    pub mounted_at_unix_ms: u128,
    pub last_activity_unix_ms: u128,
    pub session_id: String,
    pub mount_latency_ms: u128,
    pub failure_reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct RuntimeShellState {
    pub registry: Vec<RuntimeSurfaceRegistration>,
    pub mounts: Vec<MountedRuntimeSurface>,
    pub sessions: Vec<RuntimeSessionRecord>,
    pub permissions: Vec<RuntimePermissionGrant>,
    #[serde(default)]
    pub gateway_trust: Vec<GatewayTrustPolicy>,
    #[serde(default)]
    pub gateway_routes: Vec<GatewayRouteState>,
    #[serde(default)]
    pub gateway_bridge_sessions: Vec<GatewayBridgeSessionRecord>,
    pub ui_surfaces: Vec<RuntimeUiSurfaceState>,
    pub updated_unix_ms: u128,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenSurfaceResult {
    pub route: DnsResolvedRoute,
    pub mount: MountedRuntimeSurface,
    pub session: RuntimeSessionRecord,
    pub granted_permissions: Vec<RuntimePermissionGrant>,
    pub rejected_capabilities: Vec<String>,
    pub rendered_surface: TerminalRenderedSurface,
    pub events: Vec<TransportEvent>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeSurfaceView {
    pub route: String,
    pub surface_id: String,
    pub surface_kind: RuntimeSurfaceKind,
    pub tree: RuntimeUiTree,
    pub bindings: BTreeMap<String, String>,
    #[serde(default)]
    pub input_state: BTreeMap<String, String>,
    #[serde(default)]
    pub last_error: Option<String>,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub owner_peer: Option<String>,
    pub last_updated_unix_ms: u128,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeUiSurfaceState {
    pub route: String,
    pub surface_id: String,
    pub node_count: usize,
    pub input_state: BTreeMap<String, String>,
    #[serde(default)]
    pub local_state: BTreeMap<String, String>,
    #[serde(default)]
    pub runtime_state: BTreeMap<String, String>,
    #[serde(default)]
    pub distributed_state: BTreeMap<String, String>,
    #[serde(default)]
    pub source_snapshot: RuntimeSurfaceSourceSnapshot,
    #[serde(default)]
    pub state_revision: u64,
    #[serde(default)]
    pub rerender_count: u64,
    #[serde(default)]
    pub hot_reload_count: u64,
    #[serde(default)]
    pub sync_count: u64,
    #[serde(default)]
    pub permission_denials: u64,
    #[serde(default)]
    pub last_changed_bindings: Vec<String>,
    pub last_action: Option<String>,
    #[serde(default)]
    pub last_handler: Option<String>,
    #[serde(default)]
    pub last_error: Option<String>,
    pub last_render_duration_ms: u128,
    #[serde(default)]
    pub last_source_modified_unix_ms: Option<u128>,
    pub last_updated_unix_ms: u128,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct RuntimeSurfaceSourceSnapshot {
    #[serde(default)]
    pub surface_hash: String,
    #[serde(default)]
    pub binding_hash: String,
    #[serde(default)]
    pub state_hash: String,
    #[serde(default)]
    pub surface_modified_unix_ms: Option<u128>,
    #[serde(default)]
    pub source_modified_unix_ms: Option<u128>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeSurfaceSyncResult {
    pub rendered_surface: Option<TerminalRenderedSurface>,
    pub events: Vec<TransportEvent>,
    pub changed: bool,
}

#[derive(Debug, Clone)]
struct ResolvedSurfaceFrame {
    tree: RuntimeUiTree,
    bindings: BTreeMap<String, String>,
    source_snapshot: RuntimeSurfaceSourceSnapshot,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Permission {
    Network,
    IdentitySign,
    Storage { namespace: String },
    Stream { authority: String },
}

#[derive(Debug, Clone)]
pub struct RuntimeConfig {
    pub app_sandbox_enabled: bool,
    pub max_frame_bytes: usize,
    pub auto_grant_safe_capabilities: bool,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            app_sandbox_enabled: true,
            max_frame_bytes: void_protocol::MAX_FRAME_BYTES,
            auto_grant_safe_capabilities: true,
        }
    }
}

pub struct VoidRuntime<R>
where
    R: VoidDnsResolver + Send + Sync + 'static,
{
    identity: NodeIdentity,
    dns: Arc<R>,
    network: NetworkHandle,
    config: RuntimeConfig,
}

pub struct RuntimeShell<R>
where
    R: VoidDnsResolver + Send + Sync + 'static,
{
    runtime: VoidRuntime<R>,
    data_dir: PathBuf,
    state_path: PathBuf,
    state: RuntimeShellState,
}

impl<R> VoidRuntime<R>
where
    R: VoidDnsResolver + Send + Sync + 'static,
{
    pub fn new(
        identity: NodeIdentity,
        dns: Arc<R>,
        network: NetworkHandle,
        config: RuntimeConfig,
    ) -> Self {
        Self {
            identity,
            dns,
            network,
            config,
        }
    }

    pub fn identity(&self) -> &NodeIdentity {
        &self.identity
    }

    pub fn config(&self) -> &RuntimeConfig {
        &self.config
    }
}

impl<R> RuntimeShell<R>
where
    R: VoidDnsResolver + Send + Sync + 'static,
{
    pub fn load_or_create(
        data_dir: impl AsRef<Path>,
        runtime: VoidRuntime<R>,
    ) -> Result<Self, RuntimeError> {
        let runtime_dir = data_dir.as_ref().join("runtime");
        fs::create_dir_all(&runtime_dir)?;
        let state_path = runtime_dir.join("shell.json");
        let mut shell = Self {
            runtime,
            data_dir: data_dir.as_ref().to_path_buf(),
            state_path,
            state: load_json_or_default(&runtime_dir.join("shell.json"))?,
        };
        shell.ensure_builtin_registry();
        shell.persist()?;
        Ok(shell)
    }

    pub fn state(&self) -> &RuntimeShellState {
        &self.state
    }

    pub fn reconcile_registry_owner(&mut self, owner_peer_id: &str) -> Result<(), RuntimeError> {
        let mut changed = false;
        for entry in &mut self.state.registry {
            if entry.owner_peer_id != owner_peer_id {
                entry.owner_peer_id = owner_peer_id.to_string();
                changed = true;
            }
        }
        if changed {
            self.state.updated_unix_ms = unix_millis();
            self.persist()?;
        }
        Ok(())
    }

    pub fn persist(&self) -> Result<(), RuntimeError> {
        persist_json(&self.state_path, &self.state)
    }

    pub fn render_surface_once(
        &mut self,
        route: &str,
        input_state: BTreeMap<String, String>,
    ) -> Result<(TerminalRenderedSurface, Vec<TransportEvent>), RuntimeError> {
        let result = self.render_surface(route, input_state, "manual-refresh", true)?;
        Ok((
            result
                .rendered_surface
                .ok_or_else(|| RuntimeError::SurfaceNotRendered(route.to_string()))?,
            result.events,
        ))
    }

    pub fn synchronize_surface(
        &mut self,
        route: &str,
    ) -> Result<Option<RuntimeSurfaceSyncResult>, RuntimeError> {
        let input_state = self
            .state
            .ui_surfaces
            .iter()
            .find(|surface| surface.route == route)
            .map(|surface| surface.input_state.clone())
            .unwrap_or_default();
        let result = self.render_surface(route, input_state, "runtime-sync", false)?;
        if result.changed {
            Ok(Some(result))
        } else {
            Ok(None)
        }
    }

    pub fn surface_view(&self, route: &str) -> Result<RuntimeSurfaceView, RuntimeError> {
        let registration = self
            .find_registration_by_route(route)
            .cloned()
            .ok_or_else(|| RuntimeError::SurfaceRegistrationMissing(route.to_string()))?;
        let frame = self.resolve_surface_frame(&registration)?;
        let ui_surface = self
            .state
            .ui_surfaces
            .iter()
            .find(|surface| surface.route == route);
        let mount = self.state.mounts.iter().find(|mount| mount.route == route);

        Ok(RuntimeSurfaceView {
            route: route.to_string(),
            surface_id: registration.surface_id.clone(),
            surface_kind: registration.surface_kind,
            tree: frame.tree,
            bindings: frame.bindings,
            input_state: ui_surface
                .map(|surface| surface.input_state.clone())
                .unwrap_or_default(),
            last_error: ui_surface.and_then(|surface| surface.last_error.clone()),
            session_id: mount.map(|mount| mount.session_id.clone()),
            owner_peer: mount.map(|mount| mount.owner_peer.clone()),
            last_updated_unix_ms: ui_surface
                .map(|surface| surface.last_updated_unix_ms)
                .unwrap_or_else(unix_millis),
        })
    }

    pub fn dispatch_surface_action(
        &mut self,
        route: &str,
        request: RuntimeActionRequest,
    ) -> Result<RuntimeActionResult, RuntimeError> {
        let registration = self
            .find_registration_by_route(route)
            .cloned()
            .ok_or_else(|| RuntimeError::SurfaceRegistrationMissing(route.to_string()))?;
        let mut events = vec![
            TransportEvent::ActionDispatched {
                route: route.to_string(),
                surface_id: registration.surface_id.clone(),
                action: request.action.clone(),
            },
            TransportEvent::SurfaceActionTriggered {
                route: route.to_string(),
                surface_id: registration.surface_id.clone(),
                action: request.action.clone(),
            },
        ];

        let summary = match self.execute_surface_action(&registration.surface_id, &request) {
            Ok(summary) => {
                if let Some(surface) = self
                    .state
                    .ui_surfaces
                    .iter_mut()
                    .find(|surface| surface.route == route)
                {
                    surface.last_action = Some(request.action.clone());
                    surface.last_handler = Some(request.action.clone());
                    surface.last_error = None;
                    surface.input_state = request.input_state.clone();
                    surface.local_state = prefixed_input_state(&request.input_state);
                    surface.last_updated_unix_ms = unix_millis();
                }
                summary
            }
            Err(RuntimeError::PermissionDenied {
                missing_capabilities,
                ..
            }) => {
                let capability = missing_capabilities
                    .first()
                    .cloned()
                    .unwrap_or_else(|| "unknown".to_string());
                events.push(TransportEvent::SurfacePermissionDenied {
                    route: route.to_string(),
                    surface_id: registration.surface_id.clone(),
                    action: Some(request.action.clone()),
                    capability,
                    reason: "permission not granted in runtime shell".to_string(),
                });
                self.record_surface_action_error(
                    route,
                    &registration.surface_id,
                    &request,
                    &events,
                )?;
                format!("permission denied for {}", request.action)
            }
            Err(error) => {
                let error_text = error.to_string();
                events.push(TransportEvent::SurfaceRenderFailed {
                    route: route.to_string(),
                    surface_id: registration.surface_id.clone(),
                    error: format!("handler {} failed: {error_text}", request.action),
                });
                self.record_surface_action_error(
                    route,
                    &registration.surface_id,
                    &request,
                    &events,
                )?;
                format!("action failed: {error_text}")
            }
        };

        self.state.updated_unix_ms = unix_millis();
        self.persist()?;
        Ok(RuntimeActionResult { summary, events })
    }

    fn execute_surface_action(
        &self,
        surface_id: &str,
        request: &RuntimeActionRequest,
    ) -> Result<String, RuntimeError> {
        match request.action.as_str() {
            "chat.send" => {
                self.require_capability(surface_id, "surface/messaging")?;
                self.require_capability(surface_id, "service/chat")?;
                let peer_id = request
                    .input_state
                    .get("peer_id")
                    .cloned()
                    .filter(|value| !value.trim().is_empty());
                let message = request
                    .input_state
                    .get("message")
                    .cloned()
                    .filter(|value| !value.trim().is_empty())
                    .ok_or_else(|| RuntimeError::InputMissing("message".to_string()))?;
                let path = if let Some(peer_id) = peer_id {
                    self.require_capability(surface_id, "chat/direct-e2ee")?;
                    enqueue_local_command(
                        &self.data_dir,
                        ChatLocalCommand::SendDirect { peer_id, message },
                    )?
                } else {
                    self.require_capability(surface_id, "surface/room-access")?;
                    let rooms = load_chat_rooms(&self.data_dir)?;
                    let room = request
                        .input_state
                        .get("room")
                        .cloned()
                        .filter(|value| !value.trim().is_empty())
                        .or_else(|| rooms.current_room.clone())
                        .ok_or_else(|| RuntimeError::InputMissing("room or peer_id".to_string()))?;
                    enqueue_local_command(
                        &self.data_dir,
                        ChatLocalCommand::SendRoom { room, message },
                    )?
                };
                Ok(format!("queued chat.send via {}", path.display()))
            }
            "chat.join" => {
                self.require_capability(surface_id, "surface/room-access")?;
                self.require_capability(surface_id, "service/chat")?;
                let room = request
                    .input_state
                    .get("room")
                    .cloned()
                    .filter(|value| !value.trim().is_empty())
                    .ok_or_else(|| RuntimeError::InputMissing("room".to_string()))?;
                let path = enqueue_local_command(&self.data_dir, ChatLocalCommand::Join { room })?;
                Ok(format!("queued chat.join via {}", path.display()))
            }
            "chat.leave" => {
                self.require_capability(surface_id, "surface/room-access")?;
                let rooms = load_chat_rooms(&self.data_dir)?;
                let room = request
                    .input_state
                    .get("room")
                    .cloned()
                    .filter(|value| !value.trim().is_empty())
                    .or_else(|| rooms.current_room.clone())
                    .ok_or_else(|| RuntimeError::InputMissing("room".to_string()))?;
                let path = enqueue_local_command(&self.data_dir, ChatLocalCommand::Leave { room })?;
                Ok(format!("queued chat.leave via {}", path.display()))
            }
            "chat.switch_room" => {
                self.require_capability(surface_id, "surface/room-access")?;
                let room = request
                    .input_state
                    .get("room")
                    .cloned()
                    .filter(|value| !value.trim().is_empty())
                    .ok_or_else(|| RuntimeError::InputMissing("room".to_string()))?;
                let path =
                    enqueue_local_command(&self.data_dir, ChatLocalCommand::SwitchRoom { room })?;
                Ok(format!("queued chat.switch_room via {}", path.display()))
            }
            "chat.mark_read" => {
                self.require_capability(surface_id, "surface/notifications")?;
                let room = request
                    .input_state
                    .get("room")
                    .cloned()
                    .filter(|value| !value.trim().is_empty());
                let path =
                    enqueue_local_command(&self.data_dir, ChatLocalCommand::MarkRead { room })?;
                Ok(format!("queued chat.mark_read via {}", path.display()))
            }
            "surface.refresh" => Ok("surface refreshed".to_string()),
            other => Err(RuntimeError::UnknownAction(other.to_string())),
        }
    }

    fn record_surface_action_error(
        &mut self,
        route: &str,
        surface_id: &str,
        request: &RuntimeActionRequest,
        events: &[TransportEvent],
    ) -> Result<(), RuntimeError> {
        let error_text = events.iter().rev().find_map(|event| match event {
            TransportEvent::SurfacePermissionDenied { reason, .. } => Some(reason.clone()),
            TransportEvent::SurfaceRenderFailed { error, .. } => Some(error.clone()),
            _ => None,
        });
        if let Some(surface) = self
            .state
            .ui_surfaces
            .iter_mut()
            .find(|surface| surface.route == route)
        {
            surface.last_handler = Some(request.action.clone());
            surface.last_error = error_text.clone();
            surface.input_state = request.input_state.clone();
            surface.local_state = prefixed_input_state(&request.input_state);
            surface.permission_denials = surface.permission_denials.saturating_add(1);
            surface.last_updated_unix_ms = unix_millis();
        } else {
            self.upsert_ui_surface(RuntimeUiSurfaceState {
                route: route.to_string(),
                surface_id: surface_id.to_string(),
                node_count: 0,
                input_state: request.input_state.clone(),
                local_state: prefixed_input_state(&request.input_state),
                runtime_state: BTreeMap::new(),
                distributed_state: BTreeMap::new(),
                source_snapshot: RuntimeSurfaceSourceSnapshot::default(),
                state_revision: 0,
                rerender_count: 0,
                hot_reload_count: 0,
                sync_count: 0,
                permission_denials: 1,
                last_changed_bindings: Vec::new(),
                last_action: None,
                last_handler: Some(request.action.clone()),
                last_error: error_text,
                last_render_duration_ms: 0,
                last_source_modified_unix_ms: None,
                last_updated_unix_ms: unix_millis(),
            });
        }
        self.state.updated_unix_ms = unix_millis();
        self.persist()
    }

    fn render_surface(
        &mut self,
        route: &str,
        input_state: BTreeMap<String, String>,
        source: &str,
        force: bool,
    ) -> Result<RuntimeSurfaceSyncResult, RuntimeError> {
        let registration = self
            .find_registration_by_route(route)
            .cloned()
            .ok_or_else(|| RuntimeError::SurfaceRegistrationMissing(route.to_string()))?;
        let previous = self
            .state
            .ui_surfaces
            .iter()
            .find(|surface| surface.route == route)
            .cloned();

        match self.resolve_surface_frame(&registration) {
            Ok(frame) => {
                let local_state = prefixed_input_state(&input_state);
                let runtime_state =
                    classify_runtime_state(&registration.surface_id, &frame.bindings);
                let distributed_state =
                    classify_distributed_state(&registration.surface_id, &frame.bindings);
                let merged_state =
                    merge_surface_state_maps(&local_state, &runtime_state, &distributed_state);
                let mut source_snapshot = frame.source_snapshot.clone();
                source_snapshot.state_hash = stable_hash(&merged_state)?;

                let previous_state = previous
                    .as_ref()
                    .map(stored_surface_state)
                    .unwrap_or_default();
                let changed_bindings = diff_state_keys(&previous_state, &merged_state);
                let state_changed = previous
                    .as_ref()
                    .map(|surface| surface.source_snapshot.state_hash != source_snapshot.state_hash)
                    .unwrap_or(true);
                let hot_reloaded = previous
                    .as_ref()
                    .map(|surface| {
                        surface.source_snapshot.surface_hash != source_snapshot.surface_hash
                    })
                    .unwrap_or(false);
                let changed = force || previous.is_none() || state_changed || hot_reloaded;

                if !changed {
                    return Ok(RuntimeSurfaceSyncResult {
                        rendered_surface: None,
                        events: Vec::new(),
                        changed: false,
                    });
                }

                let rendered =
                    match render_terminal_surface(&frame.tree, &frame.bindings, &input_state) {
                        Ok(rendered) => rendered,
                        Err(error) => {
                            return self.render_surface_error_boundary(
                                route,
                                &registration,
                                previous.as_ref(),
                                input_state,
                                format!("render failed: {error}"),
                            );
                        }
                    };
                let rerender_count = previous
                    .as_ref()
                    .map(|surface| surface.rerender_count.saturating_add(1))
                    .unwrap_or(1);
                let hot_reload_count = previous
                    .as_ref()
                    .map(|surface| {
                        surface
                            .hot_reload_count
                            .saturating_add(u64::from(hot_reloaded))
                    })
                    .unwrap_or(u64::from(hot_reloaded));
                let sync_count = previous
                    .as_ref()
                    .map(|surface| {
                        surface
                            .sync_count
                            .saturating_add(u64::from(source == "runtime-sync"))
                    })
                    .unwrap_or(u64::from(source == "runtime-sync"));
                let permission_denials = previous
                    .as_ref()
                    .map(|surface| surface.permission_denials)
                    .unwrap_or_default();
                let state_revision = previous
                    .as_ref()
                    .map(|surface| {
                        surface
                            .state_revision
                            .saturating_add(u64::from(state_changed || hot_reloaded))
                    })
                    .unwrap_or(1);
                let affected_nodes = count_bound_nodes(&frame.tree, &changed_bindings);

                let mut events = vec![TransportEvent::SurfaceParsed {
                    route: route.to_string(),
                    surface_id: registration.surface_id.clone(),
                }];
                if previous.is_none() {
                    events.push(TransportEvent::SurfaceLoaded {
                        route: route.to_string(),
                        surface_id: registration.surface_id.clone(),
                        state_revision,
                    });
                }
                if hot_reloaded {
                    events.push(TransportEvent::SurfaceHotReloaded {
                        route: route.to_string(),
                        surface_id: registration.surface_id.clone(),
                        generation: hot_reload_count,
                        preserved_session: true,
                    });
                }
                if state_changed {
                    events.push(TransportEvent::SurfaceUpdated {
                        route: route.to_string(),
                        surface_id: registration.surface_id.clone(),
                        changed_bindings: changed_bindings.clone(),
                        source: source.to_string(),
                    });
                    events.push(TransportEvent::SurfaceStateChanged {
                        route: route.to_string(),
                        surface_id: registration.surface_id.clone(),
                        state_revision,
                        changed_bindings: changed_bindings.clone(),
                        distributed: changed_bindings.iter().any(|binding| {
                            is_distributed_state_key(&registration.surface_id, binding)
                        }),
                    });
                }
                events.push(TransportEvent::RuntimeTreeBuilt {
                    route: route.to_string(),
                    surface_id: registration.surface_id.clone(),
                    nodes: frame.tree.node_count,
                });
                events.push(TransportEvent::SurfaceRendered {
                    route: route.to_string(),
                    surface_id: registration.surface_id.clone(),
                    nodes: frame.tree.node_count,
                    render_ms: rendered.render_duration_ms,
                });
                events.push(TransportEvent::SurfaceRenderCompleted {
                    route: route.to_string(),
                    surface_id: registration.surface_id.clone(),
                    nodes: frame.tree.node_count,
                    affected_nodes,
                    rerender_count,
                    render_ms: rendered.render_duration_ms,
                });

                self.upsert_ui_surface(RuntimeUiSurfaceState {
                    route: route.to_string(),
                    surface_id: registration.surface_id.clone(),
                    node_count: frame.tree.node_count,
                    input_state,
                    local_state,
                    runtime_state,
                    distributed_state,
                    source_snapshot: source_snapshot.clone(),
                    state_revision,
                    rerender_count,
                    hot_reload_count,
                    sync_count,
                    permission_denials,
                    last_changed_bindings: changed_bindings,
                    last_action: previous
                        .as_ref()
                        .and_then(|surface| surface.last_action.clone()),
                    last_handler: previous
                        .as_ref()
                        .and_then(|surface| surface.last_handler.clone()),
                    last_error: None,
                    last_render_duration_ms: rendered.render_duration_ms,
                    last_source_modified_unix_ms: source_snapshot.source_modified_unix_ms,
                    last_updated_unix_ms: unix_millis(),
                });
                self.state.updated_unix_ms = unix_millis();
                self.persist()?;

                Ok(RuntimeSurfaceSyncResult {
                    rendered_surface: Some(rendered),
                    events,
                    changed: true,
                })
            }
            Err(error) => self.render_surface_error_boundary(
                route,
                &registration,
                previous.as_ref(),
                input_state,
                error.to_string(),
            ),
        }
    }

    fn render_surface_error_boundary(
        &mut self,
        route: &str,
        registration: &RuntimeSurfaceRegistration,
        previous: Option<&RuntimeUiSurfaceState>,
        input_state: BTreeMap<String, String>,
        error: String,
    ) -> Result<RuntimeSurfaceSyncResult, RuntimeError> {
        let changed = previous
            .map(|surface| surface.last_error.as_deref() != Some(error.as_str()))
            .unwrap_or(true);
        if !changed {
            return Ok(RuntimeSurfaceSyncResult {
                rendered_surface: None,
                events: Vec::new(),
                changed: false,
            });
        }

        let rendered = fallback_surface_render(&registration.surface_id, &error);
        self.upsert_ui_surface(RuntimeUiSurfaceState {
            route: route.to_string(),
            surface_id: registration.surface_id.clone(),
            node_count: 0,
            input_state: input_state.clone(),
            local_state: prefixed_input_state(&input_state),
            runtime_state: BTreeMap::new(),
            distributed_state: BTreeMap::new(),
            source_snapshot: previous
                .map(|surface| surface.source_snapshot.clone())
                .unwrap_or_default(),
            state_revision: previous
                .map(|surface| surface.state_revision.saturating_add(1))
                .unwrap_or(1),
            rerender_count: previous
                .map(|surface| surface.rerender_count.saturating_add(1))
                .unwrap_or(1),
            hot_reload_count: previous
                .map(|surface| surface.hot_reload_count)
                .unwrap_or_default(),
            sync_count: previous
                .map(|surface| surface.sync_count)
                .unwrap_or_default(),
            permission_denials: previous
                .map(|surface| surface.permission_denials)
                .unwrap_or_default(),
            last_changed_bindings: Vec::new(),
            last_action: previous.and_then(|surface| surface.last_action.clone()),
            last_handler: previous.and_then(|surface| surface.last_handler.clone()),
            last_error: Some(error.clone()),
            last_render_duration_ms: rendered.render_duration_ms,
            last_source_modified_unix_ms: previous
                .and_then(|surface| surface.last_source_modified_unix_ms),
            last_updated_unix_ms: unix_millis(),
        });
        self.state.updated_unix_ms = unix_millis();
        self.persist()?;
        Ok(RuntimeSurfaceSyncResult {
            rendered_surface: Some(rendered),
            events: vec![TransportEvent::SurfaceRenderFailed {
                route: route.to_string(),
                surface_id: registration.surface_id.clone(),
                error,
            }],
            changed: true,
        })
    }

    fn resolve_surface_frame(
        &self,
        registration: &RuntimeSurfaceRegistration,
    ) -> Result<ResolvedSurfaceFrame, RuntimeError> {
        let surface_path = registration
            .surface_path
            .clone()
            .ok_or_else(|| RuntimeError::SurfaceDocumentMissing(registration.surface_id.clone()))?;
        let source = fs::read_to_string(&surface_path)?;
        let document = parse_surface_document(&source)?;
        let tree = build_runtime_tree(&document)?;
        let bindings = self.build_bindings(&registration.surface_id)?;
        Ok(ResolvedSurfaceFrame {
            tree,
            bindings: bindings.clone(),
            source_snapshot: RuntimeSurfaceSourceSnapshot {
                surface_hash: stable_hash(&source)?,
                binding_hash: stable_hash(&bindings)?,
                state_hash: String::new(),
                surface_modified_unix_ms: file_modified_unix_ms(Path::new(&surface_path)),
                source_modified_unix_ms: self
                    .binding_source_modified_unix_ms(&registration.surface_id),
            },
        })
    }

    fn binding_source_modified_unix_ms(&self, surface_id: &str) -> Option<u128> {
        match surface_id {
            "chat" => [
                self.data_dir.join("chat").join("inbox.json"),
                self.data_dir.join("chat").join("notifications.json"),
                self.data_dir.join("chat").join("rooms.json"),
                self.data_dir.join("chat").join("sessions.json"),
                self.data_dir.join("topology.json"),
            ]
            .into_iter()
            .filter_map(|path| file_modified_unix_ms(&path))
            .max(),
            _ => None,
        }
    }

    pub fn grant_permission(
        &mut self,
        surface_id: &str,
        peer_owner: &str,
        capability: &str,
        allowed: bool,
    ) -> Result<(), RuntimeError> {
        let now = unix_millis();
        if let Some(existing) = self.state.permissions.iter_mut().find(|entry| {
            entry.surface_id == surface_id
                && entry.peer_owner == peer_owner
                && entry.capability == capability
        }) {
            existing.allowed = allowed;
            existing.persisted = true;
            existing.decided_at_unix_ms = now;
        } else {
            self.state.permissions.push(RuntimePermissionGrant {
                surface_id: surface_id.to_string(),
                peer_owner: peer_owner.to_string(),
                capability: capability.to_string(),
                allowed,
                persisted: true,
                requested_at_unix_ms: now,
                decided_at_unix_ms: now,
            });
        }
        self.state.updated_unix_ms = now;
        self.persist()
    }

    pub async fn open_uri(&mut self, uri: VoidUri) -> Result<OpenSurfaceResult, RuntimeError> {
        let mut events = vec![TransportEvent::SurfaceResolving {
            route: uri.to_string(),
        }];
        let route = self
            .runtime
            .dns
            .resolve_route(&uri)
            .await?
            .ok_or_else(|| RuntimeError::NameNotFound(uri.authority().to_string()))?;
        let registration = self.ensure_route_registration(&route)?;
        events.push(TransportEvent::SurfaceResolved {
            route: uri.to_string(),
            peer_owner: route.target_peer_id.clone(),
            surface_id: route.runtime_surface.clone(),
            latency_ms: route.resolution_latency_ms,
        });

        if registration.surface_kind == RuntimeSurfaceKind::Gateway {
            if let Err(error) = self.validate_gateway_trust(&registration, &mut events) {
                self.record_gateway_failure(
                    &uri,
                    &route,
                    &registration,
                    &format!("rt-gateway-failed-{}", unix_millis()),
                    &error.to_string(),
                )?;
                return Err(error);
            }
        }

        let now = unix_millis();
        let surface_id = registration.surface_id.clone();
        let (granted_permissions, rejected_capabilities) = self.negotiate_capabilities(
            &surface_id,
            &route.target_peer_id,
            &route.capabilities,
            &mut events,
            now,
        );

        if !rejected_capabilities.is_empty() {
            self.record_failed_mount(
                &uri,
                &route,
                &surface_id,
                &granted_permissions,
                &rejected_capabilities,
            )?;
            return Err(RuntimeError::PermissionDenied {
                surface_id,
                missing_capabilities: rejected_capabilities,
            });
        }

        events.push(TransportEvent::SurfaceMounting {
            route: uri.to_string(),
            peer_owner: route.target_peer_id.clone(),
            surface_id: route.runtime_surface.clone(),
        });

        let session_id = format!("rt-{}", unix_millis());
        let mut gateway_render_details = None;
        if registration.surface_kind == RuntimeSurfaceKind::Gateway {
            match self
                .execute_gateway_fetch(&registration, &route, &session_id, &mut events)
                .await
            {
                Ok((bridge, bridge_session)) => {
                    gateway_render_details = Some((
                        bridge.response_size.unwrap_or_default(),
                        bridge
                            .render_mode
                            .clone()
                            .unwrap_or_else(|| "text".to_string()),
                    ));
                    events.push(TransportEvent::GatewayMounted {
                        route: uri.to_string(),
                        gateway_id: registration.surface_id.clone(),
                        external_target: bridge.external_target.clone(),
                    });
                    events.push(TransportEvent::GatewayBridgePrepared {
                        route: uri.to_string(),
                        gateway_id: registration.surface_id.clone(),
                        protocol_stack: bridge.supported_protocols.clone(),
                        external_target: bridge.external_target.clone(),
                    });
                    self.upsert_gateway_bridge_session(bridge_session);
                    self.upsert_gateway_route(GatewayRouteState {
                        route: uri.to_string(),
                        gateway_domain: registration.domain.clone(),
                        gateway_id: registration.surface_id.clone(),
                        trust_state: self.gateway_trust_state(&registration.domain),
                        session_id: session_id.clone(),
                        active: true,
                        bridge,
                        last_error: None,
                        updated_unix_ms: unix_millis(),
                    });
                }
                Err(error) => {
                    self.record_gateway_failure(
                        &uri,
                        &route,
                        &registration,
                        &session_id,
                        &error.to_string(),
                    )?;
                    events.push(TransportEvent::GatewayBridgeFailed {
                        route: uri.to_string(),
                        gateway_id: registration.surface_id.clone(),
                        error: error.to_string(),
                    });
                    return Err(error);
                }
            }
        }

        let session = RuntimeSessionRecord {
            session_id: session_id.clone(),
            mounted_surface: route.runtime_surface.clone(),
            peer_owner: route.target_peer_id.clone(),
            active_capabilities: granted_permissions
                .iter()
                .filter(|grant| grant.allowed)
                .map(|grant| grant.capability.clone())
                .collect(),
            encryption_state: "ROUTE-RESOLVED".to_string(),
            started_at_unix_ms: now,
            last_activity_unix_ms: now,
            session_state: RuntimeSessionState::Active,
        };
        let mount = MountedRuntimeSurface {
            route: uri.to_string(),
            surface_id: registration.surface_id.clone(),
            owner_peer: route.target_peer_id.clone(),
            capabilities: session.active_capabilities.clone(),
            surface_kind: registration.surface_kind,
            runtime_state: RuntimeLifecycleState::Active,
            mount_state: MountState::Mounted,
            session_state: RuntimeSessionState::Active,
            mounted_at_unix_ms: now,
            last_activity_unix_ms: now,
            session_id: session_id.clone(),
            mount_latency_ms: route.resolution_latency_ms,
            failure_reason: None,
        };

        self.upsert_session(session.clone());
        self.upsert_mount(mount.clone());
        self.state.updated_unix_ms = now;
        events.push(TransportEvent::RuntimeSessionStarted {
            route: uri.to_string(),
            session_id: session_id.clone(),
            peer_owner: route.target_peer_id.clone(),
            surface_id: registration.surface_id.clone(),
        });
        events.push(TransportEvent::SurfaceMounted {
            route: uri.to_string(),
            surface_id: registration.surface_id.clone(),
            session_id,
            latency_ms: route.resolution_latency_ms,
        });
        self.persist()?;

        let (rendered_surface, render_events) =
            self.render_surface_once(&uri.to_string(), BTreeMap::new())?;
        events.extend(render_events);
        if let Some((bytes, render_mode)) = gateway_render_details {
            events.push(TransportEvent::ResponseSurfaceRendered {
                route: uri.to_string(),
                gateway_id: registration.surface_id.clone(),
                bytes,
                render_mode,
            });
        }

        Ok(OpenSurfaceResult {
            route,
            mount,
            session,
            granted_permissions,
            rejected_capabilities,
            rendered_surface,
            events,
        })
    }

    fn ensure_builtin_registry(&mut self) {
        if self
            .state
            .registry
            .iter()
            .any(|entry| entry.domain == "chat.void")
        {
        } else {
            let peer_owner = self.runtime.identity.peer_id().to_string();
            self.state.registry.push(RuntimeSurfaceRegistration {
                domain: "chat.void".to_string(),
                surface_id: "chat".to_string(),
                runtime_surface: "chat".to_string(),
                entry_uri: VoidUri::new("chat.void", "/", None).expect("valid builtin runtime uri"),
                owner_peer_id: peer_owner,
                capabilities: vec![
                    "chat/direct-e2ee".to_string(),
                    "dns/addressable".to_string(),
                    "routing/void-uri".to_string(),
                    "service/chat".to_string(),
                    "surface/messaging".to_string(),
                    "surface/room-access".to_string(),
                    "surface/notifications".to_string(),
                    "runtime/session-access".to_string(),
                ],
                handler: "void-chat".to_string(),
                surface_kind: RuntimeSurfaceKind::Application,
                supported_protocols: vec!["void".to_string()],
                external_route_base: None,
                trust_level: None,
                surface_path: Some(
                    self.ensure_builtin_surface_file("chat.surface", CHAT_SURFACE_SOURCE),
                ),
            });
        }
        if !self
            .state
            .registry
            .iter()
            .any(|entry| entry.domain == "local.gateway.void")
        {
            let peer_owner = self.runtime.identity.peer_id().to_string();
            let domain = "local.gateway.void";
            self.state.registry.push(RuntimeSurfaceRegistration {
                domain: domain.to_string(),
                surface_id: gateway_surface_id(domain),
                runtime_surface: "gateway".to_string(),
                entry_uri: VoidUri::new(domain, "/", None).expect("valid builtin gateway uri"),
                owner_peer_id: peer_owner.clone(),
                capabilities: gateway_capabilities(),
                handler: "void-gateway".to_string(),
                surface_kind: RuntimeSurfaceKind::Gateway,
                supported_protocols: vec!["http".to_string(), "https".to_string()],
                external_route_base: Some(default_gateway_external_base(domain)),
                trust_level: Some(GatewayTrustLevel::Trusted),
                surface_path: Some(
                    self.ensure_builtin_surface_file("gateway.surface", GATEWAY_SURFACE_SOURCE),
                ),
            });
            self.upsert_gateway_trust(GatewayTrustPolicy {
                gateway_domain: domain.to_string(),
                gateway_id: gateway_surface_id(domain),
                owner_peer: peer_owner,
                trust_level: GatewayTrustLevel::Trusted,
                trust_state: GatewayTrustState::Trusted,
                capability_scope: gateway_capabilities(),
                runtime_restrictions: vec!["external-fetch-foundation-only".to_string()],
                permission_history: gateway_capabilities()
                    .into_iter()
                    .map(|capability| GatewayPermissionDecision {
                        capability,
                        allowed: true,
                        reason: "builtin local gateway".to_string(),
                        decided_at_unix_ms: unix_millis(),
                    })
                    .collect(),
                last_warning: None,
                updated_unix_ms: unix_millis(),
            });
        }
        self.state.updated_unix_ms = unix_millis();
    }

    fn ensure_builtin_surface_file(&self, file_name: &str, source: &str) -> String {
        let surfaces_dir = self.data_dir.join("runtime").join("surfaces");
        let path = surfaces_dir.join(file_name);
        if !path.exists() {
            let _ = fs::create_dir_all(&surfaces_dir);
            let _ = fs::write(&path, source);
        }
        path.display().to_string()
    }

    fn build_bindings(&self, surface_id: &str) -> Result<BTreeMap<String, String>, RuntimeError> {
        match surface_id {
            "chat" => self.build_chat_bindings(surface_id),
            _ if surface_id.starts_with("gateway:") => self.build_gateway_bindings(surface_id),
            _ => Ok(BTreeMap::new()),
        }
    }

    fn build_gateway_bindings(
        &self,
        surface_id: &str,
    ) -> Result<BTreeMap<String, String>, RuntimeError> {
        let registration = self
            .state
            .registry
            .iter()
            .find(|entry| {
                entry.surface_id == surface_id && entry.surface_kind == RuntimeSurfaceKind::Gateway
            })
            .ok_or_else(|| RuntimeError::SurfaceRegistrationMissing(surface_id.to_string()))?;
        let trust = self
            .state
            .gateway_trust
            .iter()
            .find(|policy| policy.gateway_domain == registration.domain);
        let active_routes = self
            .state
            .gateway_routes
            .iter()
            .filter(|route| route.gateway_domain == registration.domain && route.active)
            .collect::<Vec<_>>();
        let last_route = self
            .state
            .gateway_routes
            .iter()
            .filter(|route| route.gateway_domain == registration.domain)
            .max_by_key(|route| route.updated_unix_ms);
        let bridge_sessions = self
            .state
            .gateway_bridge_sessions
            .iter()
            .filter(|session| session.gateway_domain == registration.domain)
            .collect::<Vec<_>>();

        let mut bindings = BTreeMap::new();
        bindings.insert(
            "gateway.gateway_id".to_string(),
            registration.surface_id.clone(),
        );
        bindings.insert(
            "gateway.owner_peer".to_string(),
            registration.owner_peer_id.clone(),
        );
        bindings.insert(
            "gateway.supported_protocols".to_string(),
            if registration.supported_protocols.is_empty() {
                "protocols=unknown".to_string()
            } else {
                registration.supported_protocols.join(",")
            },
        );
        bindings.insert(
            "gateway.trust_state".to_string(),
            format!(
                "trust_state={:?} trust_level={:?} warning={}",
                trust
                    .map(|policy| policy.trust_state)
                    .unwrap_or(GatewayTrustState::Pending),
                trust.map(|policy| policy.trust_level).unwrap_or_default(),
                trust
                    .and_then(|policy| policy.last_warning.clone())
                    .unwrap_or_else(|| "-".to_string())
            ),
        );
        bindings.insert(
            "gateway.active_routes".to_string(),
            if active_routes.is_empty() {
                "no active external routes".to_string()
            } else {
                active_routes
                    .iter()
                    .map(|route| {
                        format!(
                            "{} -> {} {:?}",
                            route.route, route.bridge.external_target, route.bridge.lifecycle_state
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            },
        );
        bindings.insert(
            "gateway.runtime_capabilities".to_string(),
            registration.capabilities.join(","),
        );
        bindings.insert(
            "gateway.external_target".to_string(),
            last_route
                .map(|route| format!("external_target={}", route.bridge.external_target))
                .unwrap_or_else(|| {
                    format!(
                        "external_target={}",
                        registration
                            .external_route_base
                            .clone()
                            .unwrap_or_else(|| "unresolved".to_string())
                    )
                }),
        );
        bindings.insert(
            "gateway.bridge_state".to_string(),
            last_route
                .map(|route| {
                    format!(
                        "bridge_state={:?} stream_chunks={} fetch_latency={}ms response_size={} cache_state={} render_mode={} last_error={}",
                        route.bridge.lifecycle_state,
                        route.bridge.stream_chunks.len(),
                        route.bridge.fetch_latency_ms.unwrap_or_default(),
                        route.bridge.response_size.unwrap_or_default(),
                        route.bridge.cache_state.as_deref().unwrap_or("-"),
                        route.bridge.render_mode.as_deref().unwrap_or("-"),
                        route.last_error.clone().unwrap_or_else(|| "-".to_string())
                    )
                })
                .unwrap_or_else(|| "bridge_state=prepared stream_chunks=0 last_error=-".to_string()),
        );
        bindings.insert(
            "gateway.request".to_string(),
            last_route
                .map(|route| {
                    format!(
                        "method={} url={} timeout={}ms request_id={} query={}",
                        route.bridge.request.method,
                        route.bridge.request.url,
                        route.bridge.request.timeout_ms,
                        route.bridge.request.request_id,
                        if route.bridge.request.query.is_empty() {
                            "-".to_string()
                        } else {
                            route
                                .bridge
                                .request
                                .query
                                .iter()
                                .map(|(key, value)| format!("{key}={value}"))
                                .collect::<Vec<_>>()
                                .join("&")
                        }
                    )
                })
                .unwrap_or_else(|| {
                    "method=GET url=unresolved timeout=0ms request_id=- query=-".to_string()
                }),
        );
        bindings.insert(
            "gateway.response_status".to_string(),
            last_route
                .and_then(|route| route.bridge.response.as_ref())
                .map(|response| {
                    format!(
                        "status={} content_type={} bytes={} fetched_at={} response_id={} gateway_peer={}",
                        response.status,
                        response.content_type.as_deref().unwrap_or("application/octet-stream"),
                        response.response_size,
                        response.fetched_at_unix_ms.unwrap_or_default(),
                        response.response_id,
                        response.gateway_peer,
                    )
                })
                .unwrap_or_else(|| "status=unavailable content_type=- bytes=0 fetched_at=0 response_id=- gateway_peer=-".to_string()),
        );
        bindings.insert(
            "gateway.response_headers".to_string(),
            last_route
                .and_then(|route| route.bridge.response.as_ref())
                .map(|response| format_header_bindings(&response.headers))
                .unwrap_or_else(|| "headers=empty".to_string()),
        );
        bindings.insert(
            "gateway.response_preview".to_string(),
            last_route
                .and_then(|route| route.bridge.response.as_ref())
                .map(render_gateway_response_preview)
                .unwrap_or_else(|| "response preview unavailable".to_string()),
        );
        bindings.insert(
            "gateway.response_cache".to_string(),
            last_route
                .map(|route| {
                    format!(
                        "cache_state={} snapshot_id={} cacheable={}",
                        route.bridge.cache_state.as_deref().unwrap_or("-"),
                        route.bridge.snapshot_id.as_deref().unwrap_or("-"),
                        route
                            .bridge
                            .response
                            .as_ref()
                            .map(|response| response.cacheable)
                            .unwrap_or(false)
                    )
                })
                .unwrap_or_else(|| "cache_state=- snapshot_id=- cacheable=false".to_string()),
        );
        bindings.insert(
            "gateway.bridge_sessions".to_string(),
            if bridge_sessions.is_empty() {
                "no bridge sessions".to_string()
            } else {
                bridge_sessions
                    .iter()
                    .rev()
                    .take(8)
                    .map(|session| {
                        format!(
                            "{} fetch={} response={} mount={:?} bytes={} latency={}ms target={}",
                            session.session_id,
                            session.fetch_state,
                            session.response_state,
                            session.mount_state,
                            session.response_size.unwrap_or_default(),
                            session.fetch_latency_ms.unwrap_or_default(),
                            session.external_target,
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            },
        );
        bindings.insert(
            "gateway.snapshot_state".to_string(),
            last_route
                .map(|route| {
                    format!(
                        "snapshot_id={} cache_state={} external_target={}",
                        route.bridge.snapshot_id.as_deref().unwrap_or("-"),
                        route.bridge.cache_state.as_deref().unwrap_or("-"),
                        route.bridge.external_target,
                    )
                })
                .unwrap_or_else(|| {
                    "snapshot_id=- cache_state=- external_target=unresolved".to_string()
                }),
        );
        bindings.insert(
            "gateway.permission_history".to_string(),
            trust
                .map(|policy| {
                    if policy.permission_history.is_empty() {
                        "permission_history=empty".to_string()
                    } else {
                        policy
                            .permission_history
                            .iter()
                            .rev()
                            .take(8)
                            .map(|entry| {
                                format!(
                                    "{} allowed={} reason={}",
                                    entry.capability, entry.allowed, entry.reason
                                )
                            })
                            .collect::<Vec<_>>()
                            .join("\n")
                    }
                })
                .unwrap_or_else(|| "permission_history=empty".to_string()),
        );
        Ok(bindings)
    }

    fn build_chat_bindings(
        &self,
        surface_id: &str,
    ) -> Result<BTreeMap<String, String>, RuntimeError> {
        let inbox = load_chat_inbox(&self.data_dir)?;
        let notifications = load_chat_notifications(&self.data_dir)?;
        let rooms = load_chat_rooms(&self.data_dir)?;
        let sessions = load_chat_sessions(&self.data_dir)?;
        let topology_file = self.data_dir.join("topology.json");
        let topology = if topology_file.exists() {
            Some(
                void_transport::PeerTopology::load(&topology_file)
                    .map_err(|error| RuntimeError::Topology(error.to_string()))?,
            )
        } else {
            None
        };
        let current_room = rooms.current_room.clone().or_else(|| {
            rooms
                .rooms
                .iter()
                .find(|room| room.joined)
                .map(|room| room.room.clone())
        });
        let current_room_members = current_room
            .as_ref()
            .and_then(|room_name| rooms.rooms.iter().find(|room| room.room == *room_name));
        let active_peer_count = topology
            .as_ref()
            .map(|topology| {
                topology
                    .peers
                    .values()
                    .filter(|peer| {
                        matches!(
                            peer.state,
                            void_transport::PeerConnectionState::Active
                                | void_transport::PeerConnectionState::Syncing
                        )
                    })
                    .count()
            })
            .unwrap_or_default();

        let mut bindings = BTreeMap::new();
        bindings.insert(
            "chat.status".to_string(),
            format!(
                "mounted surfaces={} active sessions={} inbox={} unread={} rooms={} notifications={} room_revision={}",
                self.state.mounts.len(),
                self.state
                    .sessions
                    .iter()
                    .filter(|session| session.session_state == RuntimeSessionState::Active)
                    .count(),
                inbox.messages.len(),
                unread_count(&inbox, None),
                rooms.rooms.len(),
                notifications.notifications.len(),
                rooms.sync_revision,
            ),
        );
        bindings.insert(
            "chat.current_room".to_string(),
            current_room
                .clone()
                .map(|room| format!("current room={room}"))
                .unwrap_or_else(|| "current room=none".to_string()),
        );
        bindings.insert(
            "chat.room_members".to_string(),
            current_room_members
                .map(format_room_members)
                .unwrap_or_else(|| "room members unavailable".to_string()),
        );
        bindings.insert(
            "chat.unread_count".to_string(),
            format!(
                "unread_total={} unread_current_room={}",
                unread_count(&inbox, None),
                unread_count(&inbox, current_room.as_deref())
            ),
        );
        bindings.insert(
            "chat.notifications".to_string(),
            if self.has_surface_capability(surface_id, "surface/notifications") {
                format_chat_notifications(&notifications)
            } else {
                "notifications permission denied".to_string()
            },
        );
        bindings.insert(
            "chat.connected_peers".to_string(),
            format!("connected_peers={active_peer_count}"),
        );
        bindings.insert(
            "chat.peers".to_string(),
            topology
                .as_ref()
                .map(|topology| {
                    if topology.peers.is_empty() {
                        "no peers observed".to_string()
                    } else {
                        topology
                            .peers
                            .values()
                            .map(|peer| {
                                format!(
                                    "{} {} last_seen={} sessions={}",
                                    peer.peer_id,
                                    peer.state,
                                    peer.last_seen_unix_ms,
                                    peer.session.session_id.as_deref().unwrap_or("-")
                                )
                            })
                            .collect::<Vec<_>>()
                            .join("\n")
                    }
                })
                .unwrap_or_else(|| "topology unavailable".to_string()),
        );
        bindings.insert(
            "chat.rooms".to_string(),
            if rooms.rooms.is_empty() {
                "no rooms joined".to_string()
            } else {
                rooms
                    .rooms
                    .iter()
                    .map(|room| {
                        format!(
                            "{} joined={} active_members={} members={} events={}",
                            room.room_name,
                            room.joined,
                            room.active_members,
                            room.members
                                .iter()
                                .map(|member| format!("{}:{}", member.peer_id, member.presence))
                                .collect::<Vec<_>>()
                                .join(","),
                            room.event_history.len(),
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            },
        );
        bindings.insert(
            "chat.inbox".to_string(),
            format_chat_inbox_messages(&inbox, current_room.as_deref()),
        );
        bindings.insert(
            "chat.inbox_messages".to_string(),
            format_chat_inbox_messages(&inbox, current_room.as_deref()),
        );
        bindings.insert(
            "chat.active_sessions".to_string(),
            if !self.has_surface_capability(surface_id, "runtime/session-access") {
                "runtime session access denied".to_string()
            } else if sessions.sessions.is_empty() {
                "no chat sessions".to_string()
            } else {
                sessions
                    .sessions
                    .iter()
                    .map(|session| {
                        format!(
                            "{} {} {} last_activity={}",
                            session.peer_id,
                            session.session_id,
                            session.encryption_state,
                            session.last_activity_unix_ms,
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            },
        );
        bindings.insert(
            "chat.sessions".to_string(),
            bindings
                .get("chat.active_sessions")
                .cloned()
                .unwrap_or_else(|| "no chat sessions".to_string()),
        );
        Ok(bindings)
    }

    fn find_registration_by_route(&self, route: &str) -> Option<&RuntimeSurfaceRegistration> {
        let uri = VoidUri::new(
            route
                .trim_start_matches("void://")
                .split('/')
                .next()
                .unwrap_or(route),
            "/",
            None,
        )
        .ok()?;
        self.state.registry.iter().find(|entry| {
            entry.domain == uri.authority() || entry.entry_uri.authority() == uri.authority()
        })
    }

    fn require_capability(&self, surface_id: &str, capability: &str) -> Result<(), RuntimeError> {
        if self.has_surface_capability(surface_id, capability) {
            Ok(())
        } else {
            Err(RuntimeError::PermissionDenied {
                surface_id: surface_id.to_string(),
                missing_capabilities: vec![capability.to_string()],
            })
        }
    }

    fn has_surface_capability(&self, surface_id: &str, capability: &str) -> bool {
        if let Some(grant) = self
            .state
            .permissions
            .iter()
            .find(|grant| grant.surface_id == surface_id && grant.capability == capability)
        {
            grant.allowed
        } else if self.gateway_capability_is_trusted(surface_id, capability) {
            true
        } else {
            is_safe_capability(capability)
        }
    }

    fn negotiate_capabilities(
        &mut self,
        surface_id: &str,
        peer_owner: &str,
        requested: &[String],
        events: &mut Vec<TransportEvent>,
        now: u128,
    ) -> (Vec<RuntimePermissionGrant>, Vec<String>) {
        let mut grants = Vec::new();
        let mut rejected = Vec::new();

        for capability in requested {
            events.push(TransportEvent::CapabilityRequested {
                route: surface_id.to_string(),
                capability: capability.clone(),
                peer_owner: peer_owner.to_string(),
            });

            let allowed = self
                .state
                .permissions
                .iter()
                .find(|entry| {
                    entry.surface_id == surface_id
                        && entry.peer_owner == peer_owner
                        && entry.capability == *capability
                })
                .map(|entry| entry.allowed)
                .unwrap_or_else(|| {
                    self.gateway_capability_is_trusted(surface_id, capability)
                        || (self.runtime.config.auto_grant_safe_capabilities
                            && is_safe_capability(capability))
                });

            let grant = RuntimePermissionGrant {
                surface_id: surface_id.to_string(),
                peer_owner: peer_owner.to_string(),
                capability: capability.clone(),
                allowed,
                persisted: allowed && is_safe_capability(capability),
                requested_at_unix_ms: now,
                decided_at_unix_ms: now,
            };
            if allowed {
                events.push(TransportEvent::CapabilityGranted {
                    peer_id: peer_owner.to_string(),
                    capability: capability.clone(),
                    scope: surface_id.to_string(),
                });
            } else {
                events.push(TransportEvent::CapabilityRejected {
                    peer_id: peer_owner.to_string(),
                    capability: capability.clone(),
                    reason: "permission not granted in runtime shell".to_string(),
                });
                rejected.push(capability.clone());
            }
            self.upsert_permission(grant.clone());
            grants.push(grant);
        }

        (grants, rejected)
    }

    fn record_failed_mount(
        &mut self,
        uri: &VoidUri,
        route: &DnsResolvedRoute,
        surface_id: &str,
        grants: &[RuntimePermissionGrant],
        rejected_capabilities: &[String],
    ) -> Result<(), RuntimeError> {
        let now = unix_millis();
        let session_id = format!("rt-failed-{}", now);
        self.upsert_mount(MountedRuntimeSurface {
            route: uri.to_string(),
            surface_id: surface_id.to_string(),
            owner_peer: route.target_peer_id.clone(),
            capabilities: grants
                .iter()
                .filter(|grant| grant.allowed)
                .map(|grant| grant.capability.clone())
                .collect(),
            surface_kind: self
                .state
                .registry
                .iter()
                .find(|entry| entry.surface_id == surface_id)
                .map(|entry| entry.surface_kind)
                .unwrap_or(RuntimeSurfaceKind::Application),
            runtime_state: RuntimeLifecycleState::Failed,
            mount_state: MountState::Failed,
            session_state: RuntimeSessionState::Failed,
            mounted_at_unix_ms: now,
            last_activity_unix_ms: now,
            session_id,
            mount_latency_ms: route.resolution_latency_ms,
            failure_reason: Some(format!(
                "missing capabilities: {}",
                rejected_capabilities.join(",")
            )),
        });
        self.state.updated_unix_ms = now;
        self.persist()
    }

    fn record_gateway_failure(
        &mut self,
        uri: &VoidUri,
        route: &DnsResolvedRoute,
        registration: &RuntimeSurfaceRegistration,
        session_id: &str,
        reason: &str,
    ) -> Result<(), RuntimeError> {
        let now = unix_millis();
        let mut bridge = self.prepare_gateway_bridge_context(registration, route, session_id)?;
        bridge.lifecycle_state = GatewayBridgeLifecycleState::Failed;
        bridge.last_error = Some(reason.to_string());
        bridge.cache_state = Some("miss".to_string());
        bridge.updated_at_unix_ms = now;

        self.upsert_mount(MountedRuntimeSurface {
            route: uri.to_string(),
            surface_id: registration.surface_id.clone(),
            owner_peer: route.target_peer_id.clone(),
            capabilities: Vec::new(),
            surface_kind: RuntimeSurfaceKind::Gateway,
            runtime_state: RuntimeLifecycleState::Failed,
            mount_state: MountState::Failed,
            session_state: RuntimeSessionState::Failed,
            mounted_at_unix_ms: now,
            last_activity_unix_ms: now,
            session_id: session_id.to_string(),
            mount_latency_ms: route.resolution_latency_ms,
            failure_reason: Some(reason.to_string()),
        });
        self.upsert_gateway_bridge_session(GatewayBridgeSessionRecord {
            session_id: session_id.to_string(),
            gateway_domain: registration.domain.clone(),
            gateway_id: registration.surface_id.clone(),
            gateway_peer: route.target_peer_id.clone(),
            external_target: bridge.external_target.clone(),
            fetch_state: "failed".to_string(),
            response_state: "unavailable".to_string(),
            permission_state: if matches!(
                self.gateway_trust_state(&registration.domain),
                GatewayTrustState::Denied
            ) {
                "denied".to_string()
            } else {
                "rejected".to_string()
            },
            mount_state: MountState::Failed,
            started_at_unix_ms: now,
            last_activity_unix_ms: now,
            fetch_latency_ms: None,
            response_size: None,
            content_type: None,
            cache_state: bridge.cache_state.clone(),
            last_error: Some(reason.to_string()),
        });
        self.upsert_gateway_route(GatewayRouteState {
            route: uri.to_string(),
            gateway_domain: registration.domain.clone(),
            gateway_id: registration.surface_id.clone(),
            trust_state: self.gateway_trust_state(&registration.domain),
            session_id: session_id.to_string(),
            active: false,
            bridge,
            last_error: Some(reason.to_string()),
            updated_unix_ms: now,
        });
        self.state.updated_unix_ms = now;
        self.persist()
    }

    fn ensure_route_registration(
        &mut self,
        route: &DnsResolvedRoute,
    ) -> Result<RuntimeSurfaceRegistration, RuntimeError> {
        if let Some(existing) = self
            .state
            .registry
            .iter()
            .find(|entry| entry.domain == route.domain.as_str())
            .cloned()
        {
            return Ok(existing);
        }

        let domain = route.domain.as_str().to_string();
        let is_gateway = route.runtime_surface == "gateway" || domain.ends_with(".gateway.void");
        let registration = RuntimeSurfaceRegistration {
            domain: domain.clone(),
            surface_id: if is_gateway {
                gateway_surface_id(&domain)
            } else {
                route.runtime_surface.clone()
            },
            runtime_surface: route.runtime_surface.clone(),
            entry_uri: route.uri.clone(),
            owner_peer_id: route.target_peer_id.clone(),
            capabilities: route.capabilities.clone(),
            handler: if is_gateway {
                "void-gateway".to_string()
            } else {
                format!("void-{}", route.runtime_surface)
            },
            surface_kind: if is_gateway {
                RuntimeSurfaceKind::Gateway
            } else {
                RuntimeSurfaceKind::Application
            },
            supported_protocols: if is_gateway {
                vec!["http".to_string(), "https".to_string()]
            } else {
                vec!["void".to_string()]
            },
            external_route_base: if is_gateway {
                Some(default_gateway_external_base(&domain))
            } else {
                None
            },
            trust_level: if is_gateway {
                Some(self.gateway_trust_level(&domain))
            } else {
                None
            },
            surface_path: Some(self.ensure_builtin_surface_file(
                if is_gateway {
                    "gateway.surface"
                } else {
                    "chat.surface"
                },
                if is_gateway {
                    GATEWAY_SURFACE_SOURCE
                } else {
                    CHAT_SURFACE_SOURCE
                },
            )),
        };
        self.upsert_registration(registration.clone());
        self.state.updated_unix_ms = unix_millis();
        self.persist()?;
        Ok(registration)
    }

    fn validate_gateway_trust(
        &mut self,
        registration: &RuntimeSurfaceRegistration,
        events: &mut Vec<TransportEvent>,
    ) -> Result<(), RuntimeError> {
        let policy = self
            .state
            .gateway_trust
            .iter()
            .find(|policy| policy.gateway_domain == registration.domain)
            .cloned()
            .unwrap_or_else(|| GatewayTrustPolicy {
                gateway_domain: registration.domain.clone(),
                gateway_id: registration.surface_id.clone(),
                owner_peer: registration.owner_peer_id.clone(),
                trust_level: registration.trust_level.unwrap_or_default(),
                trust_state: GatewayTrustState::Pending,
                capability_scope: registration.capabilities.clone(),
                runtime_restrictions: vec!["awaiting-explicit-allow".to_string()],
                permission_history: Vec::new(),
                last_warning: Some("gateway trust policy pending".to_string()),
                updated_unix_ms: unix_millis(),
            });

        if !self
            .state
            .gateway_trust
            .iter()
            .any(|existing| existing.gateway_domain == policy.gateway_domain)
        {
            self.upsert_gateway_trust(policy.clone());
        }

        events.push(TransportEvent::GatewayTrustEvaluated {
            domain: registration.domain.clone(),
            trust_state: format!("{:?}", policy.trust_state),
            trust_level: format!("{:?}", policy.trust_level),
            warning: policy.last_warning.clone(),
        });

        if matches!(policy.trust_state, GatewayTrustState::Denied)
            || matches!(policy.trust_level, GatewayTrustLevel::Untrusted)
        {
            return Err(RuntimeError::GatewayTrustDenied(
                registration.domain.clone(),
            ));
        }

        Ok(())
    }

    fn gateway_trust_state(&self, gateway_domain: &str) -> GatewayTrustState {
        self.state
            .gateway_trust
            .iter()
            .find(|policy| policy.gateway_domain == gateway_domain)
            .map(|policy| policy.trust_state)
            .unwrap_or_default()
    }

    fn gateway_trust_level(&self, gateway_domain: &str) -> GatewayTrustLevel {
        self.state
            .gateway_trust
            .iter()
            .find(|policy| policy.gateway_domain == gateway_domain)
            .map(|policy| policy.trust_level)
            .unwrap_or_default()
    }

    fn gateway_capability_is_trusted(&self, surface_id: &str, capability: &str) -> bool {
        if !capability.starts_with("gateway.") {
            return false;
        }
        let Some(registration) = self.state.registry.iter().find(|entry| {
            entry.surface_id == surface_id && entry.surface_kind == RuntimeSurfaceKind::Gateway
        }) else {
            return false;
        };
        self.state
            .gateway_trust
            .iter()
            .find(|policy| policy.gateway_domain == registration.domain)
            .map(|policy| {
                policy.trust_state == GatewayTrustState::Trusted
                    && policy.trust_level == GatewayTrustLevel::Trusted
            })
            .unwrap_or(false)
    }

    fn prepare_gateway_bridge_context(
        &self,
        registration: &RuntimeSurfaceRegistration,
        route: &DnsResolvedRoute,
        session_id: &str,
    ) -> Result<GatewayBridgeContext, RuntimeError> {
        let target_path = route.uri.path().trim_start_matches('/').to_string();
        let query = parse_query_pairs(route.uri.query());
        let external_target = gateway_external_target(
            registration.external_route_base.as_deref(),
            &target_path,
            route.uri.query(),
        );
        Ok(GatewayBridgeContext {
            gateway_domain: registration.domain.clone(),
            gateway_id: registration.surface_id.clone(),
            owner_peer: registration.owner_peer_id.clone(),
            supported_protocols: registration.supported_protocols.clone(),
            active_external_route: route.uri.to_string(),
            external_target: external_target.clone(),
            lifecycle_state: GatewayBridgeLifecycleState::Prepared,
            request: HttpBridgeRequest {
                method: "GET".to_string(),
                target_path: if target_path.is_empty() {
                    "/".to_string()
                } else {
                    format!("/{target_path}")
                },
                url: external_target,
                headers: BTreeMap::from([
                    (
                        "accept".to_string(),
                        "application/json, text/plain, text/html".to_string(),
                    ),
                    ("x-void-gateway-session".to_string(), session_id.to_string()),
                    (
                        "x-void-gateway-domain".to_string(),
                        registration.domain.clone(),
                    ),
                ]),
                query,
                body: None,
                timeout_ms: DEFAULT_GATEWAY_TIMEOUT_MS,
                gateway_peer: registration.owner_peer_id.clone(),
                request_id: format!("req-{}", unix_millis()),
                issued_at_unix_ms: unix_millis(),
            },
            response: None,
            stream_chunks: Vec::new(),
            last_error: None,
            fetch_latency_ms: None,
            response_size: None,
            snapshot_id: None,
            cache_state: Some("miss".to_string()),
            render_mode: None,
            created_at_unix_ms: unix_millis(),
            updated_at_unix_ms: unix_millis(),
        })
    }

    async fn execute_gateway_fetch(
        &self,
        registration: &RuntimeSurfaceRegistration,
        route: &DnsResolvedRoute,
        session_id: &str,
        events: &mut Vec<TransportEvent>,
    ) -> Result<(GatewayBridgeContext, GatewayBridgeSessionRecord), RuntimeError> {
        let mut bridge = self.prepare_gateway_bridge_context(registration, route, session_id)?;
        events.push(TransportEvent::BridgeSessionStarted {
            route: route.uri.to_string(),
            session_id: session_id.to_string(),
            gateway_id: registration.surface_id.clone(),
            external_target: bridge.external_target.clone(),
        });
        events.push(TransportEvent::GatewayFetchDispatched {
            route: route.uri.to_string(),
            gateway_id: registration.surface_id.clone(),
            request_id: bridge.request.request_id.clone(),
            method: bridge.request.method.clone(),
            external_target: bridge.external_target.clone(),
        });

        let request_method = reqwest::Method::from_bytes(bridge.request.method.as_bytes())
            .map_err(|error| {
                RuntimeError::HttpBridge(format!(
                    "invalid HTTP method {}: {error}",
                    bridge.request.method
                ))
            })?;
        let client = reqwest::Client::builder()
            .timeout(Duration::from_millis(bridge.request.timeout_ms.max(1)))
            .build()
            .map_err(|error| {
                RuntimeError::HttpBridge(format!("failed to build HTTP bridge client: {error}"))
            })?;
        let mut request = client.request(request_method, &bridge.request.url);
        for (name, value) in &bridge.request.headers {
            if let (Ok(header_name), Ok(header_value)) = (
                reqwest::header::HeaderName::from_bytes(name.as_bytes()),
                reqwest::header::HeaderValue::from_str(value),
            ) {
                request = request.header(header_name, header_value);
            }
        }
        if let Some(body) = &bridge.request.body {
            request = request.body(body.clone());
        }

        let fetch_started_at = unix_millis();
        let response = request.send().await.map_err(|error| {
            RuntimeError::HttpBridge(format!(
                "gateway request {} failed: {error}",
                bridge.request.request_id
            ))
        })?;
        let status = response.status().as_u16();
        let headers = response
            .headers()
            .iter()
            .map(|(name, value)| {
                (
                    name.as_str().to_string(),
                    value.to_str().unwrap_or("<binary>").to_string(),
                )
            })
            .collect::<BTreeMap<_, _>>();
        let content_type = headers.get("content-type").cloned();
        let response_body = response
            .bytes()
            .await
            .map_err(|error| {
                RuntimeError::HttpBridge(format!("gateway response body failed: {error}"))
            })?
            .to_vec();
        let fetch_completed_at = unix_millis();
        let fetch_latency_ms = fetch_completed_at.saturating_sub(fetch_started_at);
        let response_id = format!("rsp-{}", fetch_completed_at);
        let stream_chunks = build_stream_chunks(&response_body, GATEWAY_STREAM_CHUNK_BYTES);
        let response_size = response_body.len();
        let cache_state = gateway_cache_state(&headers);
        let render_mode = gateway_render_mode(content_type.as_deref());
        let response_body_state = truncate_bytes(&response_body, GATEWAY_RESPONSE_STATE_LIMIT);
        let snapshot = GatewayResourceSnapshot {
            snapshot_id: format!("snap-{}", fetch_completed_at),
            request_id: bridge.request.request_id.clone(),
            response_id: response_id.clone(),
            route: route.uri.to_string(),
            gateway_domain: registration.domain.clone(),
            gateway_peer: registration.owner_peer_id.clone(),
            external_target: bridge.external_target.clone(),
            status,
            content_type: content_type
                .clone()
                .unwrap_or_else(|| "application/octet-stream".to_string()),
            response_size,
            body_preview: render_gateway_preview(content_type.as_deref(), &response_body),
            headers: headers.clone(),
            fetched_at_unix_ms: fetch_completed_at,
            cache_state: cache_state.clone(),
            cache_key: stable_hash(&(bridge.request.url.clone(), bridge.request.method.clone()))?,
            origin: registration.domain.clone(),
        };
        self.persist_gateway_snapshot(&snapshot)?;

        bridge.lifecycle_state = GatewayBridgeLifecycleState::Mounted;
        bridge.response = Some(HttpBridgeResponse {
            status,
            headers: headers.clone(),
            body: Some(response_body_state.clone()),
            content_type: content_type.clone(),
            body_bytes: Some(response_size),
            response_size,
            fetched_at_unix_ms: Some(fetch_completed_at),
            gateway_peer: registration.owner_peer_id.clone(),
            response_id: response_id.clone(),
            body_truncated: response_body_state.len() < response_size,
            cacheable: !matches!(cache_state.as_str(), "bypass"),
            completed_at_unix_ms: Some(fetch_completed_at),
        });
        bridge.stream_chunks = stream_chunks.clone();
        bridge.fetch_latency_ms = Some(fetch_latency_ms);
        bridge.response_size = Some(response_size);
        bridge.snapshot_id = Some(snapshot.snapshot_id.clone());
        bridge.cache_state = Some(cache_state.clone());
        bridge.render_mode = Some(render_mode.clone());
        bridge.updated_at_unix_ms = fetch_completed_at;

        events.push(TransportEvent::GatewayResponseReceived {
            route: route.uri.to_string(),
            gateway_id: registration.surface_id.clone(),
            response_id,
            status,
            content_type: content_type.clone(),
            bytes: response_size,
            latency_ms: fetch_latency_ms,
        });
        events.push(TransportEvent::GatewayStreamUpdated {
            route: route.uri.to_string(),
            gateway_id: registration.surface_id.clone(),
            chunks: stream_chunks.len(),
            bytes: response_size,
            final_chunk: true,
        });
        events.push(TransportEvent::ExternalResourceMounted {
            route: route.uri.to_string(),
            gateway_id: registration.surface_id.clone(),
            snapshot_id: snapshot.snapshot_id.clone(),
            content_type: content_type.clone(),
            bytes: response_size,
            cache_state: cache_state.clone(),
        });

        Ok((
            bridge.clone(),
            GatewayBridgeSessionRecord {
                session_id: session_id.to_string(),
                gateway_domain: registration.domain.clone(),
                gateway_id: registration.surface_id.clone(),
                gateway_peer: registration.owner_peer_id.clone(),
                external_target: bridge.external_target.clone(),
                fetch_state: "completed".to_string(),
                response_state: format!(
                    "status={} content_type={}",
                    status,
                    content_type
                        .as_deref()
                        .unwrap_or("application/octet-stream")
                ),
                permission_state: "granted".to_string(),
                mount_state: MountState::Mounted,
                started_at_unix_ms: bridge.created_at_unix_ms,
                last_activity_unix_ms: fetch_completed_at,
                fetch_latency_ms: Some(fetch_latency_ms),
                response_size: Some(response_size),
                content_type,
                cache_state: Some(cache_state),
                last_error: None,
            },
        ))
    }

    fn upsert_registration(&mut self, registration: RuntimeSurfaceRegistration) {
        if let Some(existing) = self
            .state
            .registry
            .iter_mut()
            .find(|entry| entry.domain == registration.domain)
        {
            *existing = registration;
        } else {
            self.state.registry.push(registration);
        }
    }

    fn upsert_permission(&mut self, grant: RuntimePermissionGrant) {
        if let Some(existing) = self.state.permissions.iter_mut().find(|entry| {
            entry.surface_id == grant.surface_id
                && entry.peer_owner == grant.peer_owner
                && entry.capability == grant.capability
        }) {
            *existing = grant;
        } else {
            self.state.permissions.push(grant);
        }
    }

    fn upsert_mount(&mut self, mount: MountedRuntimeSurface) {
        if let Some(existing) = self
            .state
            .mounts
            .iter_mut()
            .find(|entry| entry.route == mount.route && entry.surface_id == mount.surface_id)
        {
            *existing = mount;
        } else {
            self.state.mounts.push(mount);
        }
    }

    fn upsert_gateway_route(&mut self, route: GatewayRouteState) {
        if let Some(existing) = self
            .state
            .gateway_routes
            .iter_mut()
            .find(|entry| entry.route == route.route)
        {
            *existing = route;
        } else {
            self.state.gateway_routes.push(route);
        }
    }

    fn upsert_gateway_bridge_session(&mut self, session: GatewayBridgeSessionRecord) {
        if let Some(existing) = self
            .state
            .gateway_bridge_sessions
            .iter_mut()
            .find(|entry| entry.session_id == session.session_id)
        {
            *existing = session;
        } else {
            self.state.gateway_bridge_sessions.push(session);
        }
    }

    fn persist_gateway_snapshot(
        &self,
        snapshot: &GatewayResourceSnapshot,
    ) -> Result<(), RuntimeError> {
        let snapshot_dir = self
            .data_dir
            .join("gateway")
            .join(sanitize_gateway_path_component(&snapshot.gateway_domain));
        let snapshot_path = snapshot_dir.join(format!(
            "{}.json",
            sanitize_gateway_path_component(&snapshot.snapshot_id)
        ));
        persist_json(&snapshot_path, snapshot)
    }

    fn upsert_gateway_trust(&mut self, trust: GatewayTrustPolicy) {
        if let Some(existing) = self
            .state
            .gateway_trust
            .iter_mut()
            .find(|entry| entry.gateway_domain == trust.gateway_domain)
        {
            *existing = trust;
        } else {
            self.state.gateway_trust.push(trust);
        }
    }

    fn upsert_ui_surface(&mut self, surface: RuntimeUiSurfaceState) {
        if let Some(existing) = self
            .state
            .ui_surfaces
            .iter_mut()
            .find(|entry| entry.route == surface.route)
        {
            *existing = surface;
        } else {
            self.state.ui_surfaces.push(surface);
        }
    }

    fn upsert_session(&mut self, session: RuntimeSessionRecord) {
        if let Some(existing) = self
            .state
            .sessions
            .iter_mut()
            .find(|entry| entry.session_id == session.session_id)
        {
            *existing = session;
        } else {
            self.state.sessions.push(session);
        }
    }

    pub fn register_gateway(
        &mut self,
        domain: &str,
        supported_protocols: Vec<String>,
        capabilities: Vec<String>,
        external_route_base: Option<String>,
        trust_level: GatewayTrustLevel,
    ) -> Result<RuntimeSurfaceRegistration, RuntimeError> {
        let entry = RuntimeSurfaceRegistration {
            domain: domain.to_string(),
            surface_id: gateway_surface_id(domain),
            runtime_surface: "gateway".to_string(),
            entry_uri: VoidUri::new(domain, "/", None)
                .map_err(|_| RuntimeError::GatewayInvalidDomain(domain.to_string()))?,
            owner_peer_id: self.runtime.identity.peer_id().to_string(),
            capabilities: if capabilities.is_empty() {
                gateway_capabilities()
            } else {
                capabilities
            },
            handler: "void-gateway".to_string(),
            surface_kind: RuntimeSurfaceKind::Gateway,
            supported_protocols: if supported_protocols.is_empty() {
                vec!["http".to_string(), "https".to_string()]
            } else {
                supported_protocols
            },
            external_route_base: Some(
                external_route_base.unwrap_or_else(|| default_gateway_external_base(domain)),
            ),
            trust_level: Some(trust_level),
            surface_path: Some(
                self.ensure_builtin_surface_file("gateway.surface", GATEWAY_SURFACE_SOURCE),
            ),
        };
        self.upsert_registration(entry.clone());
        self.upsert_gateway_trust(GatewayTrustPolicy {
            gateway_domain: entry.domain.clone(),
            gateway_id: entry.surface_id.clone(),
            owner_peer: entry.owner_peer_id.clone(),
            trust_level,
            trust_state: if matches!(trust_level, GatewayTrustLevel::Trusted) {
                GatewayTrustState::Trusted
            } else {
                GatewayTrustState::Pending
            },
            capability_scope: entry.capabilities.clone(),
            runtime_restrictions: vec!["http-bridge-foundation".to_string()],
            permission_history: entry
                .capabilities
                .iter()
                .map(|capability| GatewayPermissionDecision {
                    capability: capability.clone(),
                    allowed: matches!(trust_level, GatewayTrustLevel::Trusted),
                    reason: "gateway registration".to_string(),
                    decided_at_unix_ms: unix_millis(),
                })
                .collect(),
            last_warning: if matches!(trust_level, GatewayTrustLevel::Trusted) {
                None
            } else {
                Some("gateway requires explicit capability grants".to_string())
            },
            updated_unix_ms: unix_millis(),
        });
        self.state.updated_unix_ms = unix_millis();
        self.persist()?;
        Ok(entry)
    }

    pub fn gateway_registrations(&self) -> Vec<RuntimeSurfaceRegistration> {
        self.state
            .registry
            .iter()
            .filter(|entry| entry.surface_kind == RuntimeSurfaceKind::Gateway)
            .cloned()
            .collect()
    }

    pub fn gateway_trust_policy(&self, domain: &str) -> Option<GatewayTrustPolicy> {
        self.state
            .gateway_trust
            .iter()
            .find(|policy| policy.gateway_domain == domain)
            .cloned()
    }

    pub fn set_gateway_trust(
        &mut self,
        domain: &str,
        trust_state: GatewayTrustState,
        trust_level: GatewayTrustLevel,
        reason: &str,
    ) -> Result<(), RuntimeError> {
        let registration = self
            .state
            .registry
            .iter()
            .find(|entry| {
                entry.domain == domain && entry.surface_kind == RuntimeSurfaceKind::Gateway
            })
            .cloned()
            .ok_or_else(|| RuntimeError::GatewayNotRegistered(domain.to_string()))?;
        let mut policy = self
            .gateway_trust_policy(domain)
            .unwrap_or(GatewayTrustPolicy {
                gateway_domain: registration.domain.clone(),
                gateway_id: registration.surface_id.clone(),
                owner_peer: registration.owner_peer_id.clone(),
                trust_level,
                trust_state,
                capability_scope: registration.capabilities.clone(),
                runtime_restrictions: Vec::new(),
                permission_history: Vec::new(),
                last_warning: None,
                updated_unix_ms: unix_millis(),
            });
        policy.trust_level = trust_level;
        policy.trust_state = trust_state;
        policy.last_warning = if matches!(
            trust_state,
            GatewayTrustState::Warning | GatewayTrustState::Pending
        ) {
            Some(reason.to_string())
        } else {
            None
        };
        policy.updated_unix_ms = unix_millis();
        self.upsert_gateway_trust(policy);
        self.state.updated_unix_ms = unix_millis();
        self.persist()
    }

    pub fn record_gateway_permission_decision(
        &mut self,
        domain: &str,
        capability: &str,
        allowed: bool,
        reason: &str,
    ) -> Result<(), RuntimeError> {
        let mut policy = self
            .gateway_trust_policy(domain)
            .ok_or_else(|| RuntimeError::GatewayNotRegistered(domain.to_string()))?;
        policy.permission_history.push(GatewayPermissionDecision {
            capability: capability.to_string(),
            allowed,
            reason: reason.to_string(),
            decided_at_unix_ms: unix_millis(),
        });
        if policy.permission_history.len() > 24 {
            let keep_from = policy.permission_history.len() - 24;
            policy.permission_history.drain(0..keep_from);
        }
        policy.updated_unix_ms = unix_millis();
        self.upsert_gateway_trust(policy);
        self.state.updated_unix_ms = unix_millis();
        self.persist()
    }
}

impl RuntimeShell<PersistentVoidDns> {
    pub async fn synchronize_registry_dns(
        &self,
        owner: &PersistentNodeIdentity,
    ) -> Result<(), RuntimeError> {
        for entry in &self.state.registry {
            let domain = VoidDomain::new(entry.domain.clone())?;
            let existing = self.runtime.dns.resolve(&domain).await?;
            let needs_publish = existing
                .as_ref()
                .map(|record| {
                    record.owner_peer_id != owner.peer_id_string()
                        || record.target_peer_id != entry.owner_peer_id
                        || record.runtime_surface != entry.runtime_surface
                        || record.capabilities != entry.capabilities
                })
                .unwrap_or(true);
            if needs_publish {
                self.runtime
                    .dns
                    .publish_record(
                        owner,
                        domain,
                        entry.owner_peer_id.clone(),
                        entry.runtime_surface.clone(),
                        entry.capabilities.clone(),
                        Duration::from_secs(300),
                    )
                    .await?;
            }
        }
        Ok(())
    }
}

#[async_trait]
pub trait Runtime {
    async fn handle(&self, command: RuntimeCommand) -> Result<RuntimeEvent, RuntimeError>;
}

#[async_trait]
impl<R> Runtime for VoidRuntime<R>
where
    R: VoidDnsResolver + Send + Sync + 'static,
{
    async fn handle(&self, command: RuntimeCommand) -> Result<RuntimeEvent, RuntimeError> {
        match command {
            RuntimeCommand::OpenUri(uri) => {
                if uri.is_void_domain() {
                    let domain = VoidDomain::new(uri.authority())?;
                    let route = self
                        .dns
                        .resolve_route(&uri)
                        .await?
                        .ok_or_else(|| RuntimeError::NameNotFound(domain.to_string()))?;
                    Ok(RuntimeEvent::NameResolved {
                        uri,
                        target: ResolutionTarget::Service(void_dns::ServiceTarget {
                            peer_id: route.target_peer_id.clone(),
                            runtime_surface: route.runtime_surface.clone(),
                            capabilities: route.capabilities.clone(),
                        }),
                        route,
                    })
                } else {
                    Ok(RuntimeEvent::UriOpened { uri })
                }
            }
            RuntimeCommand::SendEnvelope { peer_id, envelope } => {
                self.network
                    .send(TransportCommand::SendEnvelope { peer_id, envelope })
                    .await?;
                Ok(RuntimeEvent::EnvelopeQueued)
            }
            RuntimeCommand::RequestPermission { permission } => {
                Ok(RuntimeEvent::PermissionRequested { permission })
            }
        }
    }
}

#[derive(Debug, Clone)]
pub enum RuntimeCommand {
    OpenUri(VoidUri),
    SendEnvelope { peer_id: String, envelope: Envelope },
    RequestPermission { permission: Permission },
}

#[derive(Debug, Clone)]
pub enum RuntimeEvent {
    UriOpened {
        uri: VoidUri,
    },
    NameResolved {
        uri: VoidUri,
        target: ResolutionTarget,
        route: DnsResolvedRoute,
    },
    EnvelopeQueued,
    PermissionRequested {
        permission: Permission,
    },
}

#[derive(Debug, Error)]
pub enum RuntimeError {
    #[error("VOID runtime IO error: {0}")]
    Io(#[from] io::Error),
    #[error("VOID runtime JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("VOID runtime DNS error: {0}")]
    Dns(#[from] void_dns::VoidDnsError),
    #[error("VOID runtime transport error: {0}")]
    Transport(#[from] TransportError),
    #[error("VOID name was not found: {0}")]
    NameNotFound(String),
    #[error("VOID runtime topology error: {0}")]
    Topology(String),
    #[error("VOID runtime permission denied for surface {surface_id}: {missing_capabilities:?}")]
    PermissionDenied {
        surface_id: String,
        missing_capabilities: Vec<String>,
    },
    #[error("VOID runtime surface registration missing for route {0}")]
    SurfaceRegistrationMissing(String),
    #[error("VOID runtime surface document missing for surface {0}")]
    SurfaceDocumentMissing(String),
    #[error("VOID runtime surface did not produce a render for route {0}")]
    SurfaceNotRendered(String),
    #[error("VOID gateway trust denied for domain {0}")]
    GatewayTrustDenied(String),
    #[error("VOID gateway is not registered for domain {0}")]
    GatewayNotRegistered(String),
    #[error("VOID gateway domain is invalid: {0}")]
    GatewayInvalidDomain(String),
    #[error("VOID runtime HTTP bridge error: {0}")]
    HttpBridge(String),
    #[error("VOID runtime input missing: {0}")]
    InputMissing(String),
    #[error("VOID runtime action is unknown: {0}")]
    UnknownAction(String),
    #[error("VOID runtime chat error: {0}")]
    Chat(#[from] ChatError),
    #[error("VOID runtime UI error: {0}")]
    Ui(#[from] ui::RuntimeUiError),
}

const CHAT_SURFACE_SOURCE: &str = include_str!("../../../apps/void-chat/surfaces/chat.surface");
const GATEWAY_SURFACE_SOURCE: &str = include_str!("../surfaces/gateway.surface");
const DEFAULT_GATEWAY_TIMEOUT_MS: u64 = 5_000;
const GATEWAY_STREAM_CHUNK_BYTES: usize = 1024;
const GATEWAY_RESPONSE_STATE_LIMIT: usize = 64 * 1024;
const GATEWAY_RESPONSE_PREVIEW_LIMIT: usize = 4096;

fn persist_json<T: Serialize>(path: &Path, value: &T) -> Result<(), RuntimeError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, serde_json::to_vec_pretty(value)?)?;
    Ok(())
}

fn load_json_or_default<T>(path: &Path) -> Result<T, RuntimeError>
where
    T: for<'de> Deserialize<'de> + Default,
{
    if !path.exists() {
        return Ok(T::default());
    }
    Ok(serde_json::from_slice(&fs::read(path)?)?)
}

fn unix_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default()
}

fn is_safe_capability(capability: &str) -> bool {
    capability == "messaging"
        || capability == "encrypted-sessions"
        || capability == "chat/direct-e2ee"
        || capability == "dns/addressable"
        || capability == "routing/void-uri"
        || capability == "service/chat"
        || capability == "surface/messaging"
        || capability == "surface/room-access"
        || capability == "surface/notifications"
        || capability == "runtime/session-access"
        || capability.starts_with("service/")
}

fn prefixed_input_state(input_state: &BTreeMap<String, String>) -> BTreeMap<String, String> {
    input_state
        .iter()
        .map(|(key, value)| (format!("input.{key}"), value.clone()))
        .collect()
}

fn classify_runtime_state(
    surface_id: &str,
    bindings: &BTreeMap<String, String>,
) -> BTreeMap<String, String> {
    bindings
        .iter()
        .filter(|(key, _)| !is_distributed_state_key(surface_id, key))
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect()
}

fn classify_distributed_state(
    surface_id: &str,
    bindings: &BTreeMap<String, String>,
) -> BTreeMap<String, String> {
    bindings
        .iter()
        .filter(|(key, _)| is_distributed_state_key(surface_id, key))
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect()
}

fn is_distributed_state_key(surface_id: &str, key: &str) -> bool {
    match surface_id {
        "chat" => matches!(
            key,
            "chat.peers"
                | "chat.rooms"
                | "chat.inbox"
                | "chat.inbox_messages"
                | "chat.sessions"
                | "chat.active_sessions"
                | "chat.notifications"
                | "chat.current_room"
                | "chat.room_members"
                | "chat.unread_count"
                | "chat.connected_peers"
        ),
        _ if surface_id.starts_with("gateway:") => matches!(
            key,
            "gateway.trust_state"
                | "gateway.active_routes"
                | "gateway.runtime_capabilities"
                | "gateway.external_target"
                | "gateway.bridge_state"
                | "gateway.request"
                | "gateway.response_status"
                | "gateway.response_headers"
                | "gateway.response_preview"
                | "gateway.response_cache"
                | "gateway.bridge_sessions"
                | "gateway.snapshot_state"
                | "gateway.permission_history"
        ),
        _ => false,
    }
}

fn gateway_surface_id(domain: &str) -> String {
    format!("gateway:{}", domain.to_ascii_lowercase())
}

fn gateway_capabilities() -> Vec<String> {
    vec![
        "gateway.external-routing".to_string(),
        "gateway.http".to_string(),
        "gateway.relay".to_string(),
        "gateway.resource-fetch".to_string(),
        "gateway.response-stream".to_string(),
        "routing/void-uri".to_string(),
        "service/gateway".to_string(),
    ]
}

fn default_gateway_external_base(domain: &str) -> String {
    let host = domain
        .trim_end_matches(".void")
        .trim_end_matches(".gateway")
        .trim_end_matches('.')
        .replace('.', "/");
    format!("https://{host}")
}

fn gateway_external_target(base: Option<&str>, target_path: &str, query: Option<&str>) -> String {
    let mut target = if let Some(base) = base {
        if target_path.is_empty() {
            base.to_string()
        } else {
            format!("{}/{}", base.trim_end_matches('/'), target_path)
        }
    } else if target_path.is_empty() {
        "/".to_string()
    } else {
        target_path.to_string()
    };
    if let Some(query) = query.filter(|query| !query.trim().is_empty()) {
        target.push('?');
        target.push_str(query);
    }
    target
}

fn parse_query_pairs(query: Option<&str>) -> BTreeMap<String, String> {
    let mut pairs = BTreeMap::new();
    for entry in query.unwrap_or_default().split('&') {
        if entry.trim().is_empty() {
            continue;
        }
        let (key, value) = entry.split_once('=').unwrap_or((entry, ""));
        pairs.insert(key.to_string(), value.to_string());
    }
    pairs
}

fn truncate_bytes(bytes: &[u8], limit: usize) -> Vec<u8> {
    bytes.iter().take(limit).copied().collect()
}

fn build_stream_chunks(bytes: &[u8], chunk_size: usize) -> Vec<HttpBridgeStreamChunk> {
    if bytes.is_empty() {
        return vec![HttpBridgeStreamChunk {
            sequence: 0,
            bytes: 0,
            final_chunk: true,
        }];
    }

    bytes
        .chunks(chunk_size.max(1))
        .enumerate()
        .map(|(index, chunk)| HttpBridgeStreamChunk {
            sequence: index as u64,
            bytes: chunk.len(),
            final_chunk: (index + 1) * chunk_size >= bytes.len(),
        })
        .collect()
}

fn gateway_cache_state(headers: &BTreeMap<String, String>) -> String {
    let cache_control = headers
        .get("cache-control")
        .map(|value| value.to_ascii_lowercase())
        .unwrap_or_default();
    if cache_control.contains("no-store") || cache_control.contains("no-cache") {
        "bypass".to_string()
    } else if cache_control.contains("max-age")
        || headers.contains_key("etag")
        || headers.contains_key("last-modified")
    {
        "cacheable".to_string()
    } else {
        "ephemeral".to_string()
    }
}

fn gateway_render_mode(content_type: Option<&str>) -> String {
    let content_type = content_type
        .unwrap_or("application/octet-stream")
        .to_ascii_lowercase();
    if content_type.starts_with("application/json") {
        "json".to_string()
    } else if content_type.starts_with("text/html") {
        "html-fallback".to_string()
    } else if content_type.starts_with("text/plain") || content_type.starts_with("text/") {
        "text".to_string()
    } else {
        "binary".to_string()
    }
}

fn render_gateway_preview(content_type: Option<&str>, body: &[u8]) -> String {
    let preview = match gateway_render_mode(content_type).as_str() {
        "json" => render_json_preview(body),
        "html-fallback" => render_html_preview(body),
        "text" => String::from_utf8_lossy(body).to_string(),
        _ => format!("binary response preview unavailable ({} bytes)", body.len()),
    };
    truncate_text(&preview, GATEWAY_RESPONSE_PREVIEW_LIMIT)
}

fn render_json_preview(body: &[u8]) -> String {
    serde_json::from_slice::<serde_json::Value>(body)
        .ok()
        .and_then(|value| serde_json::to_string_pretty(&value).ok())
        .unwrap_or_else(|| String::from_utf8_lossy(body).to_string())
}

fn render_html_preview(body: &[u8]) -> String {
    let html = String::from_utf8_lossy(body);
    let title = extract_html_title(&html).unwrap_or_else(|| "untitled html document".to_string());
    let text = strip_html_tags(&html);
    format!(
        "html title={title}\n\n{}",
        truncate_text(&text, GATEWAY_RESPONSE_PREVIEW_LIMIT / 2)
    )
}

fn extract_html_title(html: &str) -> Option<String> {
    let lower = html.to_ascii_lowercase();
    let start = lower.find("<title>")? + "<title>".len();
    let end = lower[start..].find("</title>")? + start;
    Some(html[start..end].trim().to_string())
}

fn strip_html_tags(html: &str) -> String {
    let mut output = String::new();
    let mut inside_tag = false;
    for character in html.chars() {
        match character {
            '<' => inside_tag = true,
            '>' => {
                inside_tag = false;
                output.push(' ');
            }
            _ if !inside_tag => output.push(character),
            _ => {}
        }
    }
    output.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn truncate_text(text: &str, limit: usize) -> String {
    let truncated = text.chars().take(limit).collect::<String>();
    if truncated.chars().count() < text.chars().count() {
        format!("{truncated}...")
    } else {
        truncated
    }
}

fn format_header_bindings(headers: &BTreeMap<String, String>) -> String {
    if headers.is_empty() {
        return "headers=empty".to_string();
    }
    headers
        .iter()
        .take(12)
        .map(|(name, value)| format!("{name}: {value}"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn render_gateway_response_preview(response: &HttpBridgeResponse) -> String {
    response
        .body
        .as_ref()
        .map(|body| render_gateway_preview(response.content_type.as_deref(), body))
        .unwrap_or_else(|| "response preview unavailable".to_string())
}

fn sanitize_gateway_path_component(raw: &str) -> String {
    raw.chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '.' | '-' | '_') {
                character
            } else {
                '_'
            }
        })
        .collect()
}

fn format_room_members(room: &void_chat::ChatRoomSnapshot) -> String {
    if room.members.is_empty() {
        return "no room members".to_string();
    }

    room.members
        .iter()
        .map(|member| {
            format!(
                "{} {} last_seen={}",
                member.peer_id, member.presence, member.last_seen_unix_ms
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn format_chat_inbox_messages(
    inbox: &void_chat::ChatInboxState,
    current_room: Option<&str>,
) -> String {
    let messages = inbox
        .messages
        .iter()
        .filter(|message| match current_room {
            Some(room) => message.room.as_deref() == Some(room),
            None => true,
        })
        .rev()
        .take(12)
        .map(|message| {
            format!(
                "ts={} room={} from={} unread={} body={}",
                message.received_at_unix_ms,
                message.room.as_deref().unwrap_or("direct"),
                message.from_peer_id,
                if message.unread { "yes" } else { "no" },
                message.body,
            )
        })
        .collect::<Vec<_>>();

    if messages.is_empty() {
        "inbox empty".to_string()
    } else {
        messages.join("\n")
    }
}

fn format_chat_notifications(notifications: &void_chat::ChatNotificationsState) -> String {
    if notifications.notifications.is_empty() {
        return "notifications empty".to_string();
    }

    notifications
        .notifications
        .iter()
        .rev()
        .take(10)
        .map(|notification| {
            format!(
                "ts={} kind={} room={} peer={} unread={} message={}",
                notification.created_at_unix_ms,
                notification.kind,
                notification.room.as_deref().unwrap_or("-"),
                notification.peer_id.as_deref().unwrap_or("-"),
                if notification.unread { "yes" } else { "no" },
                notification.message,
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn merge_surface_state_maps(
    local_state: &BTreeMap<String, String>,
    runtime_state: &BTreeMap<String, String>,
    distributed_state: &BTreeMap<String, String>,
) -> BTreeMap<String, String> {
    let mut merged = BTreeMap::new();
    merged.extend(local_state.clone());
    merged.extend(runtime_state.clone());
    merged.extend(distributed_state.clone());
    merged
}

fn stored_surface_state(surface: &RuntimeUiSurfaceState) -> BTreeMap<String, String> {
    merge_surface_state_maps(
        &surface.local_state,
        &surface.runtime_state,
        &surface.distributed_state,
    )
}

fn diff_state_keys(
    previous: &BTreeMap<String, String>,
    current: &BTreeMap<String, String>,
) -> Vec<String> {
    let mut keys = previous
        .keys()
        .chain(current.keys())
        .cloned()
        .collect::<Vec<_>>();
    keys.sort();
    keys.dedup();
    keys.into_iter()
        .filter(|key| previous.get(key) != current.get(key))
        .collect()
}

fn stable_hash<T: Serialize>(value: &T) -> Result<String, RuntimeError> {
    let bytes = serde_json::to_vec(value)?;
    Ok(blake3::hash(&bytes).to_hex().to_string())
}

fn file_modified_unix_ms(path: &Path) -> Option<u128> {
    path.metadata()
        .ok()?
        .modified()
        .ok()?
        .duration_since(UNIX_EPOCH)
        .ok()
        .map(|duration| duration.as_millis())
}

fn fallback_surface_render(surface_id: &str, error: &str) -> TerminalRenderedSurface {
    TerminalRenderedSurface {
        output: format!(
            "== VOID Surface Error ==\nsurface={surface_id}\nerror={error}\n\nCommands: refresh | quit"
        ),
        actions: Vec::new(),
        render_duration_ms: 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{path::PathBuf, time::Duration};
    use void_dns::PersistentVoidDns;
    use void_identity::PersistentNodeIdentity;
    use void_transport::network_channels;

    #[tokio::test]
    async fn resolves_void_route_into_runtime_target() {
        let dns_dir = unique_test_dir("runtime-route");
        let dns = Arc::new(PersistentVoidDns::load_or_create(&dns_dir).unwrap());
        let owner = PersistentNodeIdentity::load_or_create_dir(dns_dir.join("identity")).unwrap();
        dns.publish_record(
            &owner,
            VoidDomain::new("chat.void").unwrap(),
            owner.peer_id_string(),
            "chat",
            vec!["service/chat".into(), "routing/void-uri".into()],
            Duration::from_secs(300),
        )
        .await
        .unwrap();

        let (network, _inbox) = network_channels(8);
        let runtime = VoidRuntime::new(
            NodeIdentity::generate(),
            dns,
            network,
            RuntimeConfig::default(),
        );
        let event = runtime
            .handle(RuntimeCommand::OpenUri(
                "void://chat.void/rooms/main".parse().unwrap(),
            ))
            .await
            .unwrap();

        match event {
            RuntimeEvent::NameResolved { route, .. } => {
                assert_eq!(route.target_peer_id, owner.peer_id_string());
                assert_eq!(route.runtime_surface, "chat");
                assert!(route.capabilities.contains(&"routing/void-uri".to_string()));
            }
            other => panic!("unexpected runtime event: {other:?}"),
        }
    }

    #[tokio::test]
    async fn opens_route_into_active_runtime_surface() {
        let dns_dir = unique_test_dir("runtime-open");
        let dns = Arc::new(PersistentVoidDns::load_or_create(&dns_dir).unwrap());
        let owner = PersistentNodeIdentity::load_or_create_dir(dns_dir.join("identity")).unwrap();
        dns.publish_record(
            &owner,
            VoidDomain::new("chat.void").unwrap(),
            owner.peer_id_string(),
            "chat",
            vec![
                "chat/direct-e2ee".into(),
                "dns/addressable".into(),
                "routing/void-uri".into(),
                "service/chat".into(),
            ],
            Duration::from_secs(300),
        )
        .await
        .unwrap();

        let (network, _inbox) = network_channels(8);
        let runtime = VoidRuntime::new(
            NodeIdentity::generate(),
            dns,
            network,
            RuntimeConfig::default(),
        );
        let mut shell = RuntimeShell::load_or_create(&dns_dir, runtime).unwrap();
        let result = shell
            .open_uri("void://chat.void".parse().unwrap())
            .await
            .unwrap();

        assert_eq!(result.mount.runtime_state, RuntimeLifecycleState::Active);
        assert_eq!(result.mount.mount_state, MountState::Mounted);
        assert_eq!(result.session.session_state, RuntimeSessionState::Active);
        assert!(result
            .granted_permissions
            .iter()
            .any(|grant| grant.capability == "chat/direct-e2ee"));
        assert!(result
            .events
            .iter()
            .any(|event| matches!(event, TransportEvent::SurfaceMounted { .. })));
        assert_eq!(shell.state().mounts.len(), 1);
        assert_eq!(shell.state().sessions.len(), 1);
    }

    #[tokio::test]
    async fn rejects_unsupported_sensitive_capability() {
        let dns_dir = unique_test_dir("runtime-permission");
        let dns = Arc::new(PersistentVoidDns::load_or_create(&dns_dir).unwrap());
        let owner = PersistentNodeIdentity::load_or_create_dir(dns_dir.join("identity")).unwrap();
        dns.publish_record(
            &owner,
            VoidDomain::new("vault.void").unwrap(),
            owner.peer_id_string(),
            "vault",
            vec!["storage".into(), "filesystem".into()],
            Duration::from_secs(300),
        )
        .await
        .unwrap();

        let (network, _inbox) = network_channels(8);
        let runtime = VoidRuntime::new(
            NodeIdentity::generate(),
            dns,
            network,
            RuntimeConfig::default(),
        );
        let mut shell = RuntimeShell::load_or_create(&dns_dir, runtime).unwrap();
        let error = shell
            .open_uri("void://vault.void".parse().unwrap())
            .await
            .unwrap_err();

        assert!(matches!(error, RuntimeError::PermissionDenied { .. }));
        assert!(shell
            .state()
            .mounts
            .iter()
            .any(|mount| mount.mount_state == MountState::Failed));
    }

    #[tokio::test]
    async fn opens_trusted_gateway_route_into_bridge_context() {
        use std::io::{Read, Write};
        use std::net::TcpListener;
        use std::thread;

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buffer = [0_u8; 1024];
            let _ = stream.read(&mut buffer).unwrap();
            let body = b"trusted gateway";
            let response = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: text/plain\r\ncontent-length: {}\r\n\r\n{}",
                body.len(),
                String::from_utf8_lossy(body)
            );
            stream.write_all(response.as_bytes()).unwrap();
        });

        let dns_dir = unique_test_dir("runtime-gateway-open");
        let dns = Arc::new(PersistentVoidDns::load_or_create(&dns_dir).unwrap());
        let owner = PersistentNodeIdentity::load_or_create_dir(dns_dir.join("identity")).unwrap();
        let (network, _inbox) = network_channels(8);
        let runtime = VoidRuntime::new(
            NodeIdentity::generate(),
            dns.clone(),
            network,
            RuntimeConfig::default(),
        );
        let mut shell = RuntimeShell::load_or_create(&dns_dir, runtime).unwrap();
        shell
            .register_gateway(
                "docs.gateway.void",
                vec!["http".into(), "https".into()],
                vec![
                    "gateway.http".into(),
                    "gateway.external-routing".into(),
                    "gateway.resource-fetch".into(),
                    "gateway.response-stream".into(),
                ],
                Some(format!("http://{}", address)),
                GatewayTrustLevel::Trusted,
            )
            .unwrap();
        shell
            .set_gateway_trust(
                "docs.gateway.void",
                GatewayTrustState::Trusted,
                GatewayTrustLevel::Trusted,
                "test trusted gateway",
            )
            .unwrap();
        shell.synchronize_registry_dns(&owner).await.unwrap();

        let result = shell
            .open_uri("void://docs.gateway.void/github/openai".parse().unwrap())
            .await
            .unwrap();

        assert_eq!(result.mount.surface_kind, RuntimeSurfaceKind::Gateway);
        assert!(result
            .events
            .iter()
            .any(|event| matches!(event, TransportEvent::GatewayMounted { .. })));
        assert!(shell.state().gateway_routes.iter().any(|route| {
            route.gateway_domain == "docs.gateway.void"
                && route.bridge.external_target.ends_with("github/openai")
        }));
        assert!(shell.state().gateway_bridge_sessions.iter().any(|session| {
            session.gateway_domain == "docs.gateway.void" && session.fetch_state == "completed"
        }));
    }

    #[tokio::test]
    async fn fetches_external_resource_into_runtime_gateway_surface() {
        use std::io::{Read, Write};
        use std::net::TcpListener;
        use std::thread;

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buffer = [0_u8; 2048];
            let _ = stream.read(&mut buffer).unwrap();
            let body = br#"{"bridge":"ok","runtime":"voidnet"}"#;
            let response = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\ncache-control: max-age=60\r\n\r\n{}",
                body.len(),
                String::from_utf8_lossy(body)
            );
            stream.write_all(response.as_bytes()).unwrap();
        });

        let dns_dir = unique_test_dir("runtime-http-bridge");
        let dns = Arc::new(PersistentVoidDns::load_or_create(&dns_dir).unwrap());
        let owner = PersistentNodeIdentity::load_or_create_dir(dns_dir.join("identity")).unwrap();
        let (network, _inbox) = network_channels(8);
        let runtime = VoidRuntime::new(
            NodeIdentity::generate(),
            dns.clone(),
            network,
            RuntimeConfig::default(),
        );
        let mut shell = RuntimeShell::load_or_create(&dns_dir, runtime).unwrap();
        shell
            .register_gateway(
                "example.gateway.void",
                vec!["http".into()],
                vec![
                    "gateway.http".into(),
                    "gateway.external-routing".into(),
                    "gateway.resource-fetch".into(),
                    "gateway.response-stream".into(),
                ],
                Some(format!("http://{}", address)),
                GatewayTrustLevel::Trusted,
            )
            .unwrap();
        shell
            .set_gateway_trust(
                "example.gateway.void",
                GatewayTrustState::Trusted,
                GatewayTrustLevel::Trusted,
                "test http bridge",
            )
            .unwrap();
        shell.synchronize_registry_dns(&owner).await.unwrap();

        let result = shell
            .open_uri(
                "void://example.gateway.void/status?source=test"
                    .parse()
                    .unwrap(),
            )
            .await
            .unwrap();

        assert!(result.rendered_surface.output.contains("bridge"));
        assert!(result
            .events
            .iter()
            .any(|event| matches!(event, TransportEvent::ExternalResourceMounted { .. })));
        assert!(shell.state().gateway_bridge_sessions.iter().any(|session| {
            session.gateway_domain == "example.gateway.void"
                && session.fetch_state == "completed"
                && session.response_size.unwrap_or_default() > 0
        }));
        assert!(shell.state().gateway_routes.iter().any(|route| {
            route.gateway_domain == "example.gateway.void"
                && route
                    .bridge
                    .response
                    .as_ref()
                    .map(|response| response.status)
                    == Some(200)
                && route.bridge.snapshot_id.is_some()
        }));
        let snapshot_dir = dns_dir.join("gateway").join("example.gateway.void");
        assert!(snapshot_dir.exists());
    }

    #[tokio::test]
    async fn denies_untrusted_gateway_mount() {
        let dns_dir = unique_test_dir("runtime-gateway-deny");
        let dns = Arc::new(PersistentVoidDns::load_or_create(&dns_dir).unwrap());
        let owner = PersistentNodeIdentity::load_or_create_dir(dns_dir.join("identity")).unwrap();
        let (network, _inbox) = network_channels(8);
        let runtime = VoidRuntime::new(
            NodeIdentity::generate(),
            dns.clone(),
            network,
            RuntimeConfig::default(),
        );
        let mut shell = RuntimeShell::load_or_create(&dns_dir, runtime).unwrap();
        shell
            .register_gateway(
                "unsafe.gateway.void",
                vec!["http".into()],
                vec!["gateway.http".into(), "gateway.external-routing".into()],
                Some("https://unsafe.example".into()),
                GatewayTrustLevel::Restricted,
            )
            .unwrap();
        shell
            .set_gateway_trust(
                "unsafe.gateway.void",
                GatewayTrustState::Denied,
                GatewayTrustLevel::Untrusted,
                "test denied gateway",
            )
            .unwrap();
        shell.synchronize_registry_dns(&owner).await.unwrap();

        let error = shell
            .open_uri("void://unsafe.gateway.void".parse().unwrap())
            .await
            .unwrap_err();

        assert!(matches!(error, RuntimeError::GatewayTrustDenied(_)));
        assert!(shell.state().mounts.iter().any(|mount| {
            mount.surface_kind == RuntimeSurfaceKind::Gateway
                && mount.mount_state == MountState::Failed
        }));
        assert!(shell.state().gateway_routes.iter().any(|route| {
            route.gateway_domain == "unsafe.gateway.void"
                && !route.active
                && route.last_error.is_some()
        }));
    }

    fn unique_test_dir(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!("voidnet-runtime-{label}-{}", std::process::id()))
    }
}
