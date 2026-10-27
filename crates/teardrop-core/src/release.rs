//! Release-signing helpers shared by the updater and the release tooling:
//! SHA-256 checksums plus ed25519 signatures over `SHA256SUMS.txt`.
//!
//! The release workflow signs the checksum file with a private seed kept in
//! the `DROPPY_SIGNING_KEY` Actions secret; the app ships only the public key
//! (`update::UPDATE_PUBKEY`) and refuses installs it can't verify.

use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
use sha2::{Digest, Sha256};

pub fn sha256_hex(data: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(data);
    h.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

fn hex_decode(s: &str) -> Option<Vec<u8>> {
    if s.len() % 2 != 0 || !s.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).ok()).collect()
}

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn signing_key(seed_hex: &str) -> Option<SigningKey> {
    let seed = hex_decode(seed_hex).filter(|b| b.len() == 32)?;
    Some(SigningKey::from_bytes(&seed.try_into().ok()?))
}

/// True when `sig_text` (128 hex chars, surrounding whitespace allowed) is a
/// valid ed25519 signature of `msg` under `pubkey_hex` (64 hex chars).
pub fn verify_signature(pubkey_hex: &str, msg: &[u8], sig_text: &str) -> bool {
    let Some(pk) = hex_decode(pubkey_hex).filter(|b| b.len() == 32) else {
        return false;
    };
    let Some(sig) = hex_decode(sig_text.trim()).filter(|b| b.len() == 64) else {
        return false;
    };
    let Ok(key) = VerifyingKey::from_bytes(&pk.try_into().unwrap()) else {
        return false;
    };
    key.verify_strict(msg, &Signature::from_bytes(&sig.try_into().unwrap())).is_ok()
}

/// ed25519 signature of `msg` under the 32-byte seed (hex), as 128 lowercase hex.
pub fn sign(seed_hex: &str, msg: &[u8]) -> Option<String> {
    signing_key(seed_hex).map(|k| hex_encode(&k.sign(msg).to_bytes()))
}

/// Public key for the 32-byte seed (hex), as 64 lowercase hex chars.
pub fn public_key_hex(seed_hex: &str) -> Option<String> {
    signing_key(seed_hex).map(|k| hex_encode(&k.verifying_key().to_bytes()))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SEED: &str = "0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f20";
    const SEED2: &str = "ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff";

    #[test]
    fn sha256_known_vectors() {
        assert_eq!(sha256_hex(b""), "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855");
        assert_eq!(sha256_hex(b"abc"), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
        let long = vec![b'a'; 1000];
        assert_eq!(sha256_hex(&long).len(), 64);
    }

    #[test]
    fn sign_verify_roundtrip() {
        let pk = public_key_hex(SEED).unwrap();
        assert_eq!(pk.len(), 64);
        let sig = sign(SEED, b"SHA256SUMS.txt contents").unwrap();
        assert_eq!(sig.len(), 128);
        assert!(verify_signature(&pk, b"SHA256SUMS.txt contents", &sig));
    }

    #[test]
    fn tampered_message_and_wrong_key_fail() {
        let pk = public_key_hex(SEED).unwrap();
        let pk2 = public_key_hex(SEED2).unwrap();
        let sig = sign(SEED, b"sums").unwrap();
        assert!(!verify_signature(&pk, b"sums with a twist", &sig));
        assert!(!verify_signature(&pk2, b"sums", &sig));
    }

    #[test]
    fn malformed_inputs_fail() {
        let pk = public_key_hex(SEED).unwrap();
        let sig = sign(SEED, b"sums").unwrap();
        assert!(!verify_signature("", b"sums", &sig), "empty pubkey");
        assert!(!verify_signature("zz", b"sums", &sig), "bad hex");
        assert!(!verify_signature(&pk[..32], b"sums", &sig), "short pubkey");
        assert!(!verify_signature(&pk, b"sums", &sig[..64]), "short sig");
        assert!(!verify_signature(&pk, b"sums", "not-hex"));
        assert!(verify_signature(&pk, b"sums", &format!("{sig}\n")), "trailing newline ok");
    }
}
