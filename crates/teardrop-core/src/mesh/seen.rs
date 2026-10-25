//! §4.2 deduplication: an LRU seen-set keyed by `Packet::dedup_key`.
//! 1000 entries, 5-minute expiry. The arrival count lets a scheduled relay
//! cancel itself when the same packet shows up from another link first.

use std::collections::{HashMap, VecDeque};

const CAP: usize = 1000;
const TTL_MS: u64 = 5 * 60 * 1000;

#[derive(Default)]
pub struct Seen {
    map: HashMap<[u8; 16], Entry>,
    /// FIFO eviction order (re-added on each sighting; stale slots dropped lazily).
    order: VecDeque<[u8; 16]>,
}

struct Entry {
    /// Distinct arrivals (a scheduled relay cancels when this climbs past 1).
    count: u32,
    last_ms: u64,
}

impl Seen {
    /// Record an arrival. Returns true when this packet is new to us.
    pub fn arrive(&mut self, key: [u8; 16], now_ms: u64) -> bool {
        self.evict(now_ms);
        match self.map.get_mut(&key) {
            Some(e) => {
                e.count += 1;
                e.last_ms = now_ms;
                false
            }
            None => {
                if self.map.len() >= CAP {
                    if let Some(old) = self.order.pop_front() {
                        self.map.remove(&old);
                    }
                }
                self.map.insert(key, Entry { count: 1, last_ms: now_ms });
                self.order.push_back(key);
                true
            }
        }
    }

    /// How many times this packet has arrived (>=2 → someone else relayed it).
    pub fn arrivals(&self, key: &[u8; 16]) -> u32 {
        self.map.get(key).map(|e| e.count).unwrap_or(0)
    }

    fn evict(&mut self, now_ms: u64) {
        while let Some(&k) = self.order.front() {
            let stale = self.map.get(&k).map(|e| now_ms.saturating_sub(e.last_ms) > TTL_MS).unwrap_or(true);
            if !stale {
                break;
            }
            self.order.pop_front();
            self.map.remove(&k);
        }
    }

    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.map.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_arrival_is_new_rest_are_dupes() {
        let mut s = Seen::default();
        let k = [7u8; 16];
        assert!(s.arrive(k, 0));
        assert!(!s.arrive(k, 1));
        assert!(!s.arrive(k, 2));
        assert_eq!(s.arrivals(&k), 3);
    }

    #[test]
    fn expiry_makes_old_keys_new_again() {
        let mut s = Seen::default();
        let k = [1u8; 16];
        assert!(s.arrive(k, 0));
        assert!(s.arrive(k, TTL_MS + 1), "expired entries are forgotten");
    }

    #[test]
    fn lru_evicts_at_capacity() {
        let mut s = Seen::default();
        let mut first = [0u8; 16];
        first[0] = 0xFF;
        s.arrive(first, 0);
        for i in 0..CAP {
            let mut k = [0u8; 16];
            k[..8].copy_from_slice(&(i as u64).to_be_bytes());
            s.arrive(k, i as u64 + 1);
        }
        assert_eq!(s.len(), CAP);
        // The oldest entry was pushed out.
        assert_eq!(s.arrivals(&first), 0);
    }
}
