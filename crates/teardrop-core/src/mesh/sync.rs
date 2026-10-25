//! §6.3 gossip sync: Golomb-coded-set filters over packet ids and the
//! REQUEST_SYNC payload, wire-compatible with BitChat
//! (`Sync/GCSFilter.swift`, `Models/RequestSyncPacket.swift`).
//!
//! A peer advertises the ids it holds as a GCS; the other side answers with
//! the packets that are missing from it.

use super::Packet;
use sha2::{Digest, Sha256};

/// Highest Golomb-Rice parameter accepted from the wire.
pub const MAX_P: u32 = 32;
/// Sync-type bits (SyncTypeFlags): announce = bit 0, message = bit 1.
pub const TYPE_ANNOUNCE: u64 = 1 << 0;
pub const TYPE_MESSAGE: u64 = 1 << 1;

/// `first 16 bytes of SHA-256([type | sender | timestamp BE | payload])`.
pub fn packet_id(p: &Packet) -> [u8; 16] {
    let mut h = Sha256::new();
    h.update([p.ptype.as_u8()]);
    h.update(p.sender.0);
    h.update(p.timestamp.to_be_bytes());
    h.update(&p.payload);
    let d = h.finalize();
    let mut k = [0u8; 16];
    k.copy_from_slice(&d[..16]);
    k
}

pub fn derive_p(fpr: f64) -> u32 {
    let f = fpr.clamp(0.000001, 0.25);
    ((1.0 / f).log2().ceil() as u32).max(1)
}

pub fn estimate_max_elements(size_bytes: usize, p: u32) -> usize {
    let bits = (size_bytes * 8).max(8);
    let per = (p as usize + 2).max(3);
    (bits / per).max(1)
}

fn h64(id: &[u8; 16]) -> u64 {
    let d = Sha256::digest(id);
    u64::from_be_bytes(d[..8].try_into().unwrap()) & 0x7fff_ffff_ffff_ffff
}

fn map_hash(h: u64, modulo: u64) -> u64 {
    if modulo <= 1 {
        return 0;
    }
    match h % modulo {
        0 => 1,
        v => v,
    }
}

/// The value `id` occupies in a filter with modulus `m`.
pub fn bucket(id: &[u8; 16], m: u32) -> u64 {
    map_hash(h64(id), m.max(1) as u64)
}

pub struct Built {
    pub p: u32,
    pub m: u32,
    pub data: Vec<u8>,
    /// How many of the input ids (in input order) the filter encodes.
    pub included: usize,
}

fn hash_range(count: usize, p: u32) -> u32 {
    if count == 0 {
        return 1;
    }
    let v = (count as u64).saturating_mul(1u64 << p.min(40));
    v.clamp(1, u32::MAX as u64) as u32
}

fn encode(sorted: &[u64], p: u32) -> Vec<u8> {
    let (mut out, mut cur, mut n) = (Vec::new(), 0u8, 0u8);
    let mut bit = |b: u8, out: &mut Vec<u8>| {
        cur = (cur << 1) | (b & 1);
        n += 1;
        if n == 8 {
            out.push(cur);
            cur = 0;
            n = 0;
        }
    };
    let mut prev = 0u64;
    let mask = (1u64 << p) - 1;
    for &v in sorted {
        let x = v - prev;
        prev = v;
        let (q, r) = ((x - 1) >> p, (x - 1) & mask);
        for _ in 0..q {
            bit(1, &mut out);
        }
        bit(0, &mut out);
        for i in (0..p).rev() {
            bit(((r >> i) & 1) as u8, &mut out);
        }
    }
    if n > 0 {
        out.push(cur << (8 - n));
    }
    out
}

/// Build a filter over `ids` (newest first). Trims the oldest tail until the
/// encoding fits `max_bytes`.
pub fn build(ids: &[[u8; 16]], max_bytes: usize, fpr: f64) -> Built {
    let p = derive_p(fpr);
    if ids.is_empty() {
        return Built { p, m: 1, data: Vec::new(), included: 0 };
    }
    let cap = estimate_max_elements(max_bytes, p);
    let m = hash_range(ids.len().min(cap), p).max(1);
    let modulo = m as u64;
    let encode_first = |count: usize| -> Vec<u8> {
        let mut mapped: Vec<u64> = ids[..count].iter().map(|id| map_hash(h64(id), modulo)).collect();
        mapped.sort_unstable();
        let mut norm = Vec::with_capacity(mapped.len());
        let mut last = 0u64;
        for v in mapped {
            let v = v.min(modulo.saturating_sub(1));
            if v > last {
                norm.push(v);
                last = v;
            }
        }
        if norm.is_empty() {
            Vec::new()
        } else {
            encode(&norm, p)
        }
    };
    let mut count = ids.len().min(cap);
    let mut data = encode_first(count);
    while data.len() > max_bytes && count > 1 {
        count = (count * 9 / 10).max(1);
        data = encode_first(count);
    }
    if data.len() > max_bytes {
        return Built { p, m, data: Vec::new(), included: 0 };
    }
    let included = if data.is_empty() { 0 } else { count };
    Built { p, m, data, included }
}

/// Sorted bucket values of a received filter. Out-of-range parameters decode
/// to "has nothing" (the caller then sends everything).
pub fn decode_sorted(p: u32, m: u32, data: &[u8]) -> Vec<u64> {
    if !(1..=MAX_P).contains(&p) || m <= 1 {
        return Vec::new();
    }
    let total = data.len() * 8;
    let mut pos = 0usize;
    let bit = |pos: &mut usize| -> Option<u8> {
        if *pos >= total {
            return None;
        }
        let b = (data[*pos / 8] >> (7 - *pos % 8)) & 1;
        *pos += 1;
        Some(b)
    };
    let mut vals = Vec::new();
    let mut acc = 0u64;
    loop {
        let mut q = 0u64;
        loop {
            match bit(&mut pos) {
                Some(1) => q += 1,
                Some(_) => break,
                None => return vals,
            }
        }
        let mut r = 0u64;
        for _ in 0..p {
            match bit(&mut pos) {
                Some(b) => r = (r << 1) | b as u64,
                None => return vals,
            }
        }
        acc = acc.wrapping_add((q << p) + r + 1);
        if acc >= m as u64 {
            return vals;
        }
        vals.push(acc);
    }
}

pub fn contains(sorted: &[u64], v: u64) -> bool {
    sorted.binary_search(&v).is_ok()
}

/// REQUEST_SYNC payload TLVs (type u8, length u16 BE, value).
pub struct Request {
    pub p: u32,
    pub m: u32,
    pub data: Vec<u8>,
    /// Sync-type bitfield; `None` = default (announce + message).
    pub types: Option<u64>,
    pub since: Option<u64>,
}

impl Request {
    pub fn encode(&self) -> Vec<u8> {
        fn tlv(out: &mut Vec<u8>, t: u8, v: &[u8]) {
            out.push(t);
            out.extend_from_slice(&(v.len() as u16).to_be_bytes());
            out.extend_from_slice(v);
        }
        let mut out = Vec::new();
        tlv(&mut out, 0x01, &[self.p as u8]);
        tlv(&mut out, 0x02, &self.m.to_be_bytes());
        tlv(&mut out, 0x03, &self.data);
        if let Some(t) = self.types.filter(|t| *t != 0) {
            // Little-endian, trailing zero bytes trimmed.
            let mut b = t.to_le_bytes().to_vec();
            while b.len() > 1 && b.last() == Some(&0) {
                b.pop();
            }
            tlv(&mut out, 0x04, &b);
        }
        if let Some(s) = self.since {
            tlv(&mut out, 0x05, &s.to_be_bytes());
        }
        out
    }

    pub fn decode(d: &[u8]) -> Option<Request> {
        let (mut p, mut m, mut data, mut types, mut since) = (None, None, None, None, None);
        let mut off = 0usize;
        while off + 3 <= d.len() {
            let t = d[off];
            let n = u16::from_be_bytes([d[off + 1], d[off + 2]]) as usize;
            off += 3;
            if off + n > d.len() {
                return None;
            }
            let v = &d[off..off + n];
            off += n;
            match t {
                0x01 if v.len() == 1 => p = Some(v[0] as u32),
                0x02 if v.len() == 4 => m = Some(u32::from_be_bytes(v.try_into().ok()?)),
                0x03 => {
                    if v.len() > 1024 {
                        return None;
                    }
                    data = Some(v.to_vec());
                }
                0x04 if (1..=8).contains(&v.len()) => {
                    let mut b = [0u8; 8];
                    b[..v.len()].copy_from_slice(v);
                    types = Some(u64::from_le_bytes(b));
                }
                0x05 if v.len() == 8 => since = Some(u64::from_be_bytes(v.try_into().ok()?)),
                _ => {}
            }
        }
        let (p, m, data) = (p?, m?, data?);
        if p < 1 || p > MAX_P || m == 0 {
            return None;
        }
        Some(Request { p, m, data, types, since })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(n: usize) -> Vec<[u8; 16]> {
        (0..n)
            .map(|i| {
                let mut k = [0u8; 16];
                k[..8].copy_from_slice(&(i as u64 * 7919 + 13).to_be_bytes());
                k[8..].copy_from_slice(&(i as u64).to_le_bytes());
                k
            })
            .collect()
    }

    #[test]
    fn filter_roundtrip_has_no_false_negatives() {
        let all = ids(120);
        let b = build(&all, 400, 0.01);
        assert_eq!(b.included, 120);
        let set = decode_sorted(b.p, b.m, &b.data);
        for id in &all {
            assert!(contains(&set, bucket(id, b.m)), "member missing");
        }
        let others = ids(2000);
        let fp = others[200..].iter().filter(|id| contains(&set, bucket(id, b.m))).count();
        assert!(fp < 100, "false positives {fp}/1800");
    }

    #[test]
    fn filter_trims_to_byte_budget() {
        let all = ids(900);
        let b = build(&all, 100, 0.01);
        assert!(b.data.len() <= 100);
        assert!(b.included > 0 && b.included < 900);
    }

    #[test]
    fn request_roundtrips() {
        let r = Request { p: 7, m: 1234, data: vec![1, 2, 3], types: Some(TYPE_ANNOUNCE | TYPE_MESSAGE), since: Some(99) };
        let q = Request::decode(&r.encode()).unwrap();
        assert_eq!((q.p, q.m, q.data, q.types, q.since), (7, 1234, vec![1, 2, 3], Some(3), Some(99)));
    }
}
