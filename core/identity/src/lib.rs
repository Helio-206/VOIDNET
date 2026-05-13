use base64::{engine::general_purpose::STANDARD_NO_PAD, Engine as _};
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use libp2p::{identity, PeerId};
use rand_core::OsRng;
use serde::{Deserialize, Serialize};
use std::{
    env, fmt, fs, io,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};
use thiserror::Error;

pub const DEFAULT_NODE_DIR: &str = ".voidnet/node";
pub const LIBP2P_IDENTITY_FILE: &str = "identity.libp2p";

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct VoidPeerId(String);

impl VoidPeerId {
    pub fn from_public_key(public_key: &VerifyingKey) -> Self {
        let hash = blake3::hash(public_key.as_bytes());
        Self(format!("vpid1{}", hash.to_hex()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for VoidPeerId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

#[derive(Clone)]
pub struct NodeIdentity {
    signing_key: SigningKey,
    verifying_key: VerifyingKey,
    peer_id: VoidPeerId,
}

impl NodeIdentity {
    pub fn generate() -> Self {
        let signing_key = SigningKey::generate(&mut OsRng);
        Self::from_signing_key(signing_key)
    }

    pub fn from_signing_key(signing_key: SigningKey) -> Self {
        let verifying_key = signing_key.verifying_key();
        let peer_id = VoidPeerId::from_public_key(&verifying_key);
        Self {
            signing_key,
            verifying_key,
            peer_id,
        }
    }

    pub fn peer_id(&self) -> &VoidPeerId {
        &self.peer_id
    }

    pub fn public_key_bytes(&self) -> [u8; 32] {
        self.verifying_key.to_bytes()
    }

    pub fn public_key_base64(&self) -> String {
        STANDARD_NO_PAD.encode(self.public_key_bytes())
    }

    pub fn sign(&self, payload: impl Into<Vec<u8>>) -> SignedPayload {
        let payload = payload.into();
        let signature = self.signing_key.sign(&payload);
        SignedPayload {
            peer_id: self.peer_id.to_string(),
            public_key: self.public_key_base64(),
            payload,
            signature: STANDARD_NO_PAD.encode(signature.to_bytes()),
        }
    }

    pub fn verify(signed: &SignedPayload) -> Result<(), IdentityError> {
        let public_key_bytes: [u8; 32] = STANDARD_NO_PAD
            .decode(signed.public_key.as_bytes())?
            .try_into()
            .map_err(|_| IdentityError::InvalidPublicKeyLength)?;
        let signature_bytes: [u8; 64] = STANDARD_NO_PAD
            .decode(signed.signature.as_bytes())?
            .try_into()
            .map_err(|_| IdentityError::InvalidSignatureLength)?;

        let verifying_key = VerifyingKey::from_bytes(&public_key_bytes)?;
        let expected_peer_id = VoidPeerId::from_public_key(&verifying_key);
        if expected_peer_id.as_str() != signed.peer_id.as_str() {
            return Err(IdentityError::PeerIdMismatch {
                expected: expected_peer_id.to_string(),
                actual: signed.peer_id.clone(),
            });
        }

        let signature = Signature::from_bytes(&signature_bytes);
        verifying_key.verify(&signed.payload, &signature)?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SignedPayload {
    pub peer_id: String,
    pub public_key: String,
    pub payload: Vec<u8>,
    pub signature: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct StoredIdentity {
    peer_id: String,
    public_key: String,
    secret_key: String,
}

pub struct IdentityStore;

impl IdentityStore {
    pub fn load(path: impl AsRef<Path>) -> Result<NodeIdentity, IdentityError> {
        let raw = fs::read_to_string(path)?;
        let stored: StoredIdentity = serde_json::from_str(&raw)?;
        let secret_key_bytes: [u8; 32] = STANDARD_NO_PAD
            .decode(stored.secret_key.as_bytes())?
            .try_into()
            .map_err(|_| IdentityError::InvalidSecretKeyLength)?;
        let identity = NodeIdentity::from_signing_key(SigningKey::from_bytes(&secret_key_bytes));

        if identity.peer_id().as_str() != stored.peer_id.as_str() {
            return Err(IdentityError::PeerIdMismatch {
                expected: identity.peer_id().to_string(),
                actual: stored.peer_id,
            });
        }

        if identity.public_key_base64() != stored.public_key {
            return Err(IdentityError::PublicKeyMismatch);
        }

        Ok(identity)
    }

    pub fn save(path: impl AsRef<Path>, identity: &NodeIdentity) -> Result<(), IdentityError> {
        if let Some(parent) = path.as_ref().parent() {
            fs::create_dir_all(parent)?;
        }

        let stored = StoredIdentity {
            peer_id: identity.peer_id().to_string(),
            public_key: identity.public_key_base64(),
            secret_key: STANDARD_NO_PAD.encode(identity.signing_key.to_bytes()),
        };

        let raw = serde_json::to_string_pretty(&stored)?;
        fs::write(path, raw)?;
        Ok(())
    }

    pub fn load_or_create(path: impl AsRef<Path>) -> Result<NodeIdentity, IdentityError> {
        let path = path.as_ref();
        if path.exists() {
            return Self::load(path);
        }

        let identity = NodeIdentity::generate();
        Self::save(path, &identity)?;
        Ok(identity)
    }
}

#[derive(Clone)]
pub struct PersistentNodeIdentity {
    keypair: identity::Keypair,
    peer_id: PeerId,
    fingerprint: String,
    path: PathBuf,
    generated: bool,
}

impl PersistentNodeIdentity {
    pub fn load_or_create_default() -> Result<Self, IdentityError> {
        Self::load_or_create_dir(default_node_dir())
    }

    pub fn load_or_create_dir(data_dir: impl AsRef<Path>) -> Result<Self, IdentityError> {
        let data_dir = data_dir.as_ref();
        fs::create_dir_all(data_dir)?;

        let path = data_dir.join(LIBP2P_IDENTITY_FILE);
        if path.exists() {
            let bytes = fs::read(&path)?;
            let keypair = identity::Keypair::from_protobuf_encoding(&bytes)
                .map_err(|error| IdentityError::NetworkIdentity(error.to_string()))?;
            return Self::from_keypair(path, keypair, false);
        }

        let keypair = identity::Keypair::generate_ed25519();
        let bytes = keypair
            .to_protobuf_encoding()
            .map_err(|error| IdentityError::NetworkIdentity(error.to_string()))?;
        fs::write(&path, bytes)?;
        Self::from_keypair(path, keypair, true)
    }

    fn from_keypair(
        path: PathBuf,
        keypair: identity::Keypair,
        generated: bool,
    ) -> Result<Self, IdentityError> {
        let public_key = keypair.public();
        let peer_id = public_key.to_peer_id();
        let fingerprint = fingerprint_public_key(&public_key);

        Ok(Self {
            keypair,
            peer_id,
            fingerprint,
            path,
            generated,
        })
    }

    pub fn keypair(&self) -> &identity::Keypair {
        &self.keypair
    }

    pub fn peer_id(&self) -> PeerId {
        self.peer_id.clone()
    }

    pub fn peer_id_string(&self) -> String {
        self.peer_id.to_string()
    }

    pub fn fingerprint(&self) -> &str {
        &self.fingerprint
    }

    pub fn public_key_protobuf(&self) -> Vec<u8> {
        self.keypair.public().encode_protobuf()
    }

    pub fn public_key_protobuf_base64(&self) -> String {
        STANDARD_NO_PAD.encode(self.public_key_protobuf())
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn generated(&self) -> bool {
        self.generated
    }

    pub fn sign_bytes(&self, payload: &[u8]) -> Result<Vec<u8>, IdentityError> {
        self.keypair
            .sign(payload)
            .map_err(|error| IdentityError::NetworkIdentity(error.to_string()))
    }

    pub fn verify_bytes(
        peer_id: &str,
        public_key: &str,
        payload: &[u8],
        signature: &str,
    ) -> Result<(), IdentityError> {
        let public_key_bytes = STANDARD_NO_PAD.decode(public_key.as_bytes())?;
        let public_key = identity::PublicKey::try_decode_protobuf(&public_key_bytes)
            .map_err(|error| IdentityError::NetworkIdentity(error.to_string()))?;
        let expected_peer_id = public_key.to_peer_id().to_string();
        if expected_peer_id != peer_id {
            return Err(IdentityError::PeerIdMismatch {
                expected: expected_peer_id,
                actual: peer_id.to_string(),
            });
        }

        let signature_bytes = STANDARD_NO_PAD.decode(signature.as_bytes())?;
        if !public_key.verify(payload, &signature_bytes) {
            return Err(IdentityError::InvalidNetworkSignature);
        }

        Ok(())
    }

    pub fn sign_presence(
        &self,
        agent: impl Into<String>,
    ) -> Result<SignedPeerPresence, IdentityError> {
        let agent = agent.into();
        let issued_unix_ms = unix_millis();
        let nonce = blake3::hash(format!("{}:{issued_unix_ms}", self.peer_id).as_bytes())
            .to_hex()
            .to_string();
        let public_key = self.keypair.public().encode_protobuf();
        let public_key = STANDARD_NO_PAD.encode(public_key);
        let payload = presence_payload(&self.peer_id.to_string(), &agent, issued_unix_ms, &nonce);
        let signature = self
            .keypair
            .sign(payload.as_bytes())
            .map_err(|error| IdentityError::NetworkIdentity(error.to_string()))?;

        Ok(SignedPeerPresence {
            peer_id: self.peer_id.to_string(),
            public_key,
            agent,
            issued_unix_ms,
            nonce,
            signature: STANDARD_NO_PAD.encode(signature),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SignedPeerPresence {
    pub peer_id: String,
    pub public_key: String,
    pub agent: String,
    pub issued_unix_ms: u128,
    pub nonce: String,
    pub signature: String,
}

impl SignedPeerPresence {
    pub fn verify(&self) -> Result<(), IdentityError> {
        let public_key_bytes = STANDARD_NO_PAD.decode(self.public_key.as_bytes())?;
        let public_key = identity::PublicKey::try_decode_protobuf(&public_key_bytes)
            .map_err(|error| IdentityError::NetworkIdentity(error.to_string()))?;
        let expected_peer_id = public_key.to_peer_id().to_string();
        if expected_peer_id != self.peer_id {
            return Err(IdentityError::PeerIdMismatch {
                expected: expected_peer_id,
                actual: self.peer_id.clone(),
            });
        }

        let signature = STANDARD_NO_PAD.decode(self.signature.as_bytes())?;
        let payload = presence_payload(
            &self.peer_id,
            &self.agent,
            self.issued_unix_ms,
            &self.nonce,
        );
        if !public_key.verify(payload.as_bytes(), &signature) {
            return Err(IdentityError::InvalidPresenceSignature);
        }

        Ok(())
    }
}

pub fn default_node_dir() -> PathBuf {
    env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
        .join(DEFAULT_NODE_DIR)
}

fn fingerprint_public_key(public_key: &identity::PublicKey) -> String {
    let encoded = public_key.encode_protobuf();
    let hash = blake3::hash(&encoded);
    hash.to_hex().chars().take(16).collect()
}

fn unix_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default()
}

fn presence_payload(peer_id: &str, agent: &str, issued_unix_ms: u128, nonce: &str) -> String {
    format!("voidnet/presence/v1:{peer_id}:{agent}:{issued_unix_ms}:{nonce}")
}

#[derive(Debug, Error)]
pub enum IdentityError {
    #[error("base64 identity data is invalid: {0}")]
    Base64(#[from] base64::DecodeError),
    #[error("ed25519 identity error: {0}")]
    Ed25519(#[from] ed25519_dalek::SignatureError),
    #[error("identity IO failed: {0}")]
    Io(#[from] io::Error),
    #[error("identity JSON failed: {0}")]
    Json(#[from] serde_json::Error),
    #[error("public key has invalid length")]
    InvalidPublicKeyLength,
    #[error("secret key has invalid length")]
    InvalidSecretKeyLength,
    #[error("signature has invalid length")]
    InvalidSignatureLength,
    #[error("peer id mismatch, expected {expected}, got {actual}")]
    PeerIdMismatch { expected: String, actual: String },
    #[error("stored public key does not match stored secret key")]
    PublicKeyMismatch,
    #[error("libp2p identity failed: {0}")]
    NetworkIdentity(String),
    #[error("signed network payload did not verify")]
    InvalidNetworkSignature,
    #[error("signed peer presence did not verify")]
    InvalidPresenceSignature,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signed_payload_verifies() {
        let identity = NodeIdentity::generate();
        let signed = identity.sign(b"voidnet".to_vec());

        NodeIdentity::verify(&signed).unwrap();
    }

    #[test]
    fn signed_presence_verifies() {
        let path = PathBuf::from("/tmp/voidnet-presence-test");
        let keypair = identity::Keypair::generate_ed25519();
        let identity = PersistentNodeIdentity::from_keypair(path, keypair, true).unwrap();
        let presence = identity.sign_presence("voidnet-test").unwrap();

        presence.verify().unwrap();
    }
}
