use anyhow::{Context, Result};
use crossterm::{
    cursor::Show,
    event::{self, Event, KeyCode, KeyEventKind, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    prelude::{Alignment, Color, Modifier, Span, Style},
    text::Line,
    widgets::{Block, Borders, Cell, Clear, List, ListItem, Paragraph, Row, Table, Tabs, Wrap},
    Frame, Terminal,
};
use std::{
    collections::VecDeque,
    fs,
    io,
    panic::{self, AssertUnwindSafe},
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};
use void_chat::{
    enqueue_local_command, load_chat_inbox, load_chat_notifications, load_chat_rooms,
    load_chat_sessions, unread_count, ChatLocalCommand,
};
use void_dns::{PersistentVoidDns, VoidDnsResolver, VoidDomain};
use void_identity::{NodeIdentity, PersistentNodeIdentity};
use void_runtime::{RuntimeConfig, RuntimeShell, RuntimeShellState, VoidRuntime};
use void_transport::{
    network_channels, MeshState, PeerConnectionState, PeerTopology, RuntimeShellTopologyInfo,
};

use crate::{
    load_topology, now_unix_ms, parse_void_uri_input, persist_runtime_shell_topology,
    render_chat_diagnostics, render_gateway_diagnostics,
};

const TICK_RATE: Duration = Duration::from_millis(700);
const TAB_TITLES: [&str; 5] = ["Dashboard", "Chat", "Topology", "Gateways", "Events"];
const MAX_EVENTS: usize = 256;
const COMPACT_WIDTH: u16 = 72;
const COMPACT_HEIGHT: u16 = 18;
const EVENT_PAGE_STEP: usize = 8;

pub(crate) fn run_console(data_dir: PathBuf) -> Result<()> {
    let mut session = TerminalSession::enter()?;
    let mut app = ConsoleApp::new(data_dir)?;
    session.run(&mut app)
}

struct TerminalSession {
    terminal: Terminal<CrosstermBackend<io::Stdout>>,
}

impl TerminalSession {
    fn enter() -> Result<Self> {
        enable_raw_mode()?;

        let mut stdout = io::stdout();
        if let Err(error) = execute!(stdout, EnterAlternateScreen) {
            let _ = disable_raw_mode();
            return Err(error.into());
        }

        let backend = CrosstermBackend::new(stdout);
        let terminal = match Terminal::new(backend) {
            Ok(terminal) => terminal,
            Err(error) => {
                let _ = disable_raw_mode();
                let _ = execute!(io::stdout(), LeaveAlternateScreen, Show);
                return Err(error.into());
            }
        };

        Ok(Self { terminal })
    }

    fn run(&mut self, app: &mut ConsoleApp) -> Result<()> {
        let result = panic::catch_unwind(AssertUnwindSafe(|| run_loop(&mut self.terminal, app)));
        match result {
            Ok(result) => result,
            Err(payload) => panic::resume_unwind(payload),
        }
    }
}

impl Drop for TerminalSession {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(self.terminal.backend_mut(), LeaveAlternateScreen, Show);
        let _ = self.terminal.show_cursor();
    }
}

fn run_loop(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>, app: &mut ConsoleApp) -> Result<()> {
    let mut last_tick = Instant::now();

    loop {
        terminal.draw(|frame| draw_console(frame, app))?;

        let timeout = TICK_RATE.saturating_sub(last_tick.elapsed());
        if event::poll(timeout)? {
            match event::read()? {
                Event::Key(key) if key.kind == KeyEventKind::Press => app.handle_key(key.code, key.modifiers),
                Event::Resize(_, _) => app.clear_status_if_transient(),
                _ => {}
            }
        }

        if last_tick.elapsed() >= TICK_RATE {
            if let Err(error) = app.refresh() {
                app.report_error(format!("refresh failed: {error}"));
            }
            last_tick = Instant::now();
        }

        if app.should_quit {
            break;
        }
    }

    Ok(())
}

struct ConsoleApp {
    data_dir: PathBuf,
    tab: usize,
    command_mode: bool,
    command_input: String,
    status: String,
    status_is_error: bool,
    snapshot: ConsoleSnapshot,
    previous_digest: ConsoleDigest,
    events: VecDeque<ConsoleEvent>,
    event_scroll: usize,
    should_quit: bool,
}

impl ConsoleApp {
    fn new(data_dir: PathBuf) -> Result<Self> {
        let snapshot = ConsoleSnapshot::load(&data_dir)?;
        let digest = snapshot.digest();
        let mut events = VecDeque::new();
        events.push_front(ConsoleEvent::operator("console attached to runtime state"));

        let mut app = Self {
            data_dir,
            tab: 0,
            command_mode: false,
            command_input: String::new(),
            status: "Tab/Shift+Tab switch views · : palette · Ctrl+C or q exits".to_string(),
            status_is_error: false,
            snapshot,
            previous_digest: digest,
            events,
            event_scroll: 0,
            should_quit: false,
        };
        app.merge_persisted_events();
        Ok(app)
    }

    fn refresh(&mut self) -> Result<()> {
        let snapshot = ConsoleSnapshot::load(&self.data_dir)?;
        let digest = snapshot.digest();
        let previous_digest = self.previous_digest.clone();
        self.record_digest_delta(&previous_digest, &digest);
        self.previous_digest = digest;
        self.snapshot = snapshot;
        self.merge_persisted_events();
        self.event_scroll = self.event_scroll.min(self.max_event_scroll());
        Ok(())
    }

    fn handle_key(&mut self, code: KeyCode, modifiers: KeyModifiers) {
        if self.command_mode {
            self.handle_palette_key(code, modifiers);
            return;
        }

        match code {
            KeyCode::Char('c') if modifiers.contains(KeyModifiers::CONTROL) => {
                self.set_status("console closed", false);
                self.should_quit = true;
            }
            KeyCode::Char('q') => {
                self.set_status("console closed", false);
                self.should_quit = true;
            }
            KeyCode::Char(':') => {
                self.command_mode = true;
                self.command_input.clear();
            }
            KeyCode::Char('1') => self.tab = 0,
            KeyCode::Char('2') => self.tab = 1,
            KeyCode::Char('3') => self.tab = 2,
            KeyCode::Char('4') => self.tab = 3,
            KeyCode::Char('5') => self.tab = 4,
            KeyCode::Tab => self.tab = (self.tab + 1) % TAB_TITLES.len(),
            KeyCode::BackTab => self.tab = self.tab.checked_sub(1).unwrap_or(TAB_TITLES.len() - 1),
            KeyCode::Char('r') => match self.refresh() {
                Ok(()) => self.set_status("runtime snapshot refreshed", false),
                Err(error) => self.report_error(format!("refresh failed: {error}")),
            },
            KeyCode::Up if self.tab == 4 => self.event_scroll = (self.event_scroll + 1).min(self.max_event_scroll()),
            KeyCode::Down if self.tab == 4 => self.event_scroll = self.event_scroll.saturating_sub(1),
            KeyCode::PageUp if self.tab == 4 => {
                self.event_scroll = (self.event_scroll + EVENT_PAGE_STEP).min(self.max_event_scroll())
            }
            KeyCode::PageDown if self.tab == 4 => {
                self.event_scroll = self.event_scroll.saturating_sub(EVENT_PAGE_STEP)
            }
            KeyCode::Home if self.tab == 4 => self.event_scroll = self.max_event_scroll(),
            KeyCode::End if self.tab == 4 => self.event_scroll = 0,
            KeyCode::Char('k') if self.tab == 4 => self.event_scroll = (self.event_scroll + 1).min(self.max_event_scroll()),
            KeyCode::Char('j') if self.tab == 4 => self.event_scroll = self.event_scroll.saturating_sub(1),
            _ => {}
        }
    }

    fn handle_palette_key(&mut self, code: KeyCode, modifiers: KeyModifiers) {
        match code {
            KeyCode::Esc => {
                self.command_mode = false;
                self.command_input.clear();
                self.set_status("palette closed", false);
            }
            KeyCode::Enter => {
                let input = self.command_input.trim().to_string();
                self.command_mode = false;
                self.command_input.clear();
                if !input.is_empty() {
                    self.execute_palette_command(&input);
                } else {
                    self.set_status("palette closed", false);
                }
            }
            KeyCode::Backspace => {
                self.command_input.pop();
            }
            KeyCode::Char('u') if modifiers.contains(KeyModifiers::CONTROL) => {
                self.command_input.clear();
            }
            KeyCode::Char(character) => {
                self.command_input.push(character);
            }
            _ => {}
        }
    }

    fn execute_palette_command(&mut self, input: &str) {
        match parse_palette_command(input, self.snapshot.current_room.as_deref()) {
            Ok(command) => {
                if let Err(error) = self.apply_palette_command(command) {
                    self.report_error(error.to_string());
                }
            }
            Err(usage) => self.report_error(usage),
        }
    }

    fn apply_palette_command(&mut self, command: PaletteCommand) -> Result<()> {
        match command {
            PaletteCommand::Sync => {
                self.refresh()?;
                self.push_event(ConsoleEvent::operator("runtime sync requested"));
                self.set_status("runtime snapshot refreshed", false);
            }
            PaletteCommand::Quit => {
                self.set_status("console closed", false);
                self.should_quit = true;
            }
            PaletteCommand::Open { route } => {
                let summary = open_runtime_route(&self.data_dir, &route)?;
                self.push_event(ConsoleEvent::runtime(summary.clone()));
                self.set_status(summary, false);
                self.refresh()?;
            }
            PaletteCommand::Resolve { domain } => {
                let status = resolve_domain(&self.data_dir, &domain)?;
                self.push_event(ConsoleEvent::dns(status.clone()));
                self.set_status(status, false);
            }
            PaletteCommand::Join { room } => {
                enqueue_local_command(&self.data_dir, ChatLocalCommand::Join { room: room.clone() })?;
                let status = format!("join queued for {}", room);
                self.push_event(ConsoleEvent::chat(status.clone()));
                self.set_status(status, false);
            }
            PaletteCommand::Leave { room } => {
                enqueue_local_command(&self.data_dir, ChatLocalCommand::Leave { room: room.clone() })?;
                let status = format!("leave queued for {}", room);
                self.push_event(ConsoleEvent::chat(status.clone()));
                self.set_status(status, false);
            }
            PaletteCommand::Switch { room } => {
                enqueue_local_command(&self.data_dir, ChatLocalCommand::SwitchRoom { room: room.clone() })?;
                let status = format!("switch queued to {}", room);
                self.push_event(ConsoleEvent::chat(status.clone()));
                self.set_status(status, false);
            }
            PaletteCommand::MarkRead { room } => {
                enqueue_local_command(&self.data_dir, ChatLocalCommand::MarkRead { room: room.clone() })?;
                let status = format!("mark-read queued for {}", room.as_deref().unwrap_or("all rooms"));
                self.push_event(ConsoleEvent::chat(status.clone()));
                self.set_status(status, false);
            }
            PaletteCommand::Direct { peer_id, message } => {
                enqueue_local_command(
                    &self.data_dir,
                    ChatLocalCommand::SendDirect {
                        peer_id: peer_id.clone(),
                        message,
                    },
                )?;
                let status = format!("direct message queued for {}", shorten_peer_id(&peer_id));
                self.push_event(ConsoleEvent::chat(status.clone()));
                self.set_status(status, false);
            }
            PaletteCommand::RoomSend { room, message } => {
                enqueue_local_command(
                    &self.data_dir,
                    ChatLocalCommand::SendRoom {
                        room: room.clone(),
                        message,
                    },
                )?;
                let status = format!("room message queued for {}", room);
                self.push_event(ConsoleEvent::chat(status.clone()));
                self.set_status(status, false);
            }
            PaletteCommand::InspectPeer { peer_id } => {
                self.tab = 2;
                let status = format!("topology focus moved to {}", shorten_peer_id(peer_id.trim()));
                self.push_event(ConsoleEvent::topology(status.clone()));
                self.set_status(status, false);
            }
            PaletteCommand::InspectGateway { domain } => {
                self.tab = 3;
                let status = format!("gateway focus moved to {}", domain.trim());
                self.push_event(ConsoleEvent::gateway(status.clone()));
                self.set_status(status, false);
            }
        }

        Ok(())
    }

    fn push_event(&mut self, event: ConsoleEvent) {
        let should_skip = self.events.iter().any(|current| {
            current.subsystem == event.subsystem
                && current.message == event.message
                && current.timestamp == event.timestamp
        });
        if should_skip {
            return;
        }

        self.events.push_front(event);
        while self.events.len() > MAX_EVENTS {
            self.events.pop_back();
        }
        self.event_scroll = self.event_scroll.min(self.max_event_scroll());
    }

    fn record_digest_delta(&mut self, previous: &ConsoleDigest, current: &ConsoleDigest) {
        if previous.mesh_state != current.mesh_state {
            self.push_event(ConsoleEvent::topology(format!(
                "mesh state {} -> {}",
                previous.mesh_state, current.mesh_state
            )));
        }
        if previous.active_peers != current.active_peers {
            self.push_event(ConsoleEvent::topology(format!(
                "active peers {} -> {}",
                previous.active_peers, current.active_peers
            )));
        }
        if previous.active_sessions != current.active_sessions {
            self.push_event(ConsoleEvent::runtime(format!(
                "runtime sessions {} -> {}",
                previous.active_sessions, current.active_sessions
            )));
        }
        if previous.mounted_surfaces != current.mounted_surfaces {
            self.push_event(ConsoleEvent::runtime(format!(
                "mounted surfaces {} -> {}",
                previous.mounted_surfaces, current.mounted_surfaces
            )));
        }
        if previous.current_mount_route != current.current_mount_route {
            self.push_event(ConsoleEvent::runtime(format!(
                "mounted route {} -> {}",
                previous.current_mount_route.as_deref().unwrap_or("-"),
                current.current_mount_route.as_deref().unwrap_or("-")
            )));
        }
        if previous.current_room != current.current_room {
            self.push_event(ConsoleEvent::chat(format!(
                "current room {} -> {}",
                previous.current_room.as_deref().unwrap_or("-"),
                current.current_room.as_deref().unwrap_or("-")
            )));
        }
        if previous.unread_messages != current.unread_messages {
            self.push_event(ConsoleEvent::chat(format!(
                "unread messages {} -> {}",
                previous.unread_messages, current.unread_messages
            )));
        }
        if previous.gateway_routes != current.gateway_routes {
            self.push_event(ConsoleEvent::gateway(format!(
                "gateway routes {} -> {}",
                previous.gateway_routes, current.gateway_routes
            )));
        }
        if previous.bridge_sessions != current.bridge_sessions {
            self.push_event(ConsoleEvent::gateway(format!(
                "bridge sessions {} -> {}",
                previous.bridge_sessions, current.bridge_sessions
            )));
        }
        if previous.last_action != current.last_action {
            if let Some(action) = &current.last_action {
                self.push_event(ConsoleEvent::runtime(format!("last action {}", action)));
            }
        }
        if previous.last_error != current.last_error {
            if let Some(error) = &current.last_error {
                self.push_event(ConsoleEvent::error(format!("runtime error: {}", error)));
            }
        }
    }

    fn report_error(&mut self, message: impl Into<String>) {
        let message = message.into();
        self.push_event(ConsoleEvent::error(message.clone()));
        self.set_status(message, true);
    }

    fn set_status(&mut self, message: impl Into<String>, is_error: bool) {
        self.status = message.into();
        self.status_is_error = is_error;
    }

    fn clear_status_if_transient(&mut self) {
        if self.status_is_error {
            return;
        }
        if self.status.starts_with("palette closed") || self.status.starts_with("topology focus moved") {
            self.status = "Tab/Shift+Tab switch views · : palette · Ctrl+C or q exits".to_string();
        }
    }

    fn max_event_scroll(&self) -> usize {
        self.events.len().saturating_sub(1)
    }

    fn merge_persisted_events(&mut self) {
        let events = load_recent_event_log(&self.data_dir, 96).unwrap_or_default();
        for event in events.into_iter().rev() {
            self.push_event(event);
        }
    }
}

#[derive(Clone)]
struct ConsoleSnapshot {
    node_id: String,
    topology: PeerTopology,
    runtime_state: Option<RuntimeShellState>,
    gateway_lines: Vec<String>,
    chat_lines: Vec<String>,
    inbox: Vec<InboxRow>,
    rooms: Vec<RoomRow>,
    current_room: Option<String>,
    selected_room: Option<SelectedRoom>,
    chat_sessions: Vec<String>,
    notifications: Vec<String>,
    unread_messages: usize,
    notification_count: usize,
    current_mount_route: Option<String>,
    last_error: Option<String>,
}

impl ConsoleSnapshot {
    fn load(data_dir: &Path) -> Result<Self> {
        let data_dir_buf = data_dir.to_path_buf();
        let node_id = PersistentNodeIdentity::load_or_create_dir(&data_dir_buf)
            .map(|identity| identity.peer_id_string())
            .unwrap_or_else(|_| "unknown-local-peer".to_string());
        let topology = load_topology(&data_dir_buf)?;
        let runtime_state = load_runtime_shell_state(&data_dir_buf)?;
        let gateway_lines = render_gateway_diagnostics(&data_dir_buf).unwrap_or_default();
        let chat_lines = render_chat_diagnostics(&data_dir_buf).unwrap_or_default();

        let inbox_state = load_chat_inbox(&data_dir_buf).unwrap_or_default();
        let unread_messages = unread_count(&inbox_state, None);
        let inbox = inbox_state
            .messages
            .into_iter()
            .rev()
            .take(20)
            .map(|message| InboxRow {
                from: message.from_peer_id,
                room: message.room.or(message.room_name).unwrap_or_else(|| "direct".to_string()),
                body: message.body,
                unread: message.unread,
                received_at_unix_ms: message.received_at_unix_ms,
            })
            .collect::<Vec<_>>();

        let rooms_state = load_chat_rooms(&data_dir_buf)?;
        let current_room = rooms_state.current_room.clone();
        let rooms = rooms_state
            .rooms
            .iter()
            .map(|room| RoomRow {
                name: room.room.clone(),
                joined: room.joined,
                is_current: current_room.as_deref() == Some(room.room.as_str()),
                active_members: room.active_members,
                events: room.event_history.len(),
            })
            .collect::<Vec<_>>();
        let selected_room = rooms_state
            .rooms
            .iter()
            .find(|room| current_room.as_deref() == Some(room.room.as_str()))
            .or_else(|| rooms_state.rooms.iter().find(|room| room.joined))
            .or_else(|| rooms_state.rooms.first())
            .map(|room| SelectedRoom {
                name: room.room.clone(),
                joined: room.joined,
                active_members: room.active_members,
                members: room
                    .members
                    .iter()
                    .take(8)
                    .map(|member| format!("{} {}", shorten_peer_id(&member.peer_id), member.presence))
                    .collect::<Vec<_>>(),
                recent_events: room
                    .event_history
                    .iter()
                    .rev()
                    .take(6)
                    .map(|event| {
                        let body = event.body.as_deref().unwrap_or("-");
                        format!(
                            "{} {} {}",
                            clock_string(event.timestamp_unix_ms),
                            event.event_type,
                            truncate_with_ellipsis(&format!("{} {body}", shorten_peer_id(&event.peer_id)), 38)
                        )
                    })
                    .collect::<Vec<_>>(),
            });

        let sessions_state = load_chat_sessions(&data_dir_buf).unwrap_or_default();
        let mut chat_sessions = sessions_state
            .sessions
            .into_iter()
            .map(|session| {
                format!(
                    "{} {} {}",
                    shorten_peer_id(&session.peer_id),
                    session.encryption_state,
                    session.transport_state
                )
            })
            .collect::<Vec<_>>();
        chat_sessions.truncate(8);

        let notifications_state = load_chat_notifications(&data_dir_buf)?;
        let notification_count = notifications_state
            .notifications
            .iter()
            .filter(|entry| entry.unread)
            .count();
        let notifications = notifications_state
            .notifications
            .into_iter()
            .rev()
            .take(6)
            .map(|entry| {
                let scope = entry.room.or(entry.peer_id).unwrap_or_else(|| "runtime".to_string());
                format!("{} {}", scope, truncate_with_ellipsis(&entry.message, 34))
            })
            .collect::<Vec<_>>();

        let current_mount_route = runtime_state.as_ref().and_then(|state| {
            state
                .mounts
                .iter()
                .max_by_key(|mount| mount.last_activity_unix_ms)
                .map(|mount| mount.route.clone())
        });
        let last_error = topology
            .runtime_shell
            .as_ref()
            .and_then(|runtime| runtime.last_error.clone());

        Ok(Self {
            node_id,
            topology,
            runtime_state,
            gateway_lines,
            chat_lines,
            inbox,
            rooms,
            current_room,
            selected_room,
            chat_sessions,
            notifications,
            unread_messages,
            notification_count,
            current_mount_route,
            last_error,
        })
    }

    fn digest(&self) -> ConsoleDigest {
        let runtime = self.runtime_info();
        ConsoleDigest {
            mesh_state: self.topology.mesh_state.to_string(),
            active_peers: self
                .topology
                .peers
                .values()
                .filter(|peer| matches!(peer.state, PeerConnectionState::Active | PeerConnectionState::Syncing))
                .count(),
            mounted_surfaces: runtime.map(|state| state.mounted_surfaces).unwrap_or_default(),
            active_sessions: runtime.map(|state| state.active_sessions).unwrap_or_default(),
            gateway_routes: runtime.map(|state| state.gateway_active_routes).unwrap_or_default(),
            bridge_sessions: runtime.map(|state| state.gateway_bridge_sessions).unwrap_or_default(),
            current_room: self.current_room.clone(),
            current_mount_route: self.current_mount_route.clone(),
            unread_messages: self.unread_messages,
            last_action: runtime.and_then(|state| state.last_action.clone()),
            last_error: self.last_error.clone(),
        }
    }

    fn runtime_info(&self) -> Option<&RuntimeShellTopologyInfo> {
        self.topology.runtime_shell.as_ref()
    }
}

#[derive(Clone)]
struct ConsoleDigest {
    mesh_state: String,
    active_peers: usize,
    mounted_surfaces: usize,
    active_sessions: usize,
    gateway_routes: usize,
    bridge_sessions: usize,
    current_room: Option<String>,
    current_mount_route: Option<String>,
    unread_messages: usize,
    last_action: Option<String>,
    last_error: Option<String>,
}

#[derive(Clone)]
struct RoomRow {
    name: String,
    joined: bool,
    is_current: bool,
    active_members: usize,
    events: usize,
}

#[derive(Clone)]
struct SelectedRoom {
    name: String,
    joined: bool,
    active_members: usize,
    members: Vec<String>,
    recent_events: Vec<String>,
}

#[derive(Clone)]
struct InboxRow {
    from: String,
    room: String,
    body: String,
    unread: bool,
    received_at_unix_ms: u128,
}

#[derive(Clone)]
struct ConsoleEvent {
    timestamp: String,
    subsystem: &'static str,
    message: String,
    tone: EventTone,
}

impl ConsoleEvent {
    fn operator(message: impl Into<String>) -> Self {
        Self::new("operator", message, EventTone::Operator)
    }

    fn runtime(message: impl Into<String>) -> Self {
        Self::new("runtime", message, EventTone::Runtime)
    }

    fn topology(message: impl Into<String>) -> Self {
        Self::new("topology", message, EventTone::Topology)
    }

    fn gateway(message: impl Into<String>) -> Self {
        Self::new("gateway", message, EventTone::Gateway)
    }

    fn chat(message: impl Into<String>) -> Self {
        Self::new("chat", message, EventTone::Chat)
    }

    fn dns(message: impl Into<String>) -> Self {
        Self::new("dns", message, EventTone::Dns)
    }

    fn error(message: impl Into<String>) -> Self {
        Self::new("error", message, EventTone::Error)
    }

    fn new(subsystem: &'static str, message: impl Into<String>, tone: EventTone) -> Self {
        Self {
            timestamp: clock_string(now_unix_ms()),
            subsystem,
            message: message.into(),
            tone,
        }
    }
}

#[derive(Clone, Copy)]
enum EventTone {
    Runtime,
    Topology,
    Gateway,
    Chat,
    Dns,
    Operator,
    Error,
}

impl EventTone {
    fn color(self) -> Color {
        match self {
            EventTone::Runtime => Color::White,
            EventTone::Topology => Color::LightBlue,
            EventTone::Gateway => Color::LightMagenta,
            EventTone::Chat => Color::LightGreen,
            EventTone::Dns => Color::Yellow,
            EventTone::Operator => Color::LightCyan,
            EventTone::Error => Color::LightRed,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum PaletteCommand {
    Sync,
    Quit,
    Open { route: String },
    Resolve { domain: String },
    Join { room: String },
    Leave { room: String },
    Switch { room: String },
    MarkRead { room: Option<String> },
    Direct { peer_id: String, message: String },
    RoomSend { room: String, message: String },
    InspectPeer { peer_id: String },
    InspectGateway { domain: String },
}

fn parse_palette_command(input: &str, current_room: Option<&str>) -> std::result::Result<PaletteCommand, String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err(command_usage());
    }

    match trimmed {
        "open" => return Err("usage: open <route>".to_string()),
        "resolve" => return Err("usage: resolve <domain>".to_string()),
        "join" => return Err("usage: join <room>".to_string()),
        "leave" => return Err("usage: leave <room>".to_string()),
        "switch" => return Err("usage: switch <room>".to_string()),
        "direct" => return Err("usage: direct <peer_id> <message>".to_string()),
        "send" => return Err("usage: send <peer_id> <message>".to_string()),
        "room-send" => {
            return Err("usage: room-send <message> or room-send @<room> <message>".to_string())
        }
        "inspect peer" => return Err("usage: inspect peer <peer_id>".to_string()),
        "inspect gateway" => return Err("usage: inspect gateway <domain>".to_string()),
        _ => {}
    }

    if trimmed == "sync" || trimmed == "sync runtime" || trimmed == "refresh" {
        return Ok(PaletteCommand::Sync);
    }
    if trimmed == "quit" || trimmed == "exit" {
        return Ok(PaletteCommand::Quit);
    }

    if let Some(route) = trimmed.strip_prefix("open ") {
        let route = route.trim();
        if route.is_empty() {
            return Err("usage: open <route>".to_string());
        }
        return Ok(PaletteCommand::Open {
            route: route.to_string(),
        });
    }

    if let Some(domain) = trimmed.strip_prefix("resolve ") {
        let domain = domain.trim();
        if domain.is_empty() {
            return Err("usage: resolve <domain>".to_string());
        }
        return Ok(PaletteCommand::Resolve {
            domain: domain.to_string(),
        });
    }

    if let Some(room) = trimmed.strip_prefix("join ") {
        return parse_single_value(room, "usage: join <room>").map(|room| PaletteCommand::Join { room });
    }
    if let Some(room) = trimmed.strip_prefix("leave ") {
        return parse_single_value(room, "usage: leave <room>").map(|room| PaletteCommand::Leave { room });
    }
    if let Some(room) = trimmed.strip_prefix("switch ") {
        return parse_single_value(room, "usage: switch <room>").map(|room| PaletteCommand::Switch { room });
    }

    if let Some(room) = trimmed.strip_prefix("mark-read") {
        let room = room.trim();
        return Ok(PaletteCommand::MarkRead {
            room: if room.is_empty() { None } else { Some(room.to_string()) },
        });
    }

    if let Some(rest) = trimmed.strip_prefix("direct ") {
        return parse_peer_message(rest, "usage: direct <peer_id> <message>")
            .map(|(peer_id, message)| PaletteCommand::Direct { peer_id, message });
    }
    if let Some(rest) = trimmed.strip_prefix("send ") {
        return parse_peer_message(rest, "usage: send <peer_id> <message>")
            .map(|(peer_id, message)| PaletteCommand::Direct { peer_id, message });
    }

    if let Some(rest) = trimmed.strip_prefix("room-send ") {
        return parse_room_send(rest, current_room);
    }

    if let Some(peer_id) = trimmed.strip_prefix("inspect peer ") {
        return parse_single_value(peer_id, "usage: inspect peer <peer_id>")
            .map(|peer_id| PaletteCommand::InspectPeer { peer_id });
    }
    if let Some(domain) = trimmed.strip_prefix("inspect gateway ") {
        return parse_single_value(domain, "usage: inspect gateway <domain>")
            .map(|domain| PaletteCommand::InspectGateway { domain });
    }

    Err(command_usage())
}

fn parse_single_value(input: &str, usage: &str) -> std::result::Result<String, String> {
    let value = input.trim();
    if value.is_empty() {
        return Err(usage.to_string());
    }
    Ok(value.to_string())
}

fn parse_peer_message(input: &str, usage: &str) -> std::result::Result<(String, String), String> {
    let mut parts = input.trim().splitn(2, ' ');
    let peer_id = parts.next().unwrap_or_default().trim();
    let message = parts.next().unwrap_or_default().trim();
    if peer_id.is_empty() || message.is_empty() {
        return Err(usage.to_string());
    }
    Ok((peer_id.to_string(), message.to_string()))
}

fn parse_room_send(input: &str, current_room: Option<&str>) -> std::result::Result<PaletteCommand, String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err("usage: room-send <message> or room-send @<room> <message>".to_string());
    }

    if let Some(explicit) = trimmed.strip_prefix('@') {
        let mut parts = explicit.splitn(2, ' ');
        let room = parts.next().unwrap_or_default().trim();
        let message = parts.next().unwrap_or_default().trim();
        if room.is_empty() || message.is_empty() {
            return Err("usage: room-send @<room> <message>".to_string());
        }
        return Ok(PaletteCommand::RoomSend {
            room: room.to_string(),
            message: message.to_string(),
        });
    }

    if let Some(current_room) = current_room.filter(|room| !room.is_empty()) {
        return Ok(PaletteCommand::RoomSend {
            room: current_room.to_string(),
            message: trimmed.to_string(),
        });
    }

    let mut parts = trimmed.splitn(2, ' ');
    let room = parts.next().unwrap_or_default().trim();
    let message = parts.next().unwrap_or_default().trim();
    if room.is_empty() || message.is_empty() {
        return Err("usage: room-send <room> <message> or switch <room> first".to_string());
    }

    Ok(PaletteCommand::RoomSend {
        room: room.to_string(),
        message: message.to_string(),
    })
}

fn command_usage() -> String {
    "commands: open <route>, resolve <domain>, join <room>, leave <room>, switch <room>, room-send <message>, room-send @<room> <message>, direct <peer_id> <message>, mark-read [room], inspect peer <peer_id>, inspect gateway <domain>, sync, quit".to_string()
}

fn draw_console(frame: &mut Frame, app: &ConsoleApp) {
    let area = frame.area();
    if is_compact_area(area) {
        draw_compact_console(frame, app, area);
        if app.command_mode {
            draw_palette(frame, app, area);
        }
        return;
    }

    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(4),
            Constraint::Length(3),
            Constraint::Min(10),
            Constraint::Length(2),
        ])
        .split(area);

    draw_header(frame, app, vertical[0]);
    draw_tabs(frame, app, vertical[1]);
    draw_view(frame, app, vertical[2]);
    draw_footer(frame, app, vertical[3]);

    if app.command_mode {
        draw_palette(frame, app, area);
    }
}

fn draw_compact_console(frame: &mut Frame, app: &ConsoleApp, area: Rect) {
    let runtime = app.snapshot.runtime_info();
    let lines = vec![
        Line::from(Span::styled("VOID CONSOLE", title_style())),
        Line::from(Span::styled("terminal too small for full dashboard", subtle_style())),
        Line::from(Span::styled(
            format!("node {}", shorten_peer_id(&app.snapshot.node_id)),
            value_style(),
        )),
        Line::from(Span::styled(
            format!(
                "mesh {} · peers {}",
                app.snapshot.topology.mesh_state,
                count_active_peers(&app.snapshot.topology)
            ),
            subtle_style(),
        )),
        Line::from(Span::styled(
            format!(
                "route {} · room {}",
                app.snapshot.current_mount_route.as_deref().unwrap_or("-"),
                app.snapshot.current_room.as_deref().unwrap_or("-")
            ),
            subtle_style(),
        )),
        Line::from(Span::styled(
            format!(
                "sessions {} · unread {}",
                runtime.map(|state| state.active_sessions).unwrap_or_default(),
                app.snapshot.unread_messages
            ),
            subtle_style(),
        )),
        Line::from(Span::styled(
            format!(
                "status {}",
                truncate_with_ellipsis(&app.status, area.width.saturating_sub(12) as usize)
            ),
            if app.status_is_error {
                error_style()
            } else {
                subtle_style()
            },
        )),
        Line::from(Span::styled("resize terminal or use q / Ctrl+C to exit", subtle_style())),
    ];

    frame.render_widget(
        Paragraph::new(lines)
            .block(panel_block("COMPACT"))
            .wrap(Wrap { trim: true }),
        area,
    );
}

fn draw_header(frame: &mut Frame, app: &ConsoleApp, area: Rect) {
    let runtime = app.snapshot.runtime_info();
    let line_one = Line::from(vec![
        Span::styled(
            format!("node {}", shorten_peer_id(&app.snapshot.node_id)),
            value_style(),
        ),
        Span::raw("  "),
        Span::styled(
            format!("mesh {}", app.snapshot.topology.mesh_state),
            Style::default()
                .fg(mesh_color(app.snapshot.topology.mesh_state))
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("  "),
        Span::styled(
            format!("peers {}", count_active_peers(&app.snapshot.topology)),
            Style::default().fg(Color::LightCyan),
        ),
        Span::raw("  "),
        Span::styled(
            format!("sessions {}", runtime.map(|info| info.active_sessions).unwrap_or_default()),
            Style::default().fg(Color::LightGreen),
        ),
        Span::raw("  "),
        Span::styled(
            format!("unread {}", app.snapshot.unread_messages),
            Style::default().fg(Color::Yellow),
        ),
    ]);
    let line_two = Line::from(vec![
        Span::styled(
            format!(
                "route {}",
                truncate_with_ellipsis(app.snapshot.current_mount_route.as_deref().unwrap_or("-"), 18)
            ),
            subtle_style(),
        ),
        Span::raw("  "),
        Span::styled(
            format!(
                "room {}",
                truncate_with_ellipsis(app.snapshot.current_room.as_deref().unwrap_or("-"), 18)
            ),
            subtle_style(),
        ),
        Span::raw("  "),
        Span::styled(
            format!(
                "error {}",
                truncate_with_ellipsis(app.snapshot.last_error.as_deref().unwrap_or("-"), 22)
            ),
            if app.snapshot.last_error.is_some() {
                error_style()
            } else {
                subtle_style()
            },
        ),
    ]);

    frame.render_widget(
        Paragraph::new(vec![line_one, line_two]).block(panel_block("VOID CONSOLE")),
        area,
    );
}

fn draw_tabs(frame: &mut Frame, app: &ConsoleApp, area: Rect) {
    let titles = TAB_TITLES
        .iter()
        .map(|title| Line::from(Span::raw(format!(" {} ", title))))
        .collect::<Vec<_>>();
    let tabs = Tabs::new(titles)
        .select(app.tab)
        .highlight_style(
            Style::default()
                .fg(Color::Black)
                .bg(Color::LightMagenta)
                .add_modifier(Modifier::BOLD),
        )
        .block(Block::default().borders(Borders::ALL).border_style(border_style()));
    frame.render_widget(tabs, area);
}

fn draw_view(frame: &mut Frame, app: &ConsoleApp, area: Rect) {
    match app.tab {
        0 => draw_dashboard(frame, app, area),
        1 => draw_chat(frame, app, area),
        2 => draw_topology(frame, app, area),
        3 => draw_gateways(frame, app, area),
        _ => draw_events(frame, app, area),
    }
}

fn draw_dashboard(frame: &mut Frame, app: &ConsoleApp, area: Rect) {
    let runtime = app.snapshot.runtime_info();
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(9), Constraint::Min(8)])
        .split(area);

    let top = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(layout[0]);

    let network_lines = vec![
        Line::from(Span::styled("NETWORK", title_style())),
        Line::from(Span::styled(
            format!("mesh {}", app.snapshot.topology.mesh_state),
            value_style(),
        )),
        Line::from(Span::styled(
            format!("active peers {}", count_active_peers(&app.snapshot.topology)),
            subtle_style(),
        )),
        Line::from(Span::styled(
            format!(
                "gateways {} · bridge sessions {}",
                runtime.map(|state| state.gateway_registrations).unwrap_or_default(),
                runtime.map(|state| state.gateway_bridge_sessions).unwrap_or_default(),
            ),
            subtle_style(),
        )),
    ];
    let runtime_lines = vec![
        Line::from(Span::styled("RUNTIME", title_style())),
        Line::from(Span::styled(
            format!(
                "mounted {} · active {}",
                runtime.map(|state| state.mounted_surfaces).unwrap_or_default(),
                runtime.map(|state| state.active_sessions).unwrap_or_default(),
            ),
            value_style(),
        )),
        Line::from(Span::styled(
            format!(
                "permissions {} · routes {}",
                runtime.map(|state| state.active_permissions).unwrap_or_default(),
                runtime.map(|state| state.gateway_active_routes).unwrap_or_default(),
            ),
            subtle_style(),
        )),
        Line::from(Span::styled(
            format!("route {}", app.snapshot.current_mount_route.as_deref().unwrap_or("-")),
            subtle_style(),
        )),
    ];
    frame.render_widget(Paragraph::new(network_lines).block(panel_block("TOPOLOGY STATE")), top[0]);
    frame.render_widget(Paragraph::new(runtime_lines).block(panel_block("RUNTIME STATE")), top[1]);

    let bottom = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(40), Constraint::Percentage(30), Constraint::Percentage(30)])
        .split(layout[1]);

    let chat_lines = vec![
        Line::from(Span::styled("CHAT", title_style())),
        Line::from(Span::styled(
            format!("room {}", app.snapshot.current_room.as_deref().unwrap_or("-")),
            value_style(),
        )),
        Line::from(Span::styled(
            format!(
                "unread {} · notifications {}",
                app.snapshot.unread_messages, app.snapshot.notification_count
            ),
            subtle_style(),
        )),
        Line::from(Span::styled(
            format!("rooms {}", app.snapshot.rooms.len()),
            subtle_style(),
        )),
    ];
    frame.render_widget(Paragraph::new(chat_lines).block(panel_block("CHAT STATE")), bottom[0]);
    frame.render_widget(
        Paragraph::new(lines_from_kv(&app.snapshot.gateway_lines))
            .block(panel_block("GATEWAY"))
            .wrap(Wrap { trim: true }),
        bottom[1],
    );
    frame.render_widget(
        Paragraph::new(lines_from_kv(&app.snapshot.chat_lines))
            .block(panel_block("DIAGNOSTICS"))
            .wrap(Wrap { trim: true }),
        bottom[2],
    );
}

fn draw_chat(frame: &mut Frame, app: &ConsoleApp, area: Rect) {
    let layout = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(28), Constraint::Percentage(42), Constraint::Min(24)])
        .split(area);

    let room_items = if app.snapshot.rooms.is_empty() {
        vec![ListItem::new(Line::from(Span::styled("no rooms observed", subtle_style())))]
    } else {
        app.snapshot
            .rooms
            .iter()
            .map(|room| {
                let marker = if room.is_current {
                    "▶"
                } else if room.joined {
                    "●"
                } else {
                    "○"
                };
                ListItem::new(vec![
                    Line::from(Span::styled(format!("{} {}", marker, room.name), value_style())),
                    Line::from(Span::styled(
                        format!("{} active · {} events", room.active_members, room.events),
                        subtle_style(),
                    )),
                ])
            })
            .collect::<Vec<_>>()
    };

    let inbox_items = if app.snapshot.inbox.is_empty() {
        vec![ListItem::new(Line::from(Span::styled("inbox empty", subtle_style())))]
    } else {
        app.snapshot
            .inbox
            .iter()
            .map(|message| {
                ListItem::new(vec![
                    Line::from(vec![
                        Span::styled(shorten_peer_id(&message.from), value_style()),
                        Span::raw(" "),
                        Span::styled(message.room.clone(), subtle_style()),
                        Span::raw(" "),
                        Span::styled(clock_string(message.received_at_unix_ms), subtle_style()),
                    ]),
                    Line::from(Span::styled(
                        truncate_with_ellipsis(&message.body, 48),
                        if message.unread {
                            Style::default().fg(Color::White)
                        } else {
                            subtle_style()
                        },
                    )),
                ])
            })
            .collect::<Vec<_>>()
    };

    let details = chat_detail_lines(app);

    frame.render_widget(List::new(room_items).block(panel_block("ROOMS")), layout[0]);
    frame.render_widget(List::new(inbox_items).block(panel_block("INBOX")), layout[1]);
    frame.render_widget(
        Paragraph::new(details)
            .block(panel_block("CHAT OPERATIONS"))
            .wrap(Wrap { trim: true }),
        layout[2],
    );
}

fn draw_topology(frame: &mut Frame, app: &ConsoleApp, area: Rect) {
    if app.snapshot.topology.peers.is_empty() {
        frame.render_widget(
            Paragraph::new(vec![
                Line::from(Span::styled("no peers observed", value_style())),
                Line::from(Span::styled(
                    "topology state is persisted from the transport runtime",
                    subtle_style(),
                )),
            ])
            .block(panel_block("TOPOLOGY")),
            area,
        );
        return;
    }

    let rows = app
        .snapshot
        .topology
        .peers
        .values()
        .map(|peer| {
            Row::new(vec![
                Cell::from(shorten_peer_id(&peer.peer_id)),
                Cell::from(peer.state.to_string()),
                Cell::from(peer.transport_health.to_string()),
                Cell::from(peer.session.encryption_state.clone()),
                Cell::from(
                    peer.latency_ms
                        .map(|value| format!("{}ms", value))
                        .unwrap_or_else(|| "-".to_string()),
                ),
            ])
        })
        .collect::<Vec<_>>();
    let table = Table::new(
        rows,
        [
            Constraint::Length(20),
            Constraint::Length(16),
            Constraint::Length(14),
            Constraint::Length(16),
            Constraint::Length(10),
        ],
    )
    .header(
        Row::new(vec!["Peer", "State", "Health", "Session", "Latency"])
            .style(Style::default().fg(Color::LightMagenta).add_modifier(Modifier::BOLD)),
    )
    .block(panel_block("TOPOLOGY"));
    frame.render_widget(table, area);
}

fn draw_gateways(frame: &mut Frame, app: &ConsoleApp, area: Rect) {
    let runtime_state = app.snapshot.runtime_state.as_ref();
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(8), Constraint::Length(10)])
        .split(area);

    if let Some(state) = runtime_state {
        let rows = state
            .gateway_routes
            .iter()
            .map(|route| {
                Row::new(vec![
                    Cell::from(route.route.clone()),
                    Cell::from(format!("{:?}", route.trust_state)),
                    Cell::from(truncate_with_ellipsis(&route.bridge.external_target, 28)),
                    Cell::from(
                        route.bridge
                            .fetch_latency_ms
                            .map(|value| format!("{}ms", value))
                            .unwrap_or_else(|| "-".to_string()),
                    ),
                ])
            })
            .collect::<Vec<_>>();

        if rows.is_empty() {
            frame.render_widget(
                Paragraph::new(vec![
                    Line::from(Span::styled("no gateway routes observed", value_style())),
                    Line::from(Span::styled(
                        "gateway diagnostics still reflect persisted runtime state",
                        subtle_style(),
                    )),
                ])
                .block(panel_block("GATEWAYS")),
                layout[0],
            );
        } else {
            let table = Table::new(
                rows,
                [
                    Constraint::Length(24),
                    Constraint::Length(14),
                    Constraint::Percentage(50),
                    Constraint::Length(10),
                ],
            )
            .header(
                Row::new(vec!["Route", "Trust", "External Target", "Latency"])
                    .style(Style::default().fg(Color::LightMagenta).add_modifier(Modifier::BOLD)),
            )
            .block(panel_block("GATEWAYS"));
            frame.render_widget(table, layout[0]);
        }
    } else {
        frame.render_widget(
            Paragraph::new(vec![
                Line::from(Span::styled("runtime shell state unavailable", value_style())),
                Line::from(Span::styled(
                    "start the node and mount a gateway surface to populate this view",
                    subtle_style(),
                )),
            ])
            .block(panel_block("GATEWAYS"))
            .wrap(Wrap { trim: true }),
            layout[0],
        );
    }

    frame.render_widget(
        Paragraph::new(lines_from_kv(&app.snapshot.gateway_lines))
            .block(panel_block("GATEWAY DIAGNOSTICS"))
            .wrap(Wrap { trim: true }),
        layout[1],
    );
}

fn draw_events(frame: &mut Frame, app: &ConsoleApp, area: Rect) {
    let mut lines = if app.events.is_empty() {
        vec![Line::from(Span::styled("no runtime events observed", subtle_style()))]
    } else {
        app.events
            .iter()
            .map(|event| {
                Line::from(vec![
                    Span::styled(format!("{} ", event.timestamp), subtle_style()),
                    Span::styled(
                        format!("[{}] ", event.subsystem.to_uppercase()),
                        Style::default().fg(event.tone.color()).add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(event.message.clone(), Style::default().fg(Color::White)),
                ])
            })
            .collect::<Vec<_>>()
    };

    lines.insert(
        0,
        Line::from(Span::styled(
            format!(
                "latest first · scrollback {} of {}",
                app.event_scroll,
                app.events.len().saturating_sub(1)
            ),
            subtle_style(),
        )),
    );

    frame.render_widget(
        Paragraph::new(lines)
            .block(panel_block("EVENT STREAM"))
            .scroll((app.event_scroll.min(u16::MAX as usize) as u16, 0))
            .wrap(Wrap { trim: false }),
        area,
    );
}

fn draw_footer(frame: &mut Frame, app: &ConsoleApp, area: Rect) {
    let content = if app.command_mode {
        format!(":{}", app.command_input)
    } else if app.status.is_empty() {
        "Tab/Shift+Tab switch views · : palette · r refresh · q quit".to_string()
    } else {
        app.status.clone()
    };

    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            truncate_with_ellipsis(&content, area.width.saturating_sub(2) as usize),
            if app.status_is_error {
                error_style()
            } else {
                subtle_style()
            },
        )))
        .alignment(Alignment::Left),
        area,
    );
}

fn draw_palette(frame: &mut Frame, app: &ConsoleApp, area: Rect) {
    let popup_area = centered_rect(76, 22, area);
    frame.render_widget(Clear, popup_area);
    let popup = Paragraph::new(vec![
        Line::from(Span::styled("VOID COMMAND PALETTE", title_style())),
        Line::from(""),
        Line::from(vec![
            Span::styled(":", title_style()),
            Span::styled(app.command_input.clone(), value_style()),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            "open <route> · resolve <domain> · join <room> · leave <room>",
            subtle_style(),
        )),
        Line::from(Span::styled(
            "switch <room> · room-send <message> · room-send @<room> <message>",
            subtle_style(),
        )),
        Line::from(Span::styled(
            "direct <peer_id> <message> · mark-read [room] · sync · quit",
            subtle_style(),
        )),
        Line::from(Span::styled(
            "inspect peer <peer_id> · inspect gateway <domain>",
            subtle_style(),
        )),
        Line::from(""),
        Line::from(Span::styled("Esc closes the palette", subtle_style())),
    ])
    .block(panel_block("COMMANDS"))
    .wrap(Wrap { trim: true });
    frame.render_widget(popup, popup_area);
}

fn chat_detail_lines(app: &ConsoleApp) -> Vec<Line<'static>> {
    let mut lines = vec![
        Line::from(Span::styled("CHAT", title_style())),
        Line::from(Span::styled(
            format!("current room {}", app.snapshot.current_room.as_deref().unwrap_or("-")),
            value_style(),
        )),
        Line::from(Span::styled(
            format!(
                "unread {} · notifications {}",
                app.snapshot.unread_messages, app.snapshot.notification_count
            ),
            subtle_style(),
        )),
    ];

    if let Some(room) = &app.snapshot.selected_room {
        lines.push(Line::from(Span::styled(
            format!("focus {} · joined {} · active {}", room.name, yes_no(room.joined), room.active_members),
            subtle_style(),
        )));
        lines.push(Line::from(Span::styled("members", title_style())));
        if room.members.is_empty() {
            lines.push(Line::from(Span::styled("no members observed", subtle_style())));
        } else {
            for member in &room.members {
                lines.push(Line::from(Span::styled(member.clone(), subtle_style())));
            }
        }
        lines.push(Line::from(Span::styled("recent room events", title_style())));
        if room.recent_events.is_empty() {
            lines.push(Line::from(Span::styled("no room events observed", subtle_style())));
        } else {
            for event in &room.recent_events {
                lines.push(Line::from(Span::styled(event.clone(), subtle_style())));
            }
        }
    } else {
        lines.push(Line::from(Span::styled("no room context available", subtle_style())));
    }

    lines.push(Line::from(Span::styled("sessions", title_style())));
    if app.snapshot.chat_sessions.is_empty() {
        lines.push(Line::from(Span::styled("no chat sessions observed", subtle_style())));
    } else {
        for session in &app.snapshot.chat_sessions {
            lines.push(Line::from(Span::styled(session.clone(), subtle_style())));
        }
    }

    lines.push(Line::from(Span::styled("notifications", title_style())));
    if app.snapshot.notifications.is_empty() {
        lines.push(Line::from(Span::styled("no unread notifications", subtle_style())));
    } else {
        for notification in &app.snapshot.notifications {
            lines.push(Line::from(Span::styled(notification.clone(), subtle_style())));
        }
    }

    lines
}

fn lines_from_kv(lines: &[String]) -> Vec<Line<'static>> {
    if lines.is_empty() {
        return vec![Line::from(Span::styled("no runtime diagnostics observed", subtle_style()))];
    }
    lines
        .iter()
        .map(|line| Line::from(Span::styled(truncate_with_ellipsis(line, 44), subtle_style())))
        .collect::<Vec<_>>()
}

fn panel_block(title: &str) -> Block<'static> {
    Block::default()
        .title(Span::styled(format!(" {} ", title), title_style()))
        .borders(Borders::ALL)
        .border_style(border_style())
}

fn title_style() -> Style {
    Style::default().fg(Color::LightMagenta).add_modifier(Modifier::BOLD)
}

fn value_style() -> Style {
    Style::default().fg(Color::White).add_modifier(Modifier::BOLD)
}

fn subtle_style() -> Style {
    Style::default().fg(Color::Gray)
}

fn error_style() -> Style {
    Style::default().fg(Color::LightRed).add_modifier(Modifier::BOLD)
}

fn border_style() -> Style {
    Style::default().fg(Color::DarkGray)
}

fn mesh_color(state: MeshState) -> Color {
    match state {
        MeshState::Stable => Color::LightGreen,
        MeshState::Recovering => Color::Yellow,
        MeshState::Degraded => Color::LightYellow,
        MeshState::Partitioned => Color::LightRed,
        MeshState::Bootstrapping => Color::LightBlue,
    }
}

fn centered_rect(width_percent: u16, height_percent: u16, area: Rect) -> Rect {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - height_percent) / 2),
            Constraint::Percentage(height_percent),
            Constraint::Percentage((100 - height_percent) / 2),
        ])
        .split(area);
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - width_percent) / 2),
            Constraint::Percentage(width_percent),
            Constraint::Percentage((100 - width_percent) / 2),
        ])
        .split(vertical[1])[1]
}

fn is_compact_area(area: Rect) -> bool {
    area.width < COMPACT_WIDTH || area.height < COMPACT_HEIGHT
}

fn count_active_peers(topology: &PeerTopology) -> usize {
    topology
        .peers
        .values()
        .filter(|peer| matches!(peer.state, PeerConnectionState::Active | PeerConnectionState::Syncing))
        .count()
}

fn yes_no(value: bool) -> &'static str {
    if value {
        "yes"
    } else {
        "no"
    }
}

fn shorten_peer_id(peer_id: &str) -> String {
    if peer_id.len() <= 18 {
        peer_id.to_string()
    } else {
        format!("{}...{}", &peer_id[..8], &peer_id[peer_id.len() - 6..])
    }
}

fn truncate_with_ellipsis(input: &str, max_width: usize) -> String {
    if max_width == 0 {
        return String::new();
    }
    let char_count = input.chars().count();
    if char_count <= max_width {
        return input.to_string();
    }
    if max_width <= 3 {
        return ".".repeat(max_width);
    }
    let keep = max_width - 3;
    format!("{}...", input.chars().take(keep).collect::<String>())
}

fn clock_string(unix_ms: u128) -> String {
    let secs = (unix_ms / 1000) as u64;
    let seconds_in_day = secs % 86_400;
    let hours = seconds_in_day / 3_600;
    let minutes = (seconds_in_day % 3_600) / 60;
    let seconds = seconds_in_day % 60;
    format!("{:02}:{:02}:{:02}", hours, minutes, seconds)
}

fn load_recent_event_log(data_dir: &Path, limit: usize) -> Result<Vec<ConsoleEvent>> {
    let path = data_dir.join("events.log");
    if !path.exists() {
        return Ok(Vec::new());
    }

    let content = fs::read_to_string(&path)
        .with_context(|| format!("failed to read event log at {}", path.display()))?;
    let mut lines = content
        .lines()
        .rev()
        .take(limit)
        .filter_map(parse_persisted_event_line)
        .collect::<Vec<_>>();
    lines.reverse();
    Ok(collapse_repeated_events(lines))
}

fn parse_persisted_event_line(line: &str) -> Option<ConsoleEvent> {
    let (timestamp_raw, payload) = line.split_once('|')?;
    let timestamp_unix_ms = timestamp_raw.parse::<u128>().ok()?;
    let subsystem = payload
        .strip_prefix("[VOIDNET][")
        .and_then(|rest| rest.split_once(']'))
        .map(|(tag, _)| tag)
        .unwrap_or("EVENT");
    let message = payload
        .strip_prefix(&format!("[VOIDNET][{}] ", subsystem))
        .unwrap_or(payload)
        .to_string();
    Some(ConsoleEvent {
        timestamp: clock_string(timestamp_unix_ms),
        subsystem: match subsystem {
            "CHAT" => "chat",
            "RUNTIME" => "runtime",
            "TRANSPORT" => "transport",
            "MESH" => "topology",
            "DNS" => "dns",
            "LIFECYCLE" => "lifecycle",
            "GATEWAY" => "gateway",
            _ => "event",
        },
        message,
        tone: match subsystem {
            "CHAT" => EventTone::Chat,
            "RUNTIME" => EventTone::Runtime,
            "TRANSPORT" => EventTone::Gateway,
            "MESH" => EventTone::Topology,
            "DNS" => EventTone::Dns,
            "LIFECYCLE" => EventTone::Operator,
            "GATEWAY" => EventTone::Gateway,
            _ => EventTone::Operator,
        },
    })
}

fn collapse_repeated_events(events: Vec<ConsoleEvent>) -> Vec<ConsoleEvent> {
    let mut collapsed: Vec<ConsoleEvent> = Vec::new();

    for event in events {
        if let Some(previous) = collapsed.last_mut() {
            if previous.subsystem == event.subsystem
                && repeat_base_message(&previous.message) == event.message
            {
                if let Some((base, count)) = parse_repeat_suffix(&previous.message) {
                    previous.message = format!("{} (x{})", base, count + 1);
                } else {
                    previous.message = format!("{} (x2)", previous.message);
                }
                previous.timestamp = event.timestamp;
                continue;
            }
        }
        collapsed.push(event);
    }

    collapsed
}

fn parse_repeat_suffix(message: &str) -> Option<(&str, usize)> {
    let (base, suffix) = message.rsplit_once(" (x")?;
    let count = suffix.strip_suffix(')')?.parse::<usize>().ok()?;
    Some((base, count))
}

fn repeat_base_message(message: &str) -> &str {
    parse_repeat_suffix(message).map(|(base, _)| base).unwrap_or(message)
}

fn load_runtime_shell_state(data_dir: &PathBuf) -> Result<Option<RuntimeShellState>> {
    let shell_state_path = data_dir.join("runtime").join("shell.json");
    if !shell_state_path.exists() {
        return Ok(None);
    }
    let bytes = std::fs::read(&shell_state_path)
        .with_context(|| format!("failed to read runtime shell state at {}", shell_state_path.display()))?;
    let state = serde_json::from_slice(&bytes)?;
    Ok(Some(state))
}

fn resolve_domain(data_dir: &PathBuf, raw_domain: &str) -> Result<String> {
    let dns = PersistentVoidDns::load_or_create(data_dir)?;
    let authority = if raw_domain.trim().starts_with("void://") {
        parse_void_uri_input(raw_domain)?.authority().to_string()
    } else {
        raw_domain.trim().to_string()
    };
    let domain = VoidDomain::new(authority)?;
    let result = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?
        .block_on(dns.resolve(&domain))?;
    Ok(match result {
        Some(record) => format!("resolved {} -> {}", record.domain, shorten_peer_id(&record.target_peer_id)),
        None => format!("route {} not found", domain),
    })
}

fn open_runtime_route(data_dir: &PathBuf, raw_route: &str) -> Result<String> {
    let uri = parse_void_uri_input(raw_route)?;
    let persistent_identity = PersistentNodeIdentity::load_or_create_dir(data_dir)
        .with_context(|| format!("failed to load persistent identity in {}", data_dir.display()))?;
    let dns = Arc::new(
        PersistentVoidDns::load_or_create(data_dir)
            .with_context(|| format!("failed to load dns cache in {}", data_dir.display()))?,
    );
    let (network, _inbox) = network_channels(16);
    let runtime = VoidRuntime::new(NodeIdentity::generate(), dns.clone(), network, RuntimeConfig::default());
    let mut shell = RuntimeShell::load_or_create(data_dir, runtime)?;
    shell.reconcile_registry_owner(&persistent_identity.peer_id_string())?;
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?
        .block_on(shell.synchronize_registry_dns(&persistent_identity))?;
    let result = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?
        .block_on(shell.open_uri(uri.clone()))?;
    persist_runtime_shell_topology(data_dir, shell.state())?;
    Ok(format!(
        "opened {} -> {}",
        uri,
        truncate_with_ellipsis(&result.mount.surface_id, 18)
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_open_and_resolve_commands() {
        assert_eq!(
            parse_palette_command("open chat.void", None).unwrap(),
            PaletteCommand::Open {
                route: "chat.void".to_string(),
            }
        );
        assert_eq!(
            parse_palette_command("resolve void://chat.void", None).unwrap(),
            PaletteCommand::Resolve {
                domain: "void://chat.void".to_string(),
            }
        );
    }

    #[test]
    fn parses_direct_and_room_send_commands() {
        assert_eq!(
            parse_palette_command("direct 12D3K hello mesh", None).unwrap(),
            PaletteCommand::Direct {
                peer_id: "12D3K".to_string(),
                message: "hello mesh".to_string(),
            }
        );
        assert_eq!(
            parse_palette_command("room-send hello operators", Some("operators")).unwrap(),
            PaletteCommand::RoomSend {
                room: "operators".to_string(),
                message: "hello operators".to_string(),
            }
        );
        assert_eq!(
            parse_palette_command("room-send @mesh hello cluster", None).unwrap(),
            PaletteCommand::RoomSend {
                room: "mesh".to_string(),
                message: "hello cluster".to_string(),
            }
        );
    }

    #[test]
    fn rejects_malformed_palette_commands() {
        assert_eq!(
            parse_palette_command("direct onlypeer", None).unwrap_err(),
            "usage: direct <peer_id> <message>"
        );
        assert_eq!(
            parse_palette_command("room-send", None).unwrap_err(),
            "usage: room-send <message> or room-send @<room> <message>"
        );
        assert!(parse_palette_command("nonsense", None).is_err());
    }

    #[test]
    fn compact_layout_triggers_for_small_terminals() {
        assert!(is_compact_area(Rect::new(0, 0, 60, 14)));
        assert!(!is_compact_area(Rect::new(0, 0, 100, 28)));
    }

    #[test]
    fn ellipsis_truncates_long_strings() {
        assert_eq!(truncate_with_ellipsis("operators-room", 8), "opera...");
        assert_eq!(truncate_with_ellipsis("void", 8), "void");
    }

    #[test]
    fn repeated_events_are_collapsed() {
        let events = vec![
            ConsoleEvent::chat("same"),
            ConsoleEvent::chat("same"),
            ConsoleEvent::runtime("other"),
            ConsoleEvent::runtime("other"),
            ConsoleEvent::runtime("other"),
        ];
        let collapsed = collapse_repeated_events(events);
        assert_eq!(collapsed.len(), 2);
        assert_eq!(collapsed[0].message, "same (x2)");
        assert_eq!(collapsed[1].message, "other (x3)");
    }
}
