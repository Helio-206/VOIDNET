use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce,
};
use base64::{engine::general_purpose::STANDARD_NO_PAD, Engine as _};
use rand_core::{OsRng, RngCore};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs, io,
    path::{Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use thiserror::Error;
use void_identity::{IdentityError, PersistentNodeIdentity};
use void_protocol::{ContentType, Frame, MessageKind, PeerMessage};
use x25519_dalek::{EphemeralSecret, PublicKey as X25519PublicKey};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoomId(String);

impl RoomId {
    pub fn new(raw: impl Into<String>) -> Self {
        Self(raw.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EncryptedPayload {
    pub algorithm: String,
    pub nonce: Vec<u8>,
    pub ciphertext: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatMessage {
    pub room: RoomId,
    pub from: String,
    pub payload: EncryptedPayload,
}

impl ChatMessage {
    pub fn into_frame(self, to: impl Into<String>) -> Frame {
        Frame::PeerMessage(PeerMessage {
            from: self.from,
            to: to.into(),
            kind: MessageKind::Room {
                room_id: self.room.as_str().to_string(),
            },
            content_type: ContentType::Binary,
            payload: self.payload.ciphertext,
        })
    }
}

pub const CHAT_PROTOCOL_VERSION: &str = "voidchat/0.1.0";
pub const CHAT_DIRECT_TOPIC_PREFIX: &str = "void.chat.peer.";
pub const CHAT_ROOM_TOPIC_PREFIX: &str = "void.chat.room.";
pub const CHAT_REPLAY_WINDOW_SECS: u64 = 120;
const CHAT_DIR: &str = "chat";
const CHAT_COMMANDS_DIR: &str = "commands";
const CHAT_INBOX_FILE: &str = "inbox.json";
const CHAT_ROOMS_FILE: &str = "rooms.json";
const CHAT_SESSIONS_FILE: &str = "sessions.json";
const CHAT_NOTIFICATIONS_FILE: &str = "notifications.json";
const ROOM_EVENT_HISTORY_LIMIT: usize = 48;
const CHAT_NOTIFICATION_LIMIT: usize = 64;

pub fn direct_topic(peer_id: &str) -> String {
    format!("{CHAT_DIRECT_TOPIC_PREFIX}{peer_id}")
}

pub fn room_topic(room: &str) -> String {
    format!("{CHAT_ROOM_TOPIC_PREFIX}{room}")
}

pub fn is_room_topic(topic: &str) -> bool {
    topic.starts_with(CHAT_ROOM_TOPIC_PREFIX)
}

pub fn room_from_topic(topic: &str) -> Option<String> {
    topic
        .strip_prefix(CHAT_ROOM_TOPIC_PREFIX)
        .map(ToString::to_string)
}

pub fn direct_peer_from_topic(topic: &str) -> Option<String> {
    topic
        .strip_prefix(CHAT_DIRECT_TOPIC_PREFIX)
        .map(ToString::to_string)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChatLocalCommand {
    SendDirect { peer_id: String, message: String },
    SendRoom { room: String, message: String },
    Join { room: String },
    Leave { room: String },
    SwitchRoom { room: String },
    MarkRead { room: Option<String> },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QueuedChatCommand {
    pub command_id: String,
    pub issued_at_unix_ms: u128,
    pub command: ChatLocalCommand,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChatMessageType {
    DirectText,
    RoomText,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatTextPayload {
    #[serde(default)]
    pub room: Option<String>,
    pub body: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatInboxEntry {
    pub message_id: String,
    pub from_peer_id: String,
    pub room: Option<String>,
    pub body: String,
    pub received_at_unix_ms: u128,
    pub session_id: String,
    pub signature_verified: bool,
    #[serde(default)]
    pub unread: bool,
    #[serde(default)]
    pub room_name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ChatInboxState {
    pub messages: Vec<ChatInboxEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatNotification {
    pub notification_id: String,
    pub kind: String,
    pub message: String,
    #[serde(default)]
    pub room: Option<String>,
    #[serde(default)]
    pub peer_id: Option<String>,
    pub created_at_unix_ms: u128,
    #[serde(default)]
    pub unread: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ChatNotificationsState {
    pub notifications: Vec<ChatNotification>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChatSessionState {
    Idle,
    Negotiating,
    Established,
    Failed,
}

impl std::fmt::Display for ChatSessionState {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            ChatSessionState::Idle => "IDLE",
            ChatSessionState::Negotiating => "NEGOTIATING",
            ChatSessionState::Established => "ESTABLISHED",
            ChatSessionState::Failed => "FAILED",
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatSessionSnapshot {
    pub peer_id: String,
    pub session_id: String,
    pub established_at_unix_ms: u128,
    pub encryption_state: ChatSessionState,
    pub last_activity_unix_ms: u128,
    pub transport_state: String,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ChatSessionsState {
    pub sessions: Vec<ChatSessionSnapshot>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoomMember {
    pub peer_id: String,
    pub presence: String,
    pub last_seen_unix_ms: u128,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatRoomEvent {
    pub event_id: String,
    pub event_type: String,
    pub peer_id: String,
    #[serde(default)]
    pub body: Option<String>,
    pub timestamp_unix_ms: u128,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatRoomSnapshot {
    pub room: String,
    pub joined: bool,
    pub members: Vec<RoomMember>,
    #[serde(default)]
    pub room_id: String,
    #[serde(default)]
    pub room_name: String,
    #[serde(default)]
    pub active_members: usize,
    #[serde(default)]
    pub event_history: Vec<ChatRoomEvent>,
    pub last_changed_unix_ms: u128,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ChatRoomsState {
    pub rooms: Vec<ChatRoomSnapshot>,
    #[serde(default)]
    pub current_room: Option<String>,
    #[serde(default)]
    pub sync_revision: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionOffer {
    pub protocol_version: String,
    pub session_id: String,
    pub sender_peer_id: String,
    pub recipient_peer_id: String,
    pub sender_public_key: String,
    pub ephemeral_public_key: String,
    pub timestamp_unix_ms: u128,
    pub nonce: String,
    pub signature: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionAck {
    pub protocol_version: String,
    pub session_id: String,
    pub sender_peer_id: String,
    pub recipient_peer_id: String,
    pub sender_public_key: String,
    pub ephemeral_public_key: String,
    pub timestamp_unix_ms: u128,
    pub nonce: String,
    pub signature: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SignedEncryptedEnvelope {
    pub protocol_version: String,
    pub sender_peer_id: String,
    pub recipient_peer_id: String,
    pub sender_public_key: String,
    pub session_id: String,
    pub timestamp_unix_ms: u128,
    pub nonce: String,
    pub message_type: ChatMessageType,
    pub encrypted_payload: String,
    pub signature: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RoomMembershipAction {
    Join,
    Leave,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoomMembershipEvent {
    pub protocol_version: String,
    pub room: String,
    pub peer_id: String,
    pub sender_public_key: String,
    pub action: RoomMembershipAction,
    pub timestamp_unix_ms: u128,
    pub nonce: String,
    pub signature: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoomStateSnapshot {
    pub protocol_version: String,
    pub room: String,
    pub peer_id: String,
    pub sender_public_key: String,
    pub members: Vec<RoomMember>,
    pub recent_events: Vec<ChatRoomEvent>,
    pub timestamp_unix_ms: u128,
    pub nonce: String,
    pub signature: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChatWireMessage {
    SessionOffer(SessionOffer),
    SessionAck(SessionAck),
    DirectMessage(SignedEncryptedEnvelope),
    RoomMembership(RoomMembershipEvent),
    RoomStateSnapshot(RoomStateSnapshot),
}

#[derive(Debug, Clone)]
pub struct ReplayProtector {
    window_ms: u128,
    seen: BTreeMap<String, u128>,
}

impl ReplayProtector {
    pub fn new(window: Duration) -> Self {
        Self {
            window_ms: window.as_millis(),
            seen: BTreeMap::new(),
        }
    }

    pub fn check_and_record(
        &mut self,
        scope: &str,
        nonce: &str,
        timestamp_unix_ms: u128,
    ) -> Result<(), ChatError> {
        let now = unix_millis();
        let lower = now.saturating_sub(self.window_ms);
        let upper = now.saturating_add(self.window_ms);
        self.seen.retain(|_, seen_at| *seen_at >= lower);

        if timestamp_unix_ms < lower || timestamp_unix_ms > upper {
            return Err(ChatError::ReplayWindowExpired);
        }

        let key = format!("{scope}:{nonce}");
        if self.seen.contains_key(&key) {
            return Err(ChatError::ReplayDetected);
        }

        self.seen.insert(key, timestamp_unix_ms);
        Ok(())
    }
}

impl SessionOffer {
    pub fn new(
        identity: &PersistentNodeIdentity,
        recipient_peer_id: impl Into<String>,
        ephemeral_public_key: [u8; 32],
    ) -> Result<Self, ChatError> {
        let recipient_peer_id = recipient_peer_id.into();
        let unsigned = UnsignedSessionOffer {
            protocol_version: CHAT_PROTOCOL_VERSION.to_string(),
            session_id: generate_session_id(),
            sender_peer_id: identity.peer_id_string(),
            recipient_peer_id,
            sender_public_key: identity.public_key_protobuf_base64(),
            ephemeral_public_key: STANDARD_NO_PAD.encode(ephemeral_public_key),
            timestamp_unix_ms: unix_millis(),
            nonce: random_token(16),
        };
        let signature =
            STANDARD_NO_PAD.encode(identity.sign_bytes(&serde_json::to_vec(&unsigned)?)?);
        Ok(Self {
            protocol_version: unsigned.protocol_version,
            session_id: unsigned.session_id,
            sender_peer_id: unsigned.sender_peer_id,
            recipient_peer_id: unsigned.recipient_peer_id,
            sender_public_key: unsigned.sender_public_key,
            ephemeral_public_key: unsigned.ephemeral_public_key,
            timestamp_unix_ms: unsigned.timestamp_unix_ms,
            nonce: unsigned.nonce,
            signature,
        })
    }

    pub fn verify(&self) -> Result<(), ChatError> {
        let unsigned = UnsignedSessionOffer {
            protocol_version: self.protocol_version.clone(),
            session_id: self.session_id.clone(),
            sender_peer_id: self.sender_peer_id.clone(),
            recipient_peer_id: self.recipient_peer_id.clone(),
            sender_public_key: self.sender_public_key.clone(),
            ephemeral_public_key: self.ephemeral_public_key.clone(),
            timestamp_unix_ms: self.timestamp_unix_ms,
            nonce: self.nonce.clone(),
        };
        PersistentNodeIdentity::verify_bytes(
            &self.sender_peer_id,
            &self.sender_public_key,
            &serde_json::to_vec(&unsigned)?,
            &self.signature,
        )?;
        Ok(())
    }

    pub fn peer_public_key(&self) -> Result<X25519PublicKey, ChatError> {
        decode_x25519_public_key(&self.ephemeral_public_key)
    }
}

impl SessionAck {
    pub fn new(
        identity: &PersistentNodeIdentity,
        recipient_peer_id: impl Into<String>,
        session_id: impl Into<String>,
        ephemeral_public_key: [u8; 32],
    ) -> Result<Self, ChatError> {
        let unsigned = UnsignedSessionAck {
            protocol_version: CHAT_PROTOCOL_VERSION.to_string(),
            session_id: session_id.into(),
            sender_peer_id: identity.peer_id_string(),
            recipient_peer_id: recipient_peer_id.into(),
            sender_public_key: identity.public_key_protobuf_base64(),
            ephemeral_public_key: STANDARD_NO_PAD.encode(ephemeral_public_key),
            timestamp_unix_ms: unix_millis(),
            nonce: random_token(16),
        };
        let signature =
            STANDARD_NO_PAD.encode(identity.sign_bytes(&serde_json::to_vec(&unsigned)?)?);
        Ok(Self {
            protocol_version: unsigned.protocol_version,
            session_id: unsigned.session_id,
            sender_peer_id: unsigned.sender_peer_id,
            recipient_peer_id: unsigned.recipient_peer_id,
            sender_public_key: unsigned.sender_public_key,
            ephemeral_public_key: unsigned.ephemeral_public_key,
            timestamp_unix_ms: unsigned.timestamp_unix_ms,
            nonce: unsigned.nonce,
            signature,
        })
    }

    pub fn verify(&self) -> Result<(), ChatError> {
        let unsigned = UnsignedSessionAck {
            protocol_version: self.protocol_version.clone(),
            session_id: self.session_id.clone(),
            sender_peer_id: self.sender_peer_id.clone(),
            recipient_peer_id: self.recipient_peer_id.clone(),
            sender_public_key: self.sender_public_key.clone(),
            ephemeral_public_key: self.ephemeral_public_key.clone(),
            timestamp_unix_ms: self.timestamp_unix_ms,
            nonce: self.nonce.clone(),
        };
        PersistentNodeIdentity::verify_bytes(
            &self.sender_peer_id,
            &self.sender_public_key,
            &serde_json::to_vec(&unsigned)?,
            &self.signature,
        )?;
        Ok(())
    }

    pub fn peer_public_key(&self) -> Result<X25519PublicKey, ChatError> {
        decode_x25519_public_key(&self.ephemeral_public_key)
    }
}

impl SignedEncryptedEnvelope {
    pub fn new(
        identity: &PersistentNodeIdentity,
        recipient_peer_id: impl Into<String>,
        session_id: impl Into<String>,
        message_type: ChatMessageType,
        payload: &[u8],
    ) -> Result<Self, ChatError> {
        let encrypted_payload = STANDARD_NO_PAD.encode(payload);
        let unsigned = UnsignedEncryptedEnvelope {
            protocol_version: CHAT_PROTOCOL_VERSION.to_string(),
            sender_peer_id: identity.peer_id_string(),
            recipient_peer_id: recipient_peer_id.into(),
            sender_public_key: identity.public_key_protobuf_base64(),
            session_id: session_id.into(),
            timestamp_unix_ms: unix_millis(),
            nonce: random_token(12),
            message_type: message_type.clone(),
            encrypted_payload: encrypted_payload.clone(),
        };
        let signature =
            STANDARD_NO_PAD.encode(identity.sign_bytes(&serde_json::to_vec(&unsigned)?)?);
        Ok(Self {
            protocol_version: unsigned.protocol_version,
            sender_peer_id: unsigned.sender_peer_id,
            recipient_peer_id: unsigned.recipient_peer_id,
            sender_public_key: unsigned.sender_public_key,
            session_id: unsigned.session_id,
            timestamp_unix_ms: unsigned.timestamp_unix_ms,
            nonce: unsigned.nonce,
            message_type,
            encrypted_payload,
            signature,
        })
    }

    pub fn verify(&self) -> Result<(), ChatError> {
        let unsigned = UnsignedEncryptedEnvelope {
            protocol_version: self.protocol_version.clone(),
            sender_peer_id: self.sender_peer_id.clone(),
            recipient_peer_id: self.recipient_peer_id.clone(),
            sender_public_key: self.sender_public_key.clone(),
            session_id: self.session_id.clone(),
            timestamp_unix_ms: self.timestamp_unix_ms,
            nonce: self.nonce.clone(),
            message_type: self.message_type.clone(),
            encrypted_payload: self.encrypted_payload.clone(),
        };
        PersistentNodeIdentity::verify_bytes(
            &self.sender_peer_id,
            &self.sender_public_key,
            &serde_json::to_vec(&unsigned)?,
            &self.signature,
        )?;
        Ok(())
    }

    pub fn payload_bytes(&self) -> Result<Vec<u8>, ChatError> {
        STANDARD_NO_PAD
            .decode(self.encrypted_payload.as_bytes())
            .map_err(|_| ChatError::InvalidBase64Encoding)
    }
}

impl RoomMembershipEvent {
    pub fn new(
        identity: &PersistentNodeIdentity,
        room: impl Into<String>,
        action: RoomMembershipAction,
    ) -> Result<Self, ChatError> {
        let unsigned = UnsignedRoomMembershipEvent {
            protocol_version: CHAT_PROTOCOL_VERSION.to_string(),
            room: room.into(),
            peer_id: identity.peer_id_string(),
            sender_public_key: identity.public_key_protobuf_base64(),
            action: action.clone(),
            timestamp_unix_ms: unix_millis(),
            nonce: random_token(16),
        };
        let signature =
            STANDARD_NO_PAD.encode(identity.sign_bytes(&serde_json::to_vec(&unsigned)?)?);
        Ok(Self {
            protocol_version: unsigned.protocol_version,
            room: unsigned.room,
            peer_id: unsigned.peer_id,
            sender_public_key: unsigned.sender_public_key,
            action,
            timestamp_unix_ms: unsigned.timestamp_unix_ms,
            nonce: unsigned.nonce,
            signature,
        })
    }

    pub fn verify(&self) -> Result<(), ChatError> {
        let unsigned = UnsignedRoomMembershipEvent {
            protocol_version: self.protocol_version.clone(),
            room: self.room.clone(),
            peer_id: self.peer_id.clone(),
            sender_public_key: self.sender_public_key.clone(),
            action: self.action.clone(),
            timestamp_unix_ms: self.timestamp_unix_ms,
            nonce: self.nonce.clone(),
        };
        PersistentNodeIdentity::verify_bytes(
            &self.peer_id,
            &self.sender_public_key,
            &serde_json::to_vec(&unsigned)?,
            &self.signature,
        )?;
        Ok(())
    }
}

impl RoomStateSnapshot {
    pub fn new(
        identity: &PersistentNodeIdentity,
        snapshot: &ChatRoomSnapshot,
    ) -> Result<Self, ChatError> {
        let unsigned = UnsignedRoomStateSnapshot {
            protocol_version: CHAT_PROTOCOL_VERSION.to_string(),
            room: snapshot.room.clone(),
            peer_id: identity.peer_id_string(),
            sender_public_key: identity.public_key_protobuf_base64(),
            members: snapshot.members.clone(),
            recent_events: snapshot
                .event_history
                .iter()
                .rev()
                .take(12)
                .cloned()
                .collect(),
            timestamp_unix_ms: unix_millis(),
            nonce: random_token(16),
        };
        let signature =
            STANDARD_NO_PAD.encode(identity.sign_bytes(&serde_json::to_vec(&unsigned)?)?);
        Ok(Self {
            protocol_version: unsigned.protocol_version,
            room: unsigned.room,
            peer_id: unsigned.peer_id,
            sender_public_key: unsigned.sender_public_key,
            members: unsigned.members,
            recent_events: unsigned.recent_events,
            timestamp_unix_ms: unsigned.timestamp_unix_ms,
            nonce: unsigned.nonce,
            signature,
        })
    }

    pub fn verify(&self) -> Result<(), ChatError> {
        let unsigned = UnsignedRoomStateSnapshot {
            protocol_version: self.protocol_version.clone(),
            room: self.room.clone(),
            peer_id: self.peer_id.clone(),
            sender_public_key: self.sender_public_key.clone(),
            members: self.members.clone(),
            recent_events: self.recent_events.clone(),
            timestamp_unix_ms: self.timestamp_unix_ms,
            nonce: self.nonce.clone(),
        };
        PersistentNodeIdentity::verify_bytes(
            &self.peer_id,
            &self.sender_public_key,
            &serde_json::to_vec(&unsigned)?,
            &self.signature,
        )?;
        Ok(())
    }
}

pub fn derive_session_key(
    secret: EphemeralSecret,
    remote_public_key: X25519PublicKey,
    session_id: &str,
) -> [u8; 32] {
    let shared = secret.diffie_hellman(&remote_public_key);
    let mut material = Vec::from(shared.as_bytes().as_slice());
    material.extend_from_slice(session_id.as_bytes());
    *blake3::hash(&material).as_bytes()
}

pub fn encrypt_payload(key: &[u8; 32], plaintext: &[u8]) -> Result<EncryptedPayload, ChatError> {
    let cipher = Aes256Gcm::new_from_slice(key).map_err(|_| ChatError::InvalidKeyLength)?;
    let mut nonce_bytes = [0u8; 12];
    OsRng.fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from(nonce_bytes);
    let ciphertext = cipher
        .encrypt(&nonce, plaintext)
        .map_err(|_| ChatError::EncryptionFailed)?;
    Ok(EncryptedPayload {
        algorithm: "aes-256-gcm".to_string(),
        nonce: nonce_bytes.to_vec(),
        ciphertext,
    })
}

pub fn decrypt_payload(key: &[u8; 32], payload: &EncryptedPayload) -> Result<Vec<u8>, ChatError> {
    let cipher = Aes256Gcm::new_from_slice(key).map_err(|_| ChatError::InvalidKeyLength)?;
    if payload.nonce.len() != 12 {
        return Err(ChatError::InvalidNonceLength);
    }
    let nonce = Nonce::from_slice(&payload.nonce);
    cipher
        .decrypt(nonce, payload.ciphertext.as_ref())
        .map_err(|_| ChatError::DecryptionFailed)
}

pub fn serialize_payload(payload: &EncryptedPayload) -> Result<Vec<u8>, ChatError> {
    Ok(serde_json::to_vec(payload)?)
}

pub fn deserialize_payload(bytes: &[u8]) -> Result<EncryptedPayload, ChatError> {
    Ok(serde_json::from_slice(bytes)?)
}

pub fn enqueue_local_command(
    data_dir: impl AsRef<Path>,
    command: ChatLocalCommand,
) -> Result<PathBuf, ChatError> {
    let envelope = QueuedChatCommand {
        command_id: random_file_token(12),
        issued_at_unix_ms: unix_millis(),
        command,
    };
    let commands_dir = chat_commands_dir(data_dir.as_ref());
    fs::create_dir_all(&commands_dir)?;
    let path = commands_dir.join(format!(
        "{}-{}.json",
        envelope.issued_at_unix_ms, envelope.command_id
    ));
    fs::write(&path, serde_json::to_vec_pretty(&envelope)?)?;
    Ok(path)
}

pub fn drain_local_commands(
    data_dir: impl AsRef<Path>,
) -> Result<Vec<QueuedChatCommand>, ChatError> {
    let commands_dir = chat_commands_dir(data_dir.as_ref());
    if !commands_dir.exists() {
        return Ok(Vec::new());
    }

    let mut paths: Vec<PathBuf> = fs::read_dir(&commands_dir)?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("json"))
        .collect();
    paths.sort();

    let mut commands = Vec::new();
    for path in paths {
        let raw = fs::read(&path)?;
        let command: QueuedChatCommand = serde_json::from_slice(&raw)?;
        fs::remove_file(&path)?;
        commands.push(command);
    }

    Ok(commands)
}

pub fn load_chat_inbox(data_dir: impl AsRef<Path>) -> Result<ChatInboxState, ChatError> {
    load_json_or_default(chat_inbox_file(data_dir.as_ref()))
}

pub fn save_chat_inbox(
    data_dir: impl AsRef<Path>,
    inbox: &ChatInboxState,
) -> Result<(), ChatError> {
    save_json(chat_inbox_file(data_dir.as_ref()), inbox)
}

pub fn load_chat_rooms(data_dir: impl AsRef<Path>) -> Result<ChatRoomsState, ChatError> {
    load_json_or_default(chat_rooms_file(data_dir.as_ref()))
}

pub fn save_chat_rooms(
    data_dir: impl AsRef<Path>,
    rooms: &ChatRoomsState,
) -> Result<(), ChatError> {
    save_json(chat_rooms_file(data_dir.as_ref()), rooms)
}

pub fn load_chat_sessions(data_dir: impl AsRef<Path>) -> Result<ChatSessionsState, ChatError> {
    load_json_or_default(chat_sessions_file(data_dir.as_ref()))
}

pub fn save_chat_sessions(
    data_dir: impl AsRef<Path>,
    sessions: &ChatSessionsState,
) -> Result<(), ChatError> {
    save_json(chat_sessions_file(data_dir.as_ref()), sessions)
}

pub fn load_chat_notifications(
    data_dir: impl AsRef<Path>,
) -> Result<ChatNotificationsState, ChatError> {
    load_json_or_default(chat_notifications_file(data_dir.as_ref()))
}

pub fn save_chat_notifications(
    data_dir: impl AsRef<Path>,
    notifications: &ChatNotificationsState,
) -> Result<(), ChatError> {
    save_json(chat_notifications_file(data_dir.as_ref()), notifications)
}

pub fn upsert_room_member(rooms: &mut ChatRoomsState, room: &str, peer_id: &str, joined: bool) {
    let now = unix_millis();
    let snapshot = ensure_room_snapshot_mut(rooms, room);

    if let Some(member) = snapshot
        .members
        .iter_mut()
        .find(|member| member.peer_id == peer_id)
    {
        member.presence = if joined { "ONLINE" } else { "OFFLINE" }.to_string();
        member.last_seen_unix_ms = now;
    } else {
        snapshot.members.push(RoomMember {
            peer_id: peer_id.to_string(),
            presence: if joined { "ONLINE" } else { "OFFLINE" }.to_string(),
            last_seen_unix_ms: now,
        });
    }

    snapshot.last_changed_unix_ms = now;
    refresh_room_snapshot(snapshot);
    rooms.sync_revision = rooms.sync_revision.saturating_add(1);
}

pub fn set_local_room_joined(rooms: &mut ChatRoomsState, room: &str, joined: bool) {
    let now = unix_millis();
    let snapshot = ensure_room_snapshot_mut(rooms, room);

    snapshot.joined = joined;
    snapshot.last_changed_unix_ms = now;
    refresh_room_snapshot(snapshot);
    if joined {
        rooms.current_room = Some(room.to_string());
    } else if rooms.current_room.as_deref() == Some(room) {
        rooms.current_room = rooms
            .rooms
            .iter()
            .find(|entry| entry.joined)
            .map(|entry| entry.room.clone());
    }
    rooms.sync_revision = rooms.sync_revision.saturating_add(1);
}

pub fn set_current_room(rooms: &mut ChatRoomsState, room: Option<String>) {
    rooms.current_room = room;
    rooms.sync_revision = rooms.sync_revision.saturating_add(1);
}

pub fn record_room_event(
    rooms: &mut ChatRoomsState,
    room: &str,
    event_type: &str,
    peer_id: &str,
    body: Option<String>,
) -> String {
    let now = unix_millis();
    let event_id = random_file_token(10);
    let snapshot = ensure_room_snapshot_mut(rooms, room);
    snapshot.event_history.push(ChatRoomEvent {
        event_id: event_id.clone(),
        event_type: event_type.to_string(),
        peer_id: peer_id.to_string(),
        body,
        timestamp_unix_ms: now,
    });
    snapshot
        .event_history
        .sort_by_key(|event| event.timestamp_unix_ms);
    if snapshot.event_history.len() > ROOM_EVENT_HISTORY_LIMIT {
        let keep_from = snapshot.event_history.len() - ROOM_EVENT_HISTORY_LIMIT;
        snapshot.event_history.drain(0..keep_from);
    }
    snapshot.last_changed_unix_ms = now;
    refresh_room_snapshot(snapshot);
    rooms.sync_revision = rooms.sync_revision.saturating_add(1);
    event_id
}

pub fn merge_room_snapshot(rooms: &mut ChatRoomsState, remote: &ChatRoomSnapshot) {
    let snapshot = ensure_room_snapshot_mut(rooms, &remote.room);
    for remote_member in &remote.members {
        if let Some(member) = snapshot
            .members
            .iter_mut()
            .find(|member| member.peer_id == remote_member.peer_id)
        {
            if remote_member.last_seen_unix_ms >= member.last_seen_unix_ms {
                *member = remote_member.clone();
            }
        } else {
            snapshot.members.push(remote_member.clone());
        }
    }
    for remote_event in &remote.event_history {
        if !snapshot
            .event_history
            .iter()
            .any(|event| event.event_id == remote_event.event_id)
        {
            snapshot.event_history.push(remote_event.clone());
        }
    }
    snapshot.room_id = if snapshot.room_id.is_empty() {
        remote.room_id.clone()
    } else {
        snapshot.room_id.clone()
    };
    snapshot.room_name = if snapshot.room_name.is_empty() {
        remote.room_name.clone()
    } else {
        snapshot.room_name.clone()
    };
    snapshot.last_changed_unix_ms = snapshot
        .last_changed_unix_ms
        .max(remote.last_changed_unix_ms);
    refresh_room_snapshot(snapshot);
    rooms.sync_revision = rooms.sync_revision.saturating_add(1);
}

pub fn mark_inbox_read(inbox: &mut ChatInboxState, room: Option<&str>) -> usize {
    let mut cleared = 0;
    for message in &mut inbox.messages {
        let matches_room = match room {
            Some(room) => message.room.as_deref() == Some(room),
            None => true,
        };
        if matches_room && message.unread {
            message.unread = false;
            cleared += 1;
        }
    }
    cleared
}

pub fn unread_count(inbox: &ChatInboxState, room: Option<&str>) -> usize {
    inbox
        .messages
        .iter()
        .filter(|message| message.unread)
        .filter(|message| match room {
            Some(room) => message.room.as_deref() == Some(room),
            None => true,
        })
        .count()
}

pub fn push_notification(
    notifications: &mut ChatNotificationsState,
    kind: &str,
    room: Option<&str>,
    peer_id: Option<&str>,
    message: impl Into<String>,
) -> usize {
    notifications.notifications.push(ChatNotification {
        notification_id: random_file_token(10),
        kind: kind.to_string(),
        message: message.into(),
        room: room.map(ToString::to_string),
        peer_id: peer_id.map(ToString::to_string),
        created_at_unix_ms: unix_millis(),
        unread: true,
    });
    notifications
        .notifications
        .sort_by_key(|entry| entry.created_at_unix_ms);
    if notifications.notifications.len() > CHAT_NOTIFICATION_LIMIT {
        let keep_from = notifications.notifications.len() - CHAT_NOTIFICATION_LIMIT;
        notifications.notifications.drain(0..keep_from);
    }
    notifications
        .notifications
        .iter()
        .filter(|entry| entry.unread)
        .count()
}

pub fn mark_notifications_read(
    notifications: &mut ChatNotificationsState,
    room: Option<&str>,
) -> usize {
    let mut cleared = 0;
    for notification in &mut notifications.notifications {
        let matches_room = match room {
            Some(room) => notification.room.as_deref() == Some(room),
            None => true,
        };
        if matches_room && notification.unread {
            notification.unread = false;
            cleared += 1;
        }
    }
    cleared
}

pub fn now_unix_ms() -> u128 {
    unix_millis()
}

pub fn random_ephemeral_secret() -> EphemeralSecret {
    EphemeralSecret::random_from_rng(OsRng)
}

pub fn public_key_from_secret(secret: &EphemeralSecret) -> X25519PublicKey {
    X25519PublicKey::from(secret)
}

#[derive(Debug, Error)]
pub enum ChatError {
    #[error("chat IO failed: {0}")]
    Io(#[from] io::Error),
    #[error("chat JSON failed: {0}")]
    Json(#[from] serde_json::Error),
    #[error("chat identity failed: {0}")]
    Identity(#[from] IdentityError),
    #[error("chat key length is invalid")]
    InvalidKeyLength,
    #[error("chat nonce length is invalid")]
    InvalidNonceLength,
    #[error("chat base64 payload is invalid")]
    InvalidBase64Encoding,
    #[error("chat x25519 public key is invalid")]
    InvalidX25519PublicKey,
    #[error("chat encryption failed")]
    EncryptionFailed,
    #[error("chat decryption failed")]
    DecryptionFailed,
    #[error("chat replay detected")]
    ReplayDetected,
    #[error("chat replay window expired")]
    ReplayWindowExpired,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct UnsignedSessionOffer {
    protocol_version: String,
    session_id: String,
    sender_peer_id: String,
    recipient_peer_id: String,
    sender_public_key: String,
    ephemeral_public_key: String,
    timestamp_unix_ms: u128,
    nonce: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct UnsignedSessionAck {
    protocol_version: String,
    session_id: String,
    sender_peer_id: String,
    recipient_peer_id: String,
    sender_public_key: String,
    ephemeral_public_key: String,
    timestamp_unix_ms: u128,
    nonce: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct UnsignedEncryptedEnvelope {
    protocol_version: String,
    sender_peer_id: String,
    recipient_peer_id: String,
    sender_public_key: String,
    session_id: String,
    timestamp_unix_ms: u128,
    nonce: String,
    message_type: ChatMessageType,
    encrypted_payload: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct UnsignedRoomMembershipEvent {
    protocol_version: String,
    room: String,
    peer_id: String,
    sender_public_key: String,
    action: RoomMembershipAction,
    timestamp_unix_ms: u128,
    nonce: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct UnsignedRoomStateSnapshot {
    protocol_version: String,
    room: String,
    peer_id: String,
    sender_public_key: String,
    members: Vec<RoomMember>,
    recent_events: Vec<ChatRoomEvent>,
    timestamp_unix_ms: u128,
    nonce: String,
}

fn load_json_or_default<T>(path: PathBuf) -> Result<T, ChatError>
where
    T: DeserializeOwned + Default,
{
    if !path.exists() {
        return Ok(T::default());
    }

    Ok(serde_json::from_slice(&fs::read(path)?)?)
}

fn save_json<T>(path: PathBuf, value: &T) -> Result<(), ChatError>
where
    T: Serialize,
{
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    fs::write(path, serde_json::to_vec_pretty(value)?)?;
    Ok(())
}

fn chat_dir(data_dir: &Path) -> PathBuf {
    data_dir.join(CHAT_DIR)
}

fn chat_commands_dir(data_dir: &Path) -> PathBuf {
    chat_dir(data_dir).join(CHAT_COMMANDS_DIR)
}

fn chat_inbox_file(data_dir: &Path) -> PathBuf {
    chat_dir(data_dir).join(CHAT_INBOX_FILE)
}

fn chat_rooms_file(data_dir: &Path) -> PathBuf {
    chat_dir(data_dir).join(CHAT_ROOMS_FILE)
}

fn chat_sessions_file(data_dir: &Path) -> PathBuf {
    chat_dir(data_dir).join(CHAT_SESSIONS_FILE)
}

fn chat_notifications_file(data_dir: &Path) -> PathBuf {
    chat_dir(data_dir).join(CHAT_NOTIFICATIONS_FILE)
}

fn ensure_room_snapshot_mut<'a>(
    rooms: &'a mut ChatRoomsState,
    room: &str,
) -> &'a mut ChatRoomSnapshot {
    if !rooms.rooms.iter().any(|entry| entry.room == room) {
        rooms.rooms.push(ChatRoomSnapshot {
            room: room.to_string(),
            joined: false,
            members: Vec::new(),
            room_id: room.to_string(),
            room_name: room.to_string(),
            active_members: 0,
            event_history: Vec::new(),
            last_changed_unix_ms: unix_millis(),
        });
    }
    let snapshot = rooms
        .rooms
        .iter_mut()
        .find(|entry| entry.room == room)
        .expect("room snapshot inserted");
    if snapshot.room_id.is_empty() {
        snapshot.room_id = snapshot.room.clone();
    }
    if snapshot.room_name.is_empty() {
        snapshot.room_name = snapshot.room.clone();
    }
    refresh_room_snapshot(snapshot);
    snapshot
}

fn refresh_room_snapshot(snapshot: &mut ChatRoomSnapshot) {
    if snapshot.room_id.is_empty() {
        snapshot.room_id = snapshot.room.clone();
    }
    if snapshot.room_name.is_empty() {
        snapshot.room_name = snapshot.room.clone();
    }
    snapshot.active_members = snapshot
        .members
        .iter()
        .filter(|member| member.presence == "ONLINE")
        .count();
    snapshot
        .members
        .sort_by(|left, right| left.peer_id.cmp(&right.peer_id));
    snapshot
        .event_history
        .sort_by_key(|event| event.timestamp_unix_ms);
    if snapshot.event_history.len() > ROOM_EVENT_HISTORY_LIMIT {
        let keep_from = snapshot.event_history.len() - ROOM_EVENT_HISTORY_LIMIT;
        snapshot.event_history.drain(0..keep_from);
    }
}

fn generate_session_id() -> String {
    random_token(18)
}

fn random_token(len: usize) -> String {
    let mut bytes = vec![0u8; len];
    OsRng.fill_bytes(&mut bytes);
    STANDARD_NO_PAD.encode(bytes)
}

fn random_file_token(len: usize) -> String {
    random_token(len).replace('/', "_").replace('+', "-")
}

fn decode_x25519_public_key(value: &str) -> Result<X25519PublicKey, ChatError> {
    let bytes: [u8; 32] = STANDARD_NO_PAD
        .decode(value.as_bytes())
        .map_err(|_| ChatError::InvalidBase64Encoding)?
        .try_into()
        .map_err(|_| ChatError::InvalidX25519PublicKey)?;
    Ok(X25519PublicKey::from(bytes))
}

fn unix_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use void_identity::PersistentNodeIdentity;

    #[test]
    fn session_negotiation_derives_same_key() {
        let dir_a = PathBuf::from("/tmp/voidnet-chat-test-a");
        let dir_b = PathBuf::from("/tmp/voidnet-chat-test-b");
        let identity_a = PersistentNodeIdentity::load_or_create_dir(&dir_a).unwrap();
        let identity_b = PersistentNodeIdentity::load_or_create_dir(&dir_b).unwrap();

        let secret_a = random_ephemeral_secret();
        let public_a = public_key_from_secret(&secret_a);
        let offer = SessionOffer::new(
            &identity_a,
            identity_b.peer_id_string(),
            public_a.to_bytes(),
        )
        .unwrap();
        offer.verify().unwrap();

        let secret_b = random_ephemeral_secret();
        let public_b = public_key_from_secret(&secret_b);
        let ack = SessionAck::new(
            &identity_b,
            identity_a.peer_id_string(),
            offer.session_id.clone(),
            public_b.to_bytes(),
        )
        .unwrap();
        ack.verify().unwrap();

        let key_a = derive_session_key(secret_a, ack.peer_public_key().unwrap(), &offer.session_id);
        let key_b = derive_session_key(
            secret_b,
            offer.peer_public_key().unwrap(),
            &offer.session_id,
        );

        assert_eq!(key_a, key_b);
    }

    #[test]
    fn encrypts_and_decrypts_payload() {
        let key = [7u8; 32];
        let payload = encrypt_payload(&key, b"voidnet-secret").unwrap();
        let plaintext = decrypt_payload(&key, &payload).unwrap();

        assert_eq!(plaintext, b"voidnet-secret");
    }

    #[test]
    fn replay_protector_rejects_duplicate_nonce() {
        let mut protector = ReplayProtector::new(Duration::from_secs(CHAT_REPLAY_WINDOW_SECS));
        let now = now_unix_ms();

        protector
            .check_and_record("peer-a", "nonce-1", now)
            .unwrap();
        assert!(matches!(
            protector.check_and_record("peer-a", "nonce-1", now),
            Err(ChatError::ReplayDetected)
        ));
    }

    #[test]
    fn enqueues_local_command_into_cold_directory() {
        let data_dir =
            std::env::temp_dir().join(format!("voidnet-chat-queue-{}", std::process::id()));
        let path = enqueue_local_command(
            &data_dir,
            ChatLocalCommand::Join {
                room: "operators".to_string(),
            },
        )
        .unwrap();

        assert!(path.exists());
        assert_eq!(path.extension().and_then(|ext| ext.to_str()), Some("json"));
        assert!(path.to_string_lossy().contains("chat/commands/"));
    }

    #[test]
    fn merges_room_snapshot_and_preserves_history() {
        let mut rooms = ChatRoomsState::default();
        set_local_room_joined(&mut rooms, "operators", true);
        upsert_room_member(&mut rooms, "operators", "peer-a", true);
        let local_event = record_room_event(&mut rooms, "operators", "room.join", "peer-a", None);

        let remote = ChatRoomSnapshot {
            room: "operators".to_string(),
            joined: false,
            members: vec![RoomMember {
                peer_id: "peer-b".to_string(),
                presence: "ONLINE".to_string(),
                last_seen_unix_ms: now_unix_ms(),
            }],
            room_id: "operators".to_string(),
            room_name: "operators".to_string(),
            active_members: 1,
            event_history: vec![ChatRoomEvent {
                event_id: "evt-remote".to_string(),
                event_type: "room.message".to_string(),
                peer_id: "peer-b".to_string(),
                body: Some("hello".to_string()),
                timestamp_unix_ms: now_unix_ms(),
            }],
            last_changed_unix_ms: now_unix_ms(),
        };

        merge_room_snapshot(&mut rooms, &remote);

        let operators = rooms
            .rooms
            .iter()
            .find(|room| room.room == "operators")
            .unwrap();
        assert!(operators.joined);
        assert_eq!(operators.active_members, 2);
        assert!(operators
            .event_history
            .iter()
            .any(|event| event.event_id == local_event));
        assert!(operators
            .event_history
            .iter()
            .any(|event| event.event_id == "evt-remote"));
    }

    #[test]
    fn marks_room_messages_and_notifications_as_read() {
        let mut inbox = ChatInboxState {
            messages: vec![ChatInboxEntry {
                message_id: "msg-1".to_string(),
                from_peer_id: "peer-a".to_string(),
                room: Some("operators".to_string()),
                body: "hello".to_string(),
                received_at_unix_ms: now_unix_ms(),
                session_id: "session-1".to_string(),
                signature_verified: true,
                unread: true,
                room_name: Some("operators".to_string()),
            }],
        };
        let mut notifications = ChatNotificationsState::default();
        push_notification(
            &mut notifications,
            "message.received",
            Some("operators"),
            Some("peer-a"),
            "new message",
        );

        assert_eq!(unread_count(&inbox, Some("operators")), 1);
        assert_eq!(mark_inbox_read(&mut inbox, Some("operators")), 1);
        assert_eq!(
            mark_notifications_read(&mut notifications, Some("operators")),
            1
        );
        assert_eq!(unread_count(&inbox, Some("operators")), 0);
        assert!(notifications
            .notifications
            .iter()
            .all(|entry| !entry.unread));
    }
}
