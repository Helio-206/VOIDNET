use async_trait::async_trait;
use base64::{engine::general_purpose::STANDARD_NO_PAD, Engine as _};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fmt, fs, io,
    path::{Path, PathBuf},
    str::FromStr,
    sync::Arc,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use thiserror::Error;
use tokio::sync::RwLock;
use void_identity::PersistentNodeIdentity;
use void_protocol::VoidUri;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct VoidDomain(String);

impl VoidDomain {
    pub fn new(raw: impl Into<String>) -> Result<Self, VoidDnsError> {
        let domain = raw.into().to_ascii_lowercase();

        if !domain.ends_with(".void") {
            return Err(VoidDnsError::InvalidDomain(domain));
        }

        let labels: Vec<&str> = domain.trim_end_matches(".void").split('.').collect();
        if labels.is_empty()
            || labels.iter().any(|label| {
                label.is_empty()
                    || !label
                        .chars()
                        .all(|ch| ch.is_ascii_alphanumeric() || ch == '-')
            })
        {
            return Err(VoidDnsError::InvalidDomain(domain));
        }

        Ok(Self(domain))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for VoidDomain {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl FromStr for VoidDomain {
    type Err = VoidDnsError;

    fn from_str(raw: &str) -> Result<Self, Self::Err> {
        Self::new(raw)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DnsRecord {
    pub domain: VoidDomain,
    pub owner_peer_id: String,
    pub target_peer_id: String,
    pub runtime_surface: String,
    pub capabilities: Vec<String>,
    pub created_at_unix_ms: u128,
    pub expires_at_unix_ms: u128,
    pub public_key: String,
    pub signature: String,
}

impl DnsRecord {
    pub fn sign(
        identity: &PersistentNodeIdentity,
        domain: VoidDomain,
        target_peer_id: impl Into<String>,
        runtime_surface: impl Into<String>,
        capabilities: Vec<String>,
        ttl: Duration,
    ) -> Result<Self, VoidDnsError> {
        let created_at_unix_ms = unix_millis();
        let unsigned = UnsignedDnsRecord {
            domain,
            owner_peer_id: identity.peer_id_string(),
            target_peer_id: target_peer_id.into(),
            runtime_surface: runtime_surface.into(),
            capabilities: normalize_capabilities(capabilities),
            created_at_unix_ms,
            expires_at_unix_ms: created_at_unix_ms + ttl.as_millis(),
            public_key: identity.public_key_protobuf_base64(),
        };
        let signature = identity.sign_bytes(&serde_json::to_vec(&unsigned)?)?;
        Ok(Self {
            domain: unsigned.domain,
            owner_peer_id: unsigned.owner_peer_id,
            target_peer_id: unsigned.target_peer_id,
            runtime_surface: unsigned.runtime_surface,
            capabilities: unsigned.capabilities,
            created_at_unix_ms: unsigned.created_at_unix_ms,
            expires_at_unix_ms: unsigned.expires_at_unix_ms,
            public_key: unsigned.public_key,
            signature: STANDARD_NO_PAD.encode(signature),
        })
    }

    pub fn verify(&self) -> Result<(), VoidDnsError> {
        PersistentNodeIdentity::verify_bytes(
            &self.owner_peer_id,
            &self.public_key,
            &serde_json::to_vec(&self.unsigned())?,
            &self.signature,
        )?;
        Ok(())
    }

    pub fn is_expired(&self, now_unix_ms: u128) -> bool {
        self.expires_at_unix_ms <= now_unix_ms
    }

    pub fn ttl_remaining_secs(&self, now_unix_ms: u128) -> u64 {
        if self.is_expired(now_unix_ms) {
            0
        } else {
            ((self.expires_at_unix_ms - now_unix_ms) / 1_000) as u64
        }
    }

    pub fn resolution_target(&self) -> ResolutionTarget {
        ResolutionTarget::Service(ServiceTarget {
            peer_id: self.target_peer_id.clone(),
            runtime_surface: self.runtime_surface.clone(),
            capabilities: self.capabilities.clone(),
        })
    }

    pub fn fingerprint(&self) -> Result<String, VoidDnsError> {
        Ok(blake3::hash(&serde_json::to_vec(self)?)
            .to_hex()
            .to_string())
    }

    fn unsigned(&self) -> UnsignedDnsRecord {
        UnsignedDnsRecord {
            domain: self.domain.clone(),
            owner_peer_id: self.owner_peer_id.clone(),
            target_peer_id: self.target_peer_id.clone(),
            runtime_surface: self.runtime_surface.clone(),
            capabilities: self.capabilities.clone(),
            created_at_unix_ms: self.created_at_unix_ms,
            expires_at_unix_ms: self.expires_at_unix_ms,
            public_key: self.public_key.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResolutionTarget {
    Peer { peer_id: String },
    Content { content_id: String },
    Service(ServiceTarget),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServiceTarget {
    pub peer_id: String,
    pub runtime_surface: String,
    pub capabilities: Vec<String>,
}

#[async_trait]
pub trait VoidDnsResolver {
    async fn resolve(&self, domain: &VoidDomain) -> Result<Option<DnsRecord>, VoidDnsError>;

    async fn resolve_route(&self, uri: &VoidUri) -> Result<Option<DnsResolvedRoute>, VoidDnsError> {
        let domain = VoidDomain::new(uri.authority())?;
        let started = Instant::now();
        let Some(record) = self.resolve(&domain).await? else {
            return Ok(None);
        };

        Ok(Some(DnsResolvedRoute {
            uri: uri.clone(),
            domain,
            target_peer_id: record.target_peer_id.clone(),
            runtime_surface: record.runtime_surface.clone(),
            capabilities: record.capabilities.clone(),
            ttl_remaining_secs: record.ttl_remaining_secs(unix_millis()),
            signature_verified: true,
            resolution_latency_ms: started.elapsed().as_millis(),
        }))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DnsRecordSource {
    LocalPublish,
    MeshPropagation,
    CacheRestore,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DnsCacheEntry {
    pub record: DnsRecord,
    pub verified: bool,
    pub source: DnsRecordSource,
    pub cached_at_unix_ms: u128,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DnsConflict {
    pub domain: VoidDomain,
    pub active_owner_peer_id: String,
    pub conflicting_owner_peer_id: String,
    pub active_record_fingerprint: String,
    pub conflicting_record_fingerprint: String,
    pub detected_at_unix_ms: u128,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct DnsCacheSnapshot {
    pub records: BTreeMap<String, DnsCacheEntry>,
    pub conflicts: BTreeMap<String, Vec<DnsConflict>>,
    pub updated_unix_ms: u128,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DnsApplyOutcome {
    Published(DnsRecord),
    Updated(DnsRecord),
    Duplicate(DnsRecord),
    Conflict {
        active: DnsRecord,
        incoming: DnsRecord,
        conflict: DnsConflict,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DnsResolvedRoute {
    pub uri: VoidUri,
    pub domain: VoidDomain,
    pub target_peer_id: String,
    pub runtime_surface: String,
    pub capabilities: Vec<String>,
    pub ttl_remaining_secs: u64,
    pub signature_verified: bool,
    pub resolution_latency_ms: u128,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DnsInspection {
    pub domain: VoidDomain,
    pub active_record: Option<DnsCacheEntry>,
    pub conflicts: Vec<DnsConflict>,
    pub ttl_remaining_secs: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DnsCommand {
    Publish {
        domain: VoidDomain,
        runtime_surface: Option<String>,
        target_peer_id: Option<String>,
        capabilities: Vec<String>,
        ttl_secs: u64,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QueuedDnsCommand {
    pub command_id: String,
    pub issued_at_unix_ms: u128,
    pub command: DnsCommand,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DnsPropagationMessage {
    pub event_id: String,
    pub issued_at_unix_ms: u128,
    pub record: DnsRecord,
}

#[derive(Clone)]
pub struct PersistentVoidDns {
    root_dir: PathBuf,
    snapshot_path: PathBuf,
    state: Arc<RwLock<DnsCacheSnapshot>>,
}

impl PersistentVoidDns {
    pub fn load_or_create(data_dir: impl AsRef<Path>) -> Result<Self, VoidDnsError> {
        let root_dir = data_dir.as_ref().join("dns");
        fs::create_dir_all(&root_dir)?;
        let snapshot_path = root_dir.join("cache.json");
        let snapshot = load_json_or_default(&snapshot_path)?;
        Ok(Self {
            root_dir,
            snapshot_path,
            state: Arc::new(RwLock::new(snapshot)),
        })
    }

    pub fn root_dir(&self) -> &Path {
        &self.root_dir
    }

    pub async fn publish_record(
        &self,
        identity: &PersistentNodeIdentity,
        domain: VoidDomain,
        target_peer_id: impl Into<String>,
        runtime_surface: impl Into<String>,
        capabilities: Vec<String>,
        ttl: Duration,
    ) -> Result<(DnsRecord, DnsApplyOutcome), VoidDnsError> {
        let record = DnsRecord::sign(
            identity,
            domain,
            target_peer_id,
            runtime_surface,
            capabilities,
            ttl,
        )?;
        let outcome = self
            .apply_record(record.clone(), DnsRecordSource::LocalPublish)
            .await?;
        Ok((record, outcome))
    }

    pub async fn apply_record(
        &self,
        record: DnsRecord,
        source: DnsRecordSource,
    ) -> Result<DnsApplyOutcome, VoidDnsError> {
        record.verify()?;
        let now = unix_millis();
        if record.is_expired(now) {
            return Err(VoidDnsError::ExpiredRecord(record.domain.to_string()));
        }

        let mut state = self.state.write().await;
        let domain_key = record.domain.to_string();
        if let Some(existing) = state.records.get(&domain_key).cloned() {
            if existing.record.is_expired(now) {
                state.records.remove(&domain_key);
            } else if existing.record.fingerprint()? == record.fingerprint()? {
                return Ok(DnsApplyOutcome::Duplicate(existing.record));
            } else if existing.record.owner_peer_id != record.owner_peer_id {
                let conflict = DnsConflict {
                    domain: record.domain.clone(),
                    active_owner_peer_id: existing.record.owner_peer_id.clone(),
                    conflicting_owner_peer_id: record.owner_peer_id.clone(),
                    active_record_fingerprint: existing.record.fingerprint()?,
                    conflicting_record_fingerprint: record.fingerprint()?,
                    detected_at_unix_ms: now,
                    reason: "duplicate ownership advertisement".to_string(),
                };
                let conflicts = state.conflicts.entry(domain_key).or_default();
                if !conflicts.iter().any(|entry| {
                    entry.conflicting_record_fingerprint == conflict.conflicting_record_fingerprint
                }) {
                    conflicts.push(conflict.clone());
                }
                state.updated_unix_ms = now;
                persist_json(&self.snapshot_path, &*state)?;
                return Ok(DnsApplyOutcome::Conflict {
                    active: existing.record,
                    incoming: record,
                    conflict,
                });
            } else if existing.record.created_at_unix_ms >= record.created_at_unix_ms {
                return Ok(DnsApplyOutcome::Duplicate(existing.record));
            }
        }

        let entry = DnsCacheEntry {
            record: record.clone(),
            verified: true,
            source,
            cached_at_unix_ms: now,
        };
        let outcome = if state.records.contains_key(&domain_key) {
            DnsApplyOutcome::Updated(record.clone())
        } else {
            DnsApplyOutcome::Published(record.clone())
        };
        state.records.insert(domain_key, entry);
        state.updated_unix_ms = now;
        persist_json(&self.snapshot_path, &*state)?;
        Ok(outcome)
    }

    pub async fn purge_expired(&self) -> Result<Vec<DnsRecord>, VoidDnsError> {
        let mut state = self.state.write().await;
        let now = unix_millis();
        let mut expired = Vec::new();
        state.records.retain(|_, entry| {
            if entry.record.is_expired(now) {
                expired.push(entry.record.clone());
                false
            } else {
                true
            }
        });
        if !expired.is_empty() {
            state.updated_unix_ms = now;
            persist_json(&self.snapshot_path, &*state)?;
        }
        Ok(expired)
    }

    pub async fn list_records(&self) -> Vec<DnsCacheEntry> {
        let state = self.state.read().await;
        state.records.values().cloned().collect()
    }

    pub async fn list_conflicts(&self) -> Vec<DnsConflict> {
        let state = self.state.read().await;
        state
            .conflicts
            .values()
            .flat_map(|items| items.iter().cloned())
            .collect()
    }

    pub async fn inspect(&self, domain: &VoidDomain) -> Result<DnsInspection, VoidDnsError> {
        let now = unix_millis();
        let state = self.state.read().await;
        let key = domain.to_string();
        let active_record = state.records.get(&key).cloned();
        Ok(DnsInspection {
            domain: domain.clone(),
            ttl_remaining_secs: active_record
                .as_ref()
                .map(|entry| entry.record.ttl_remaining_secs(now)),
            active_record,
            conflicts: state.conflicts.get(&key).cloned().unwrap_or_default(),
        })
    }

    pub async fn resolve_route(
        &self,
        uri: &VoidUri,
    ) -> Result<Option<DnsResolvedRoute>, VoidDnsError> {
        <Self as VoidDnsResolver>::resolve_route(self, uri).await
    }

    pub fn propagation_message(record: DnsRecord) -> Result<DnsPropagationMessage, VoidDnsError> {
        Ok(DnsPropagationMessage {
            event_id: record.fingerprint()?,
            issued_at_unix_ms: unix_millis(),
            record,
        })
    }
}

#[async_trait]
impl VoidDnsResolver for PersistentVoidDns {
    async fn resolve(&self, domain: &VoidDomain) -> Result<Option<DnsRecord>, VoidDnsError> {
        let _ = self.purge_expired().await?;
        let state = self.state.read().await;
        Ok(state
            .records
            .get(domain.as_str())
            .map(|entry| entry.record.clone()))
    }
}

pub const DNS_COMMANDS_DIR: &str = "commands";

pub fn enqueue_dns_command(
    data_dir: impl AsRef<Path>,
    command: DnsCommand,
) -> Result<PathBuf, VoidDnsError> {
    let root_dir = data_dir.as_ref().join("dns");
    let commands_dir = root_dir.join(DNS_COMMANDS_DIR);
    fs::create_dir_all(&commands_dir)?;
    let envelope = QueuedDnsCommand {
        command_id: random_token(12),
        issued_at_unix_ms: unix_millis(),
        command,
    };
    let path = commands_dir.join(format!(
        "{}-{}.json",
        envelope.issued_at_unix_ms, envelope.command_id
    ));
    persist_json(&path, &envelope)?;
    Ok(path)
}

pub fn drain_dns_commands(
    data_dir: impl AsRef<Path>,
) -> Result<Vec<QueuedDnsCommand>, VoidDnsError> {
    let commands_dir = data_dir.as_ref().join("dns").join(DNS_COMMANDS_DIR);
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
        let command: QueuedDnsCommand = serde_json::from_slice(&fs::read(&path)?)?;
        fs::remove_file(&path)?;
        commands.push(command);
    }
    Ok(commands)
}

pub type LocalVoidDns = PersistentVoidDns;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct UnsignedDnsRecord {
    domain: VoidDomain,
    owner_peer_id: String,
    target_peer_id: String,
    runtime_surface: String,
    capabilities: Vec<String>,
    created_at_unix_ms: u128,
    expires_at_unix_ms: u128,
    public_key: String,
}

#[derive(Debug, Error)]
pub enum VoidDnsError {
    #[error("VOID DNS IO failed: {0}")]
    Io(#[from] io::Error),
    #[error("VOID DNS JSON failed: {0}")]
    Json(#[from] serde_json::Error),
    #[error("invalid .void domain: {0}")]
    InvalidDomain(String),
    #[error("VOID DNS record failed signature verification for domain {0}")]
    InvalidSignature(String),
    #[error("VOID DNS record expired for domain {0}")]
    ExpiredRecord(String),
    #[error("VOID DNS lookup failed: {0}")]
    Lookup(String),
    #[error("VOID DNS identity failed: {0}")]
    Identity(#[from] void_identity::IdentityError),
}

fn persist_json<T: Serialize>(path: &Path, value: &T) -> Result<(), VoidDnsError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, serde_json::to_vec_pretty(value)?)?;
    Ok(())
}

fn load_json_or_default<T>(path: &Path) -> Result<T, VoidDnsError>
where
    T: DeserializeOwned + Default,
{
    if !path.exists() {
        return Ok(T::default());
    }
    Ok(serde_json::from_slice(&fs::read(path)?)?)
}

fn normalize_capabilities(mut capabilities: Vec<String>) -> Vec<String> {
    capabilities.sort();
    capabilities.dedup();
    capabilities
}

fn random_token(len: usize) -> String {
    let source = format!("{}-{}", unix_millis(), std::process::id());
    blake3::hash(source.as_bytes()).to_hex()[..len.min(64)].to_string()
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

    #[tokio::test]
    async fn resolves_signed_local_record() {
        let dir = unique_test_dir("resolve");
        let identity = PersistentNodeIdentity::load_or_create_dir(dir.join("identity")).unwrap();
        let dns = PersistentVoidDns::load_or_create(&dir).unwrap();
        let domain = VoidDomain::new("chat.void").unwrap();

        let (record, outcome) = dns
            .publish_record(
                &identity,
                domain.clone(),
                identity.peer_id_string(),
                "void-chat",
                vec!["chat/direct-e2ee".into(), "service/chat".into()],
                Duration::from_secs(60),
            )
            .await
            .unwrap();

        assert!(matches!(outcome, DnsApplyOutcome::Published(_)));
        assert_eq!(dns.resolve(&domain).await.unwrap(), Some(record));
    }

    #[tokio::test]
    async fn detects_conflicting_ownership() {
        let dir = unique_test_dir("conflict");
        let owner_a = PersistentNodeIdentity::load_or_create_dir(dir.join("owner-a")).unwrap();
        let owner_b = PersistentNodeIdentity::load_or_create_dir(dir.join("owner-b")).unwrap();
        let dns = PersistentVoidDns::load_or_create(&dir).unwrap();
        let domain = VoidDomain::new("vault.void").unwrap();

        dns.publish_record(
            &owner_a,
            domain.clone(),
            owner_a.peer_id_string(),
            "void-vault",
            vec!["service/storage".into()],
            Duration::from_secs(120),
        )
        .await
        .unwrap();

        let conflicting = DnsRecord::sign(
            &owner_b,
            domain.clone(),
            owner_b.peer_id_string(),
            "void-vault",
            vec!["service/storage".into()],
            Duration::from_secs(120),
        )
        .unwrap();
        let outcome = dns
            .apply_record(conflicting.clone(), DnsRecordSource::MeshPropagation)
            .await
            .unwrap();

        assert!(matches!(outcome, DnsApplyOutcome::Conflict { .. }));
        let inspection = dns.inspect(&domain).await.unwrap();
        assert_eq!(inspection.conflicts.len(), 1);
        assert_eq!(
            inspection.active_record.unwrap().record.owner_peer_id,
            owner_a.peer_id_string()
        );
    }

    #[tokio::test]
    async fn resolves_void_route() {
        let dir = unique_test_dir("route");
        let identity = PersistentNodeIdentity::load_or_create_dir(dir.join("identity")).unwrap();
        let dns = PersistentVoidDns::load_or_create(&dir).unwrap();
        dns.publish_record(
            &identity,
            VoidDomain::new("room.core.void").unwrap(),
            identity.peer_id_string(),
            "void-chat",
            vec!["service/chat".into(), "room/membership".into()],
            Duration::from_secs(120),
        )
        .await
        .unwrap();

        let uri: VoidUri = "void://room.core.void/rooms/main".parse().unwrap();
        let resolved = dns.resolve_route(&uri).await.unwrap().unwrap();

        assert_eq!(resolved.target_peer_id, identity.peer_id_string());
        assert_eq!(resolved.runtime_surface, "void-chat");
        assert!(resolved.capabilities.contains(&"service/chat".to_string()));
    }

    fn unique_test_dir(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!("voidnet-dns-{label}-{}", unix_millis()))
    }
}
