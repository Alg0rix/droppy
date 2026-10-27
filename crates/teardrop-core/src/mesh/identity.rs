//! §3 Identity: a Curve25519 static key (Noise key agreement) plus an
//! Ed25519 signing key. The 8-byte peer ID is the first 8 bytes of the
//! SHA-256 fingerprint of the Noise static key — stable across sessions.

use ed25519_dalek::SigningKey;
use sha2::{Digest, Sha256};
use std::fmt;

/// Short on-air identity: first 8 bytes of SHA-256(noise static key).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PeerId(pub [u8; 8]);

impl PeerId {
    pub fn of_noise_pk(pk: &[u8]) -> Self {
        let h = Sha256::digest(pk);
        let mut id = [0u8; 8];
        id.copy_from_slice(&h[..8]);
        PeerId(id)
    }

    pub fn hex(self) -> String {
        super::hex(&self.0)
    }

    pub fn from_hex(s: &str) -> Option<Self> {
        let b = super::unhex(s)?;
        if b.len() != 8 {
            return None;
        }
        let mut id = [0u8; 8];
        id.copy_from_slice(&b);
        Some(PeerId(id))
    }
}

impl fmt::Display for PeerId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.hex())
    }
}

/// Long-term device identity (whitepaper §3).
#[derive(Clone)]
pub struct Identity {
    /// X25519 static private key (Noise `s`).
    pub noise_sk: [u8; 32],
    /// X25519 static public key.
    pub noise_pk: [u8; 32],
    /// Ed25519 signing key.
    pub sign_sk: [u8; 32],
    /// Ed25519 verifying key.
    pub sign_pk: [u8; 32],
}

impl Identity {
    /// Fresh random identity.
    pub fn generate() -> Self {
        use rand::RngCore;
        let mut rng = rand::rngs::OsRng;
        let mut sign_seed = [0u8; 32];
        rng.fill_bytes(&mut sign_seed);
        let signing = SigningKey::from_bytes(&sign_seed);
        let mut noise_sk = [0u8; 32];
        rng.fill_bytes(&mut noise_sk);
        let noise_pk = noise_pubkey(&noise_sk);
        Self { noise_sk, noise_pk, sign_sk: sign_seed, sign_pk: signing.verifying_key().to_bytes() }
    }

    /// Rebuild from stored secrets (validates the pairing).
    pub fn from_secrets(noise_sk: [u8; 32], sign_sk: [u8; 32]) -> Option<Self> {
        let signing = SigningKey::from_bytes(&sign_sk);
        Some(Self { noise_sk, noise_pk: noise_pubkey(&noise_sk), sign_sk, sign_pk: signing.verifying_key().to_bytes() })
    }

    pub fn peer_id(&self) -> PeerId {
        PeerId::of_noise_pk(&self.noise_pk)
    }

    pub fn signing_key(&self) -> SigningKey {
        SigningKey::from_bytes(&self.sign_sk)
    }
}

/// X25519 public key for a private key, via snow's DH resolver.
fn noise_pubkey(sk: &[u8; 32]) -> [u8; 32] {
    use snow::resolvers::CryptoResolver;
    let mut dh = snow::resolvers::DefaultResolver.resolve_dh(&snow::params::DHChoice::Curve25519).expect("resolver must offer 25519");
    dh.set(sk);
    let mut pk = [0u8; 32];
    pk.copy_from_slice(dh.pubkey());
    pk
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn peer_id_is_sha256_prefix() {
        let id = Identity::generate();
        let pid = id.peer_id();
        assert_eq!(pid.0.to_vec(), Sha256::digest(id.noise_pk)[..8].to_vec());
    }

    #[test]
    fn secrets_roundtrip() {
        let a = Identity::generate();
        let b = Identity::from_secrets(a.noise_sk, a.sign_sk).unwrap();
        assert_eq!(a.noise_pk, b.noise_pk);
        assert_eq!(a.sign_pk, b.sign_pk);
        assert_eq!(a.peer_id(), b.peer_id());
    }

    #[test]
    fn peer_id_hex_roundtrip() {
        let pid = Identity::generate().peer_id();
        assert_eq!(PeerId::from_hex(&pid.hex()), Some(pid));
        assert!(PeerId::from_hex("zz").is_none());
    }
}
