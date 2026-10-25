//! §4.1 Packet format — wire-compatible with BitChat's `BinaryProtocol` v1
//! (`localPackages/BitFoundation`):
//!
//! ```text
//! [0]    version u8 (=1; v2 adds u32 length + route, accepted inbound)
//! [1]    type u8
//! [2]    ttl u8                       — excluded from the signature
//! [3:11] timestamp u64 BE (ms)
//! [11]   flags u8 (bit0 has_recipient, bit1 has_signature,
//!                  bit2 is_compressed, bit3 has_route (v2), bit4 is_rsr)
//! [12:14] payload_len u16 BE
//! [14:22] sender_id [8]
//! [22:30] recipient_id [8]            (flag0)
//! [..]   payload (compressed: [orig_len u16][zlib data])
//! [..]   signature [64]               (flag1)
//! ```
//!
//! Only `NoiseHandshake` and `NoiseEncrypted` frames are padded, toward
//! 256/512/1024/2048-byte buckets (PKCS#7-style: pad bytes equal the pad
//! length; frames needing >255 pad bytes go out unpadded).

use super::PeerId;
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use sha2::{Digest, Sha256};

pub const VERSION: u8 = 1;
const F_RECIPIENT: u8 = 0x01;
const F_SIGNATURE: u8 = 0x02;
const F_COMPRESSED: u8 = 0x04;
const F_ROUTE: u8 = 0x08;
// Bit 4 (0x10) is BitChat's RSR flag — mutable, excluded from signatures;
// we never set it.
const PAD_BUCKETS: &[usize] = &[256, 512, 1024, 2048];
/// Reassembled frames can reach ~1 MiB; raw frames stay under one MTU.
pub const MAX_PACKET: usize = 4 * 1024 * 1024;
/// Largest payload we will inflate (decompression-bomb guard).
const MAX_INFLATED: usize = 2 * 1024 * 1024;

/// BitChat wire types (MessageType.swift). Unknown types decode as `Other`
/// so the node can drop them without aborting the whole frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PacketType {
    /// Signed presence: nick + both public keys + neighbour ids.
    Announce,
    /// Signed public broadcast message.
    Message,
    /// "I'm leaving" notification.
    Leave,
    /// One Noise XX handshake frame (payload = raw Noise message).
    NoiseHandshake,
    /// Session ciphertext (typed private payload inside).
    NoiseEncrypted,
    /// One chunk of a larger packet (see `fragment`).
    Fragment,
    /// Anything else on the BitChat type space — decoded then dropped.
    Other(u8),
}

impl PacketType {
    fn from_u8(v: u8) -> Self {
        match v {
            0x01 => Self::Announce,
            0x02 => Self::Message,
            0x03 => Self::Leave,
            0x10 => Self::NoiseHandshake,
            0x11 => Self::NoiseEncrypted,
            0x20 => Self::Fragment,
            v => Self::Other(v),
        }
    }

    pub fn as_u8(self) -> u8 {
        match self {
            Self::Announce => 0x01,
            Self::Message => 0x02,
            Self::Leave => 0x03,
            Self::NoiseHandshake => 0x10,
            Self::NoiseEncrypted => 0x11,
            Self::Fragment => 0x20,
            Self::Other(v) => v,
        }
    }

    /// §4.1: only Noise frames are padded.
    pub fn padded(self) -> bool {
        matches!(self, Self::NoiseHandshake | Self::NoiseEncrypted)
    }
}

#[derive(Debug, Clone)]
pub struct Packet {
    /// Wire format version (1 or 2; outbound is always v1).
    pub version: u8,
    pub ptype: PacketType,
    pub ttl: u8,
    pub timestamp: u64,
    pub sender: PeerId,
    pub recipient: Option<PeerId>,
    /// Usable payload (decompressed when the wire form was compressed).
    pub payload: Vec<u8>,
    /// Payload exactly as carried on the wire — what the signature covers.
    /// Equals `payload` whenever compression was not applied.
    pub wire_payload: Vec<u8>,
    /// The wire form was zlib-compressed.
    pub compressed: bool,
    /// Explicit route hops (v2 only; signed, so kept for verification).
    pub route: Vec<[u8; 8]>,
    pub signature: Option<[u8; 64]>,
}

impl Packet {
    pub fn new(ptype: PacketType, sender: PeerId, timestamp: u64, payload: Vec<u8>) -> Self {
        Self {
            version: VERSION,
            ptype,
            ttl: super::ORIGIN_TTL,
            timestamp,
            sender,
            recipient: None,
            wire_payload: payload.clone(),
            payload,
            compressed: false,
            route: Vec::new(),
            signature: None,
        }
    }

    /// Wire encoding, padding applied for Noise packet types.
    pub fn encode(&self) -> Vec<u8> {
        let mut out = self.encode_unpadded();
        if self.ptype.padded() {
            pad_frame(&mut out);
        }
        out
    }

    /// Frame without padding (also the bytes a fragment carries chunks of).
    pub fn encode_unpadded(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(24 + self.wire_payload.len() + 66);
        self.encode_head(&mut out, self.ttl);
        out.extend_from_slice(&self.wire_payload);
        if let Some(sig) = &self.signature {
            out.extend_from_slice(sig);
        }
        out
    }

    /// Header + sender + optional recipient (+ route on v2). `ttl` lets the
    /// signature form use 0 while the wire form carries the real value. The
    /// signature flag follows `signature.is_some()` — matching BitChat, whose
    /// signable form is encoded with signature=nil (flag cleared).
    fn encode_head(&self, out: &mut Vec<u8>, ttl: u8) {
        let mut flags = 0u8;
        if self.recipient.is_some() {
            flags |= F_RECIPIENT;
        }
        if self.signature.is_some() {
            flags |= F_SIGNATURE;
        }
        if self.compressed {
            flags |= F_COMPRESSED;
        }
        if self.version == 2 && !self.route.is_empty() {
            flags |= F_ROUTE;
        }
        out.push(self.version);
        out.push(self.ptype.as_u8());
        out.push(ttl);
        out.extend_from_slice(&self.timestamp.to_be_bytes());
        out.push(flags);
        if self.version == 2 {
            out.extend_from_slice(&(self.wire_payload.len() as u32).to_be_bytes());
        } else {
            out.extend_from_slice(&(self.wire_payload.len() as u16).to_be_bytes());
        }
        out.extend_from_slice(&self.sender.0);
        if let Some(r) = &self.recipient {
            out.extend_from_slice(&r.0);
        }
        if self.version == 2 && !self.route.is_empty() {
            out.push(self.route.len() as u8);
            for h in &self.route {
                out.extend_from_slice(h);
            }
        }
    }

    /// The signed bytes: the frame encoded at its own version with TTL=0,
    /// no signature and the RSR bit clear, then PKCS#7-padded to the
    /// 256/512/1024/2048 bucket — BitChat's `toBinaryDataForSigning` runs
    /// `BinaryProtocol.encode` with default padding, so the pad bytes are
    /// part of the signed form. Route stays — it is signed. The payload is
    /// the wire payload: for compressed packets exactly what was hashed.
    fn signable(&self) -> Vec<u8> {
        let mut c = self.clone();
        c.ttl = 0;
        c.signature = None;
        let mut out = c.encode_unpadded();
        pad_frame(&mut out);
        out
    }

    pub fn sign(&mut self, sk: &SigningKey) {
        self.signature = Some(sk.sign(&self.signable()).to_bytes());
    }

    pub fn verify(&self, vk: &VerifyingKey) -> bool {
        let Some(sig) = self.signature else { return false };
        let Ok(sig) = Signature::from_slice(&sig) else { return false };
        vk.verify(&self.signable(), &sig).is_ok()
    }

    /// Decode one wire frame. Returns `None` on malformed input.
    ///
    /// Padded types try the raw frame first (an unpadded emission whose tail
    /// happens to look like pad bytes still parses), then the stripped form.
    pub fn decode(bytes: &[u8]) -> Option<Packet> {
        if bytes.len() < 22 || bytes.len() > MAX_PACKET {
            return None;
        }
        let ptype = PacketType::from_u8(bytes[1]);
        if ptype.padded() {
            Self::parse(bytes).or_else(|| Self::parse(&unpad_frame(bytes)))
        } else {
            Self::parse(bytes)
        }
    }

    fn parse(bytes: &[u8]) -> Option<Packet> {
        if bytes.len() < 22 || (bytes[0] != 1 && bytes[0] != 2) {
            return None;
        }
        let version = bytes[0];
        let ptype = PacketType::from_u8(bytes[1]);
        let ttl = bytes[2];
        let timestamp = u64::from_be_bytes(bytes[3..11].try_into().ok()?);
        let flags = bytes[11];
        let payload_len = if version == 2 {
            if bytes.len() < 16 {
                return None;
            }
            u32::from_be_bytes(bytes[12..16].try_into().ok()?) as usize
        } else {
            u16::from_be_bytes(bytes[12..14].try_into().ok()?) as usize
        };
        let mut off = if version == 2 { 16 } else { 14 };
        if bytes.len() < off + 8 {
            return None;
        }
        let mut sender = [0u8; 8];
        sender.copy_from_slice(&bytes[off..off + 8]);
        off += 8;
        let recipient = if flags & F_RECIPIENT != 0 {
            if bytes.len() < off + 8 {
                return None;
            }
            let mut r = [0u8; 8];
            r.copy_from_slice(&bytes[off..off + 8]);
            off += 8;
            Some(PeerId(r))
        } else {
            None
        };
        let mut route = Vec::new();
        if version == 2 && flags & F_ROUTE != 0 {
            let n = *bytes.get(off)? as usize;
            off += 1;
            if bytes.len() < off + n * 8 {
                return None;
            }
            for _ in 0..n {
                let mut h = [0u8; 8];
                h.copy_from_slice(&bytes[off..off + 8]);
                route.push(h);
                off += 8;
            }
        }
        let sig_len = if flags & F_SIGNATURE != 0 { 64 } else { 0 };
        if bytes.len() != off + payload_len + sig_len {
            return None;
        }
        let wire_payload = bytes[off..off + payload_len].to_vec();
        off += payload_len;
        let signature = if sig_len == 64 {
            let mut s = [0u8; 64];
            s.copy_from_slice(&bytes[off..off + 64]);
            Some(s)
        } else {
            None
        };
        let (payload, compressed) = if flags & F_COMPRESSED != 0 {
            let w = if version == 2 { 4 } else { 2 };
            if wire_payload.len() < w + 1 {
                return None;
            }
            let orig = if version == 2 {
                u32::from_be_bytes(wire_payload[..4].try_into().ok()?) as usize
            } else {
                u16::from_be_bytes(wire_payload[..2].try_into().ok()?) as usize
            };
            if orig > MAX_INFLATED {
                return None;
            }
            use std::io::Read;
            let mut d = flate2::read::ZlibDecoder::new(&wire_payload[w..]);
            let mut out = Vec::with_capacity(orig.min(64 * 1024));
            let mut limited = d.by_ref().take((orig + 1) as u64);
            if limited.read_to_end(&mut out).is_err() {
                return None;
            }
            (out, true)
        } else {
            (wire_payload.clone(), false)
        };
        Some(Packet {
            version,
            ptype,
            ttl,
            timestamp,
            sender: PeerId(sender),
            recipient,
            payload,
            wire_payload,
            compressed,
            route,
            signature,
        })
    }

    /// Dedup identity: sender + timestamp + type + wire payload digest (§4.2).
    pub fn dedup_key(&self) -> [u8; 16] {
        let mut h = Sha256::new();
        h.update(self.sender.0);
        h.update(self.timestamp.to_be_bytes());
        h.update([self.ptype.as_u8()]);
        h.update(&self.wire_payload);
        let d = h.finalize();
        let mut k = [0u8; 16];
        k.copy_from_slice(&d[..16]);
        k
    }
}

/// PKCS#7-style pad toward the next bucket; >255 needed → unpadded (§4.1).
/// Bucket choice matches BitChat's `optimalBlockSize`: smallest block that
/// fits the frame plus 16 bytes of crypto overhead.
fn pad_frame(out: &mut Vec<u8>) {
    let Some(&bucket) = PAD_BUCKETS.iter().find(|&&b| b >= out.len() + 16) else { return };
    let n = bucket - out.len();
    if n > 255 {
        return;
    }
    out.resize(out.len() + n, n as u8);
}

/// Strip a valid pad tail; return the frame unchanged when absent.
fn unpad_frame(bytes: &[u8]) -> Vec<u8> {
    let n = *bytes.last().unwrap_or(&0) as usize;
    if n >= 1 && n <= 255 && n <= bytes.len() && bytes[bytes.len() - n..].iter().all(|&b| b as usize == n) {
        bytes[..bytes.len() - n].to_vec()
    } else {
        bytes.to_vec()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mesh::Identity;

    #[test]
    fn roundtrip_signed_directed() {
        let id = Identity::generate();
        let mut p = Packet::new(PacketType::Message, id.peer_id(), 0x1122_3344_5566, b"hello mesh".to_vec());
        p.recipient = Some(PeerId([9; 8]));
        p.ttl = 4;
        p.sign(&id.signing_key());
        let wire = p.encode();
        let q = Packet::decode(&wire).unwrap();
        assert_eq!(q.ptype, PacketType::Message);
        assert_eq!(q.ttl, 4);
        assert_eq!(q.timestamp, p.timestamp);
        assert_eq!(q.sender, id.peer_id());
        assert_eq!(q.recipient, Some(PeerId([9; 8])));
        assert_eq!(q.payload, b"hello mesh");
        let vk = VerifyingKey::from_bytes(&id.sign_pk).unwrap();
        assert!(q.verify(&vk));
    }

    #[test]
    fn signature_survives_ttl_decrement() {
        let id = Identity::generate();
        let mut p = Packet::new(PacketType::Message, id.peer_id(), 7, vec![1, 2, 3]);
        p.sign(&id.signing_key());
        let wire = p.encode();
        let mut q = Packet::decode(&wire).unwrap();
        q.ttl -= 1; // a relay decremented it
        let vk = VerifyingKey::from_bytes(&id.sign_pk).unwrap();
        assert!(q.verify(&vk), "TTL is excluded from the signature");
        assert!(!q.verify(&VerifyingKey::from_bytes(&Identity::generate().sign_pk).unwrap()));
    }

    #[test]
    fn noise_frames_pad_to_bucket() {
        let id = Identity::generate();
        let p = Packet::new(PacketType::NoiseEncrypted, id.peer_id(), 1, vec![0xAB; 100]);
        let wire = p.encode();
        assert_eq!(wire.len(), 256, "small noise frame pads to 256");
        let q = Packet::decode(&wire).unwrap();
        assert_eq!(q.payload.len(), 100);
        assert!(q.payload.iter().all(|&b| b == 0xAB));
    }

    #[test]
    fn unpadded_when_pad_would_exceed_one_byte() {
        let id = Identity::generate();
        // 22+1700 = 1722 → 2048 needs 326 pad bytes (>255) → natural length.
        let payload = vec![0u8; 1700];
        let p = Packet::new(PacketType::NoiseEncrypted, id.peer_id(), 1, payload.clone());
        let wire = p.encode();
        assert_eq!(wire.len(), 22 + 1700, "no bucket within 255 bytes → unpadded");
        assert_eq!(Packet::decode(&wire).unwrap().payload, payload);
        // Past the largest bucket entirely → always natural length.
        let payload = vec![0u8; 2030];
        let p = Packet::new(PacketType::NoiseEncrypted, id.peer_id(), 1, payload.clone());
        let wire = p.encode();
        assert_eq!(wire.len(), 22 + 2030);
        assert_eq!(Packet::decode(&wire).unwrap().payload, payload);
    }

    #[test]
    fn public_types_stay_natural_length() {
        let id = Identity::generate();
        let p = Packet::new(PacketType::Announce, id.peer_id(), 1, vec![1; 30]);
        assert_eq!(p.encode().len(), 22 + 30);
    }

    #[test]
    fn rejects_garbage() {
        assert!(Packet::decode(&[]).is_none());
        assert!(Packet::decode(&[0u8; 21]).is_none());
        assert!(Packet::decode(&[9u8; 64]).is_none()); // bad version/type
    }
}
