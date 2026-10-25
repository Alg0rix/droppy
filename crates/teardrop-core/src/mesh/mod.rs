//! BLE mesh chat protocol (bitchat-style, whitepaper v2.0).
//!
//! Pure logic, no OS dependencies: packet codec, Ed25519 identity, Noise XX
//! sessions, controlled-flood relaying, fragmentation and presence. The
//! Windows crate supplies links (GATT connections) and wall-clock time; this
//! module decides what goes out, when, and what it means.
//!
//! Layout per whitepaper section:
//! * §3 identity   — `Identity`, `PeerId`
//! * §4.1 packets  — `Packet`, `PacketType` (encode/decode, sign, pad)
//! * §4.2 dedup    — `Seen`
//! * §4.4 chunks   — `fragment`, `Reassembler`
//! * §5.1 sessions — `Session` (Noise XX via snow)
//! * node          — `Node` glues it all: announce, relay, DM queueing

mod fragment;
mod identity;
mod node;
mod noise;
mod packet;
mod seen;
mod sync;

pub use fragment::{fragment_packet, Reassembler, FRAG_CHUNK_MAX};
pub use identity::{Identity, PeerId};
pub use node::{Event, LinkId, Node, Op, PeerInfo, SendTo};
pub use noise::Session;
pub use packet::{Packet, PacketType};
pub use seen::Seen;

/// Packets originate with this TTL (whitepaper §4.2).
pub const ORIGIN_TTL: u8 = 7;
/// Announce keeps a peer "reachable" this long (whitepaper §4.5).
pub const PEER_FRESH_MS: u64 = 60_000;
/// A peer entry is dropped entirely this long after its last announce.
pub const PEER_DROP_MS: u64 = 10 * 60_000;

pub fn hex(b: &[u8]) -> String {
    const D: &[u8; 16] = b"0123456789abcdef";
    let mut s = String::with_capacity(b.len() * 2);
    for &x in b {
        s.push(D[(x >> 4) as usize] as char);
        s.push(D[(x & 15) as usize] as char);
    }
    s
}

pub fn unhex(s: &str) -> Option<Vec<u8>> {
    if s.len() % 2 != 0 || !s.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    Some(
        (0..s.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&s[i..i + 2], 16).ok())
            .collect::<Option<Vec<u8>>>()?,
    )
}
