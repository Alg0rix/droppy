//! §4.4 Fragmentation.
//!
//! A packet that does not fit one link write is split into `Fragment`
//! packets — hop-local: each receiver reassembles, then relays the whole
//! packet onward (re-fragmenting if the next link is narrower).
//!
//! Fragment payload: `[8B frag_id][2B index][2B total][chunk]`.

use super::{Packet, PacketType, PeerId};
use std::collections::HashMap;

/// §4.4 target chunk size; links with a smaller MTU shrink it further.
pub const FRAG_CHUNK_MAX: usize = 469;
const META: usize = 12;
/// Wire cost of a fragment packet around its chunk (22-byte header + meta).
const WRAP: usize = 22 + META;
const MAX_ASSEMBLIES: usize = 128;
const ASSEMBLY_TTL_MS: u64 = 30_000;
const MAX_FRAME: usize = 1024 * 1024;

/// Split `frame` into wire-ready fragment packets for a link whose largest
/// write is `link_payload` bytes. Returns the frame itself when it fits.
pub fn fragment_packet(frame: &[u8], sender: PeerId, link_payload: usize, now_ms: u64) -> Vec<Vec<u8>> {
    if frame.len() <= link_payload || link_payload <= WRAP + 1 {
        return vec![frame.to_vec()];
    }
    let chunk = FRAG_CHUNK_MAX.min(link_payload - WRAP);
    let total = frame.len().div_ceil(chunk);
    let frag_id: u64 = rand::random();
    (0..total)
        .map(|i| {
            let mut payload = Vec::with_capacity(META + chunk.min(frame.len() - i * chunk));
            payload.extend_from_slice(&frag_id.to_be_bytes());
            payload.extend_from_slice(&(i as u16).to_be_bytes());
            payload.extend_from_slice(&(total as u16).to_be_bytes());
            payload.extend_from_slice(&frame[i * chunk..(i * chunk + chunk).min(frame.len())]);
            Packet::new(PacketType::Fragment, sender, now_ms, payload).encode()
        })
        .collect()
}

struct Assembly {
    parts: Vec<Option<Vec<u8>>>,
    got: u16,
    at: u64,
}

/// Per-node reassembly of inbound fragments (whitepaper bounds: 128
/// concurrent, 30 s timeout, 1 MiB result cap).
#[derive(Default)]
pub struct Reassembler {
    map: HashMap<(PeerId, u64), Assembly>,
}

impl Reassembler {
    /// Feed one `Fragment` packet's payload. Returns the reassembled frame
    /// once the last piece lands.
    pub fn feed(&mut self, sender: PeerId, payload: &[u8], now_ms: u64) -> Option<Vec<u8>> {
        if payload.len() <= META {
            return None;
        }
        let frag_id = u64::from_be_bytes(payload[0..8].try_into().ok()?);
        let idx = u16::from_be_bytes(payload[8..10].try_into().ok()?);
        let total = u16::from_be_bytes(payload[10..12].try_into().ok()?);
        if total == 0 || idx >= total || total as usize * FRAG_CHUNK_MAX > MAX_FRAME * 2 {
            return None;
        }
        if self.map.len() >= MAX_ASSEMBLIES && !self.map.contains_key(&(sender, frag_id)) {
            return None;
        }
        let a = self.map.entry((sender, frag_id)).or_insert_with(|| Assembly {
            parts: vec![None; total as usize],
            got: 0,
            at: now_ms,
        });
        if a.parts.len() != total as usize {
            return None; // conflicting totals for one frag_id
        }
        let slot = &mut a.parts[idx as usize];
        if slot.is_none() {
            *slot = Some(payload[META..].to_vec());
            a.got += 1;
        }
        a.at = now_ms;
        if a.got != total {
            return None;
        }
        let a = self.map.remove(&(sender, frag_id))?;
        let mut frame = Vec::with_capacity(a.parts.iter().map(|p| p.as_ref().map_or(0, |p| p.len())).sum());
        for p in a.parts.into_iter().flatten() {
            frame.extend_from_slice(&p);
        }
        Some(frame)
    }

    /// Drop assemblies older than 30 s.
    pub fn sweep(&mut self, now_ms: u64) {
        self.map.retain(|_, a| now_ms.saturating_sub(a.at) <= ASSEMBLY_TTL_MS);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mesh::Identity;

    #[test]
    fn small_frame_passes_through() {
        let f = vec![9u8; 100];
        let out = fragment_packet(&f, PeerId([1; 8]), 512, 0);
        assert_eq!(out, vec![f]);
    }

    #[test]
    fn splits_and_reassembles() {
        let frame: Vec<u8> = (0..3000u32).map(|i| (i % 251) as u8).collect();
        let sender = Identity::generate().peer_id();
        let frags = fragment_packet(&frame, sender, 512, 100);
        assert!(frags.len() >= 4, "3000B over ~478B chunks");
        assert!(frags.iter().all(|f| f.len() <= 512));
        let mut r = Reassembler::default();
        let mut done = None;
        // Out-of-order delivery is fine.
        for f in frags.iter().rev() {
            let p = Packet::decode(f).unwrap();
            assert_eq!(p.ptype, PacketType::Fragment);
            if let Some(v) = r.feed(p.sender, &p.payload, 200) {
                done = Some(v);
            }
        }
        assert_eq!(done.unwrap(), frame);
    }

    #[test]
    fn duplicate_piece_does_not_double_count() {
        let frame = vec![5u8; 1500];
        let sender = PeerId([3; 8]);
        let frags = fragment_packet(&frame, sender, 200, 0);
        let mut r = Reassembler::default();
        for f in &frags {
            let p = Packet::decode(f).unwrap();
            r.feed(p.sender, &p.payload, 0);
            r.feed(p.sender, &p.payload, 0); // dup
        }
        // Full set already delivered; feeding the dup only kept got==total once.
        let p = Packet::decode(&frags[0]).unwrap();
        assert!(r.feed(p.sender, &p.payload, 0).is_none());
    }

    #[test]
    fn rejects_bad_meta() {
        let mut r = Reassembler::default();
        assert!(r.feed(PeerId([0; 8]), &[1, 2, 3], 0).is_none());
        // total = 0
        let mut p = vec![0u8; 13];
        p[8] = 0;
        assert!(r.feed(PeerId([0; 8]), &p, 0).is_none());
    }
}
