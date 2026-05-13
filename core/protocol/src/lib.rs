use serde::{Deserialize, Serialize};
use std::{fmt, str::FromStr};
use thiserror::Error;

pub mod ui;

pub const VOID_SCHEME: &str = "void";
pub const MAX_FRAME_BYTES: usize = 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProtocolVersion {
    pub major: u16,
    pub minor: u16,
}

impl ProtocolVersion {
    pub const V1: Self = Self { major: 1, minor: 0 };
}

impl Default for ProtocolVersion {
    fn default() -> Self {
        Self::V1
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct VoidUri {
    authority: String,
    path: String,
    query: Option<String>,
}

impl VoidUri {
    pub fn new(
        authority: impl Into<String>,
        path: impl Into<String>,
        query: Option<String>,
    ) -> Result<Self, ParseVoidUriError> {
        let authority = authority.into();
        let mut path = path.into();

        if authority.is_empty() {
            return Err(ParseVoidUriError::MissingAuthority);
        }

        if !authority
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '-' | '_' | ':'))
        {
            return Err(ParseVoidUriError::InvalidAuthority(authority));
        }

        if path.is_empty() {
            path = "/".to_string();
        }

        if !path.starts_with('/') {
            path.insert(0, '/');
        }

        Ok(Self {
            authority,
            path,
            query,
        })
    }

    pub fn authority(&self) -> &str {
        &self.authority
    }

    pub fn path(&self) -> &str {
        &self.path
    }

    pub fn query(&self) -> Option<&str> {
        self.query.as_deref()
    }

    pub fn is_void_domain(&self) -> bool {
        self.authority.ends_with(".void")
    }
}

impl fmt::Display for VoidUri {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{VOID_SCHEME}://{}{}", self.authority, self.path)?;
        if let Some(query) = &self.query {
            write!(formatter, "?{query}")?;
        }
        Ok(())
    }
}

impl FromStr for VoidUri {
    type Err = ParseVoidUriError;

    fn from_str(raw: &str) -> Result<Self, Self::Err> {
        let without_scheme = raw
            .strip_prefix("void://")
            .ok_or(ParseVoidUriError::InvalidScheme)?;
        let (without_query, query) = match without_scheme.split_once('?') {
            Some((head, tail)) => (head, Some(tail.to_string())),
            None => (without_scheme, None),
        };
        let (authority, path) = match without_query.split_once('/') {
            Some((authority, path)) => (authority, format!("/{path}")),
            None => (without_query, "/".to_string()),
        };

        Self::new(authority, path, query)
    }
}

#[derive(Debug, Error)]
pub enum ParseVoidUriError {
    #[error("VOID URI must start with void://")]
    InvalidScheme,
    #[error("VOID URI authority is missing")]
    MissingAuthority,
    #[error("VOID URI authority contains unsupported characters: {0}")]
    InvalidAuthority(String),
}

pub type StreamId = u64;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Envelope {
    pub version: ProtocolVersion,
    pub stream_id: StreamId,
    pub ttl: u8,
    pub frame: Frame,
}

impl Envelope {
    pub fn new(stream_id: StreamId, frame: Frame) -> Self {
        Self {
            version: ProtocolVersion::V1,
            stream_id,
            ttl: 32,
            frame,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Frame {
    Hello(HelloFrame),
    PeerMessage(PeerMessage),
    Route(RouteFrame),
    ContentRequest(ContentRequest),
    ContentChunk(ContentChunk),
    StreamOpen(StreamOpen),
    StreamData(StreamData),
    StreamClose(StreamClose),
    AppCall(AppCall),
    UiDocument(UiDocument),
    Error(ProtocolErrorFrame),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HelloFrame {
    pub peer_id: String,
    pub agent: String,
    pub supported_versions: Vec<ProtocolVersion>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PeerMessage {
    pub from: String,
    pub to: String,
    pub kind: MessageKind,
    pub content_type: ContentType,
    pub payload: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum MessageKind {
    Direct,
    Room { room_id: String },
    System,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ContentType {
    Binary,
    Text,
    Json,
    VoidUi,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RouteFrame {
    pub destination: String,
    pub next_hop: Option<String>,
    pub path: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContentRequest {
    pub uri: VoidUri,
    pub accept: Vec<ContentType>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContentChunk {
    pub uri: VoidUri,
    pub sequence: u64,
    pub final_chunk: bool,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StreamOpen {
    pub uri: VoidUri,
    pub mode: StreamMode,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum StreamMode {
    Request,
    Subscribe,
    Duplex,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StreamData {
    pub sequence: u64,
    pub bytes: Vec<u8>,
    pub final_chunk: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StreamClose {
    pub reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppCall {
    pub uri: VoidUri,
    pub method: String,
    pub payload: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UiDocument {
    pub uri: VoidUri,
    pub source: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProtocolErrorFrame {
    pub code: u16,
    pub message: String,
}

pub fn encode_envelope(envelope: &Envelope) -> Result<Vec<u8>, ProtocolCodecError> {
    let bytes = bincode::serialize(envelope)?;
    if bytes.len() > MAX_FRAME_BYTES {
        return Err(ProtocolCodecError::FrameTooLarge {
            actual: bytes.len(),
            max: MAX_FRAME_BYTES,
        });
    }
    Ok(bytes)
}

pub fn decode_envelope(bytes: &[u8]) -> Result<Envelope, ProtocolCodecError> {
    if bytes.len() > MAX_FRAME_BYTES {
        return Err(ProtocolCodecError::FrameTooLarge {
            actual: bytes.len(),
            max: MAX_FRAME_BYTES,
        });
    }
    Ok(bincode::deserialize(bytes)?)
}

#[derive(Debug, Error)]
pub enum ProtocolCodecError {
    #[error("VOID frame is too large: {actual} bytes exceeds {max} bytes")]
    FrameTooLarge { actual: usize, max: usize },
    #[error("VOID frame codec failure: {0}")]
    Codec(#[from] Box<bincode::ErrorKind>),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_void_uri() {
        let uri: VoidUri = "void://chat.void/room/main?limit=64".parse().unwrap();
        assert_eq!(uri.authority(), "chat.void");
        assert_eq!(uri.path(), "/room/main");
        assert_eq!(uri.query(), Some("limit=64"));
        assert!(uri.is_void_domain());
    }

    #[test]
    fn encodes_round_trip_envelope() {
        let uri: VoidUri = "void://core.void".parse().unwrap();
        let envelope = Envelope::new(
            7,
            Frame::ContentRequest(ContentRequest {
                uri,
                accept: vec![ContentType::VoidUi],
            }),
        );

        let bytes = encode_envelope(&envelope).unwrap();
        let decoded = decode_envelope(&bytes).unwrap();

        assert_eq!(decoded, envelope);
    }
}
