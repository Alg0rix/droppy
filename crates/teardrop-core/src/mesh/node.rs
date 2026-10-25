//! The mesh node: presence, controlled-flood relaying, Noise sessions and
//! private messaging — pure logic. The transport layer calls `on_frame` /
//! `on_link_*` / `tick` and executes the returned `Op`s on real links.

use super::{fragment_packet, Identity, Packet, PacketType, PeerId, Reassembler, Seen, Session, PEER_DROP_MS, PEER_FRESH_MS};
use ed25519_dalek::VerifyingKey;
use rand::Rng;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

/// A transport-level link id (one BLE peer connection, either direction).
pub type LinkId = u64;

#[derive(Debug, Clone)]
pub enum SendTo {
    /// Every link.
    All,
    /// Every link except this one (split horizon).
    Except(LinkId),
    /// A deterministic subset.
    Only(Vec<LinkId>),
}

/// Something for the transport to put on the wire.
#[derive(Debug, Clone)]
pub enum Op {
    Send { to: SendTo, bytes: Vec<u8> },
}

/// Something for the UI / service layer.
#[derive(Debug, Clone)]
pub enum Event {
    /// Peer set or a peer's details changed.
    PeersChanged,
    /// Our link count changed (degree drives flood tuning).
    Links(usize),
    /// Signed public broadcast (`mine` = we sent it).
    Public { from: PeerId, nick: String, id: u64, text: String, ts: u64, mine: bool },
    /// Decrypted private message (`mine` = we sent it, `to` = the peer).
    Private { from: PeerId, to: PeerId, id: u64, text: String, ts: u64, mine: bool },
    /// The peer's client acknowledged our message id.
    Delivered { peer: PeerId, id: u64 },
    /// The peer read our message id.
    Read { peer: PeerId, id: u64 },
    /// Our message id could not be delivered (handshake failed).
    Failed { peer: PeerId, id: u64 },
    /// A Noise session with this peer came up.
    Secure(PeerId),
    /// Free-form diagnostic.
    Log(String),
}

#[derive(Debug, Clone)]
pub struct PeerInfo {
    pub id: PeerId,
    pub nick: String,
    /// Announced within the last 60 s.
    pub fresh: bool,
    /// Noise session established.
    pub secure: bool,
}

// ---- payload bodies (JSON; announcements stay inspectable by design, §3) --

#[derive(Serialize, Deserialize)]
struct AnnounceBody {
    n: String,
    npk: String,
    spk: String,
    #[serde(default)]
    nb: Vec<String>,
}

#[derive(Serialize, Deserialize)]
struct PublicBody {
    id: String,
    t: String,
    n: String,
    npk: String,
    spk: String,
}

#[derive(Serialize)]
struct InnerOut<'a> {
    k: &'a str,
    id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    t: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    ts: Option<u64>,
}

#[derive(Deserialize)]
struct InnerIn {
    k: String,
    id: String,
    #[serde(default)]
    t: Option<String>,
    #[serde(default)]
    ts: Option<u64>,
}

// ----

struct PeerSession {
    s: Session,
    /// We sent the first flight (initiator); false = responder.
    initiator: bool,
    /// Last flight we emitted — resent while a handshake stalls.
    last_flight: Vec<u8>,
    retries: u8,
    touched_ms: u64,
}

impl PeerSession {
    fn flight_tag(&self) -> u8 {
        if self.initiator {
            1
        } else {
            2
        }
    }
}

struct Peer {
    noise_pk: [u8; 32],
    sign_pk: [u8; 32],
    nick: String,
    last_seen: u64,
    neighbors: Vec<PeerId>,
    session: Option<PeerSession>,
    /// (id, text, ts) private payloads waiting on a session.
    queue: Vec<(u64, String, u64)>,
}

struct Scheduled {
    at: u64,
    key: [u8; 16],
    to: SendTo,
    bytes: Vec<u8>,
}

pub struct Node {
    id: Identity,
    nick: String,
    links: HashSet<LinkId>,
    peers: HashMap<PeerId, Peer>,
    seen: Seen,
    reasm: Reassembler,
    pending: Vec<Scheduled>,
    next_announce: u64,
    rng: rand::rngs::OsRng,
}

impl Node {
    pub fn new(id: Identity, nick: String, now_ms: u64) -> Self {
        Self {
            id,
            nick,
            links: HashSet::new(),
            peers: HashMap::new(),
            seen: Seen::default(),
            reasm: Reassembler::default(),
            pending: Vec::new(),
            next_announce: now_ms + 400,
            rng: rand::rngs::OsRng,
        }
    }

    pub fn peer_id(&self) -> PeerId {
        self.id.peer_id()
    }

    pub fn link_count(&self) -> usize {
        self.links.len()
    }

    pub fn peers(&self, now_ms: u64) -> Vec<PeerInfo> {
        let mut v: Vec<PeerInfo> = self
            .peers
            .iter()
            .map(|(pid, p)| PeerInfo {
                id: *pid,
                nick: p.nick.clone(),
                fresh: now_ms.saturating_sub(p.last_seen) <= PEER_FRESH_MS,
                secure: p.session.as_ref().map(|s| s.s.is_transport()).unwrap_or(false),
            })
            .collect();
        v.sort_by_key(|p| p.id);
        v
    }

    pub fn nick_of(&self, peer: &PeerId) -> Option<String> {
        self.peers.get(peer).map(|p| p.nick.clone())
    }

    pub fn set_nick(&mut self, nick: String, now_ms: u64) {
        self.nick = nick;
        self.next_announce = self.next_announce.min(now_ms + 300);
    }

    // ---------------------------------------------------------------- links

    pub fn on_link_up(&mut self, link: LinkId, now_ms: u64) -> Vec<Event> {
        self.links.insert(link);
        // Say hello quickly so the new neighbour learns us.
        self.next_announce = self.next_announce.min(now_ms + 300 + self.rng.gen_range(0..400));
        vec![Event::Links(self.links.len())]
    }

    pub fn on_link_down(&mut self, link: LinkId) -> Vec<Event> {
        if self.links.remove(&link) {
            vec![Event::Links(self.links.len())]
        } else {
            vec![]
        }
    }

    // ------------------------------------------------------------ receive

    pub fn on_frame(&mut self, link: LinkId, bytes: &[u8], now_ms: u64) -> (Vec<Op>, Vec<Event>) {
        let Some(pkt) = Packet::decode(bytes) else {
            return (vec![], vec![Event::Log(format!("mesh: dropped undecodable {}B frame", bytes.len()))]);
        };
        let key = pkt.dedup_key();
        if !self.seen.arrive(key, now_ms) {
            return (vec![], vec![]); // duplicate; cancels any pending relay of it
        }
        let (ops, evs) = self.process(link, pkt.clone(), now_ms);
        if let Some(sched) = self.relay(&pkt, link, now_ms, key) {
            self.pending.push(sched);
        }
        (ops, evs)
    }

    fn process(&mut self, link: LinkId, pkt: Packet, now_ms: u64) -> (Vec<Op>, Vec<Event>) {
        match pkt.ptype {
            PacketType::Fragment => {
                match self.reasm.feed(pkt.sender, &pkt.payload, now_ms) {
                    Some(frame) => self.on_frame(link, &frame, now_ms),
                    None => (vec![], vec![]),
                }
            }
            PacketType::Announce => self.on_announce(pkt, now_ms),
            PacketType::Message => self.on_message(pkt),
            PacketType::NoiseHandshake => self.on_handshake(pkt, now_ms),
            PacketType::NoiseEncrypted => self.on_encrypted(pkt, now_ms),
        }
    }

    fn on_announce(&mut self, pkt: Packet, now_ms: u64) -> (Vec<Op>, Vec<Event>) {
        let Some(body) = serde_json::from_slice::<AnnounceBody>(&pkt.payload).ok() else {
            return (vec![], vec![]);
        };
        let (Some(npk), Some(spk)) = (parse_32(&body.npk), parse_32(&body.spk)) else {
            return (vec![], vec![]);
        };
        // Peer id binds to the announced Noise key.
        if PeerId::of_noise_pk(&npk) != pkt.sender {
            return (vec![], vec![Event::Log("mesh: announce id/key mismatch".into())]);
        }
        let Ok(vk) = VerifyingKey::from_bytes(&spk) else { return (vec![], vec![]) };
        if !pkt.verify(&vk) {
            return (vec![], vec![Event::Log("mesh: announce bad signature".into())]);
        }
        // Sign-key pinning: the first announce wins for this peer id.
        if self.peers.get(&pkt.sender).map(|p| p.sign_pk != spk).unwrap_or(false) {
            return (vec![], vec![Event::Log(format!("mesh: sign key flip for {}", pkt.sender))]);
        }
        let changed = match self.peers.get_mut(&pkt.sender) {
            Some(p) => {
                let changed = p.nick != body.n;
                p.nick = body.n;
                p.last_seen = now_ms;
                p.neighbors = body.nb.iter().filter_map(|s| PeerId::from_hex(s)).take(10).collect();
                changed
            }
            None => {
                self.peers.insert(
                    pkt.sender,
                    Peer {
                        noise_pk: npk,
                        sign_pk: spk,
                        nick: body.n,
                        last_seen: now_ms,
                        neighbors: body.nb.iter().filter_map(|s| PeerId::from_hex(s)).take(10).collect(),
                        session: None,
                        queue: Vec::new(),
                    },
                );
                true
            }
        };
        (vec![], if changed { vec![Event::PeersChanged] } else { vec![] })
    }

    fn on_message(&mut self, pkt: Packet) -> (Vec<Op>, Vec<Event>) {
        let Some(body) = serde_json::from_slice::<PublicBody>(&pkt.payload).ok() else {
            return (vec![], vec![]);
        };
        let (Some(npk), Some(spk)) = (parse_32(&body.npk), parse_32(&body.spk)) else {
            return (vec![], vec![]);
        };
        if PeerId::of_noise_pk(&npk) != pkt.sender {
            return (vec![], vec![]);
        }
        let Ok(vk) = VerifyingKey::from_bytes(&spk) else { return (vec![], vec![]) };
        if !pkt.verify(&vk) {
            return (vec![], vec![]);
        }
        if let Some(p) = self.peers.get(&pkt.sender) {
            if p.sign_pk != spk {
                return (vec![], vec![]);
            }
        }
        let Ok(id) = u64::from_str_radix(&body.id, 16) else { return (vec![], vec![]) };
        (vec![], vec![Event::Public { from: pkt.sender, nick: body.n, id, text: body.t, ts: pkt.timestamp, mine: false }])
    }

    fn on_handshake(&mut self, pkt: Packet, now_ms: u64) -> (Vec<Op>, Vec<Event>) {
        if pkt.recipient != Some(self.peer_id()) || pkt.payload.len() < 2 {
            return (vec![], vec![]);
        }
        let from = pkt.sender;
        let flight = pkt.payload[0];
        let mut ops = Vec::new();
        let mut evs = Vec::new();

        if !self.peers.contains_key(&from) {
            self.peers.insert(from, Peer::placeholder(from, now_ms));
        }
        {
            let peer = self.peers.get(&from).unwrap();
            if let Some(ps) = &peer.session {
                if ps.s.is_transport() && flight != 1 {
                    // Stale flight for a session we already finished.
                    return (vec![], vec![]);
                }
                if !ps.s.is_transport() && ps.initiator && !ps.last_flight.is_empty() && flight == 1 && self.peer_id() < from {
                    // Simultaneous open: the lower peer id keeps its initiator
                    // role, the other side becomes responder (deterministic).
                    return (vec![], vec![]);
                }
            }
        }

        let peer = self.peers.get_mut(&from).unwrap();
        // Start a responder session when there's none, when a fresh msg1
        // arrives over a finished session (re-handshake), or when we lose
        // the simultaneous-open tie-break (the earlier check already
        // returned for the case we win).
        let need_responder = match &peer.session {
            None => true,
            Some(ps) => ps.s.is_transport() || (ps.initiator && !ps.last_flight.is_empty() && flight == 1),
        };
        if need_responder {
            match Session::responder(&self.id.noise_sk) {
                Ok(s) => {
                    peer.session = Some(PeerSession { s, initiator: false, last_flight: Vec::new(), retries: 0, touched_ms: now_ms });
                }
                Err(_) => return (vec![], vec![]),
            }
        }
        let ps = peer.session.as_mut().unwrap();
        ps.touched_ms = now_ms;
        match ps.s.read_handshake(&pkt.payload[1..]) {
            Ok((reply, done)) => {
                if let Some(flight_bytes) = reply {
                    ps.last_flight = flight_bytes.clone();
                    ops.push(self.directed_op(PacketType::NoiseHandshake, from, flight_bytes, 2, now_ms));
                }
                if done {
                    evs.extend(self.session_up(from, now_ms, &mut ops));
                }
            }
            Err(e) => {
                evs.push(Event::Log(format!("mesh: handshake with {from} failed: {e}")));
                peer.session = None;
            }
        }
        (ops, evs)
    }

    /// A session just finished its handshake: bind the remote static key to
    /// the claimed peer id, then flush the queued private payloads.
    fn session_up(&mut self, from: PeerId, now_ms: u64, ops: &mut Vec<Op>) -> Vec<Event> {
        let peer = self.peers.get_mut(&from).unwrap();
        let remote = peer.session.as_mut().and_then(|ps| ps.s.remote_static());
        match remote {
            Some(pk) if PeerId::of_noise_pk(&pk) == from => {
                peer.noise_pk = pk;
            }
            _ => {
                peer.session = None;
                peer.queue.clear();
                return vec![Event::Log(format!("mesh: {from} noise key does not match peer id — dropped"))];
            }
        }
        let mut evs = vec![Event::Secure(from), Event::PeersChanged];
        let queued = std::mem::take(&mut peer.queue);
        for (id, text, ts) in queued {
            let inner = serde_json::to_vec(&InnerOut { k: "m", id: format!("{id:x}"), t: Some(&text), ts: Some(ts) }).unwrap();
            if let Some(op) = self.encrypt_to(from, &inner, now_ms) {
                ops.push(op);
            } else {
                evs.push(Event::Failed { peer: from, id });
            }
        }
        evs
    }

    fn on_encrypted(&mut self, pkt: Packet, now_ms: u64) -> (Vec<Op>, Vec<Event>) {
        if pkt.recipient != Some(self.peer_id()) {
            return (vec![], vec![]);
        }
        let from = pkt.sender;
        let mut ops = Vec::new();
        let mut evs = Vec::new();
        let Some(peer) = self.peers.get_mut(&from) else { return (vec![], vec![]) };
        let Some(ps) = peer.session.as_mut() else { return (vec![], vec![]) };
        let Some(plain) = ps.s.decrypt(&pkt.payload) else {
            return (vec![], vec![Event::Log(format!("mesh: undecryptable frame from {from}"))]);
        };
        let Ok(inner) = serde_json::from_slice::<InnerIn>(&plain) else { return (vec![], vec![]) };
        let Ok(id) = u64::from_str_radix(&inner.id, 16) else { return (vec![], vec![]) };
        match inner.k.as_str() {
            "m" => {
                let text = inner.t.clone().unwrap_or_default();
                let ts = inner.ts.unwrap_or(pkt.timestamp);
                evs.push(Event::Private { from, to: self.peer_id(), id, text, ts, mine: false });
                // Delivery receipt rides back inside the same session.
                let ack = serde_json::to_vec(&InnerOut { k: "a", id: inner.id, t: None, ts: None }).unwrap();
                if let Some(op) = self.encrypt_to(from, &ack, now_ms) {
                    ops.push(op);
                }
            }
            "a" => evs.push(Event::Delivered { peer: from, id }),
            "r" => evs.push(Event::Read { peer: from, id }),
            _ => {}
        }
        (ops, evs)
    }

    // -------------------------------------------------------------- send

    /// Signed public broadcast.
    pub fn send_public(&mut self, text: &str, now_ms: u64) -> (Vec<Op>, Event) {
        let id: u64 = self.rng.gen();
        let body = serde_json::to_vec(&PublicBody {
            id: format!("{id:x}"),
            t: text.to_string(),
            n: self.nick.clone(),
            npk: super::hex(&self.id.noise_pk),
            spk: super::hex(&self.id.sign_pk),
        })
        .unwrap();
        let mut pkt = Packet::new(PacketType::Message, self.peer_id(), now_ms, body);
        pkt.sign(&self.id.signing_key());
        self.seen.arrive(pkt.dedup_key(), now_ms);
        let wire = pkt.encode();
        (
            vec![Op::Send { to: SendTo::All, bytes: wire }],
            Event::Public { from: self.peer_id(), nick: self.nick.clone(), id, text: text.to_string(), ts: now_ms, mine: true },
        )
    }

    /// Private message; starts a Noise XX session when none exists and
    /// queues the payload under it until the transport is up.
    pub fn send_private(&mut self, to: PeerId, text: &str, now_ms: u64) -> (Vec<Op>, Vec<Event>) {
        if to == self.peer_id() {
            return (vec![], vec![]);
        }
        if !self.peers.contains_key(&to) {
            self.peers.insert(to, Peer::placeholder(to, now_ms));
        }
        let id: u64 = self.rng.gen();
        let echo = Event::Private { from: self.peer_id(), to, id, text: text.to_string(), ts: now_ms, mine: true };
        let inner = serde_json::to_vec(&InnerOut { k: "m", id: format!("{id:x}"), t: Some(text), ts: Some(now_ms) }).unwrap();
        let mut ops = Vec::new();
        let mut evs = vec![echo];

        let secure = self.peers.get(&to).and_then(|p| p.session.as_ref()).map(|s| s.s.is_transport()).unwrap_or(false);
        if secure {
            if let Some(op) = self.encrypt_to(to, &inner, now_ms) {
                ops.push(op);
            }
        } else {
            self.peers.get_mut(&to).unwrap().queue.push((id, text.to_string(), now_ms));
            self.ensure_handshake(to, now_ms, &mut ops, &mut evs);
        }
        (ops, evs)
    }

    /// Read receipt for the newest incoming message from `peer`.
    pub fn mark_read(&mut self, peer: PeerId, id: u64, now_ms: u64) -> Vec<Op> {
        let inner = serde_json::to_vec(&InnerOut { k: "r", id: format!("{id:x}"), t: None, ts: None }).unwrap();
        self.encrypt_to(peer, &inner, now_ms).into_iter().collect()
    }

    /// Encrypt `inner` to `peer` and wrap it in a directed packet on all links.
    fn encrypt_to(&mut self, to: PeerId, inner: &[u8], now_ms: u64) -> Option<Op> {
        let peer = self.peers.get_mut(&to)?;
        let ps = peer.session.as_mut()?;
        let ct = ps.s.encrypt(inner)?;
        let mut pkt = Packet::new(PacketType::NoiseEncrypted, self.peer_id(), now_ms, ct);
        pkt.recipient = Some(to);
        self.seen.arrive(pkt.dedup_key(), now_ms);
        Some(Op::Send { to: SendTo::All, bytes: pkt.encode() })
    }

    fn directed_op(&mut self, ptype: PacketType, to: PeerId, payload: Vec<u8>, flight: u8, now_ms: u64) -> Op {
        let mut body = Vec::with_capacity(payload.len() + 1);
        body.push(flight);
        body.extend_from_slice(&payload);
        let mut pkt = Packet::new(ptype, self.peer_id(), now_ms, body);
        pkt.recipient = Some(to);
        self.seen.arrive(pkt.dedup_key(), now_ms);
        Op::Send { to: SendTo::All, bytes: pkt.encode() }
    }

    /// Start (or nudge) an initiator handshake with `peer`.
    fn ensure_handshake(&mut self, to: PeerId, now_ms: u64, ops: &mut Vec<Op>, evs: &mut Vec<Event>) {
        let peer = self.peers.get_mut(&to).unwrap();
        if peer.session.is_none() {
            match Session::initiator(&self.id.noise_sk) {
                Ok(s) => peer.session = Some(PeerSession { s, initiator: true, last_flight: Vec::new(), retries: 0, touched_ms: now_ms }),
                Err(_) => return,
            }
        }
        let ps = peer.session.as_mut().unwrap();
        if ps.s.is_transport() || !ps.last_flight.is_empty() {
            return; // already running
        }
        match ps.s.write_handshake() {
            Ok((flight, _done)) => {
                ps.last_flight = flight.clone();
                ps.touched_ms = now_ms;
                let mut pkt = Packet::new(PacketType::NoiseHandshake, self.peer_id(), now_ms, {
                    let mut b = Vec::with_capacity(flight.len() + 1);
                    b.push(1u8);
                    b.extend_from_slice(&flight);
                    b
                });
                pkt.recipient = Some(to);
                self.seen.arrive(pkt.dedup_key(), now_ms);
                ops.push(Op::Send { to: SendTo::All, bytes: pkt.encode() });
            }
            Err(e) => evs.push(Event::Log(format!("mesh: cannot handshake with {to}: {e}"))),
        }
    }

    // -------------------------------------------------------------- relay

    /// §4.2 controlled flood. Returns the scheduled relay (None = absorbed).
    fn relay(&mut self, pkt: &Packet, ingress: LinkId, now_ms: u64, key: [u8; 16]) -> Option<Scheduled> {
        if pkt.ttl == 0 || pkt.ptype == PacketType::Fragment {
            return None; // fragments are hop-local
        }
        if pkt.recipient == Some(self.peer_id()) {
            return None; // addressed to us
        }
        let degree = self.links.len();
        if degree <= 1 {
            return None; // nowhere else to send it
        }
        let mut ttl = pkt.ttl - 1;
        if degree >= 6 {
            ttl = ttl.min(5); // dense graphs cap relay depth
        }
        let mut out = pkt.clone();
        out.ttl = ttl;
        let bytes = out.encode();
        let directed = pkt.recipient.is_some();
        let to = if directed {
            SendTo::Except(ingress) // directed traffic is never subset
        } else {
            match pkt.ptype {
                // Presence floods fully.
                PacketType::Announce => SendTo::Except(ingress),
                // Broadcast messages go to ~log2(degree) links, seeded by the
                // message id so every node picks the same subset (§4.2).
                PacketType::Message => {
                    let k = (usize::BITS - degree.leading_zeros()).max(1) as usize; // ~log2
                    let mut ids: Vec<LinkId> = self.links.iter().copied().filter(|l| *l != ingress).collect();
                    ids.sort_unstable();
                    let seed = u64::from_be_bytes(key[..8].try_into().unwrap());
                    let picked: Vec<LinkId> = (0..k.min(ids.len()))
                        .map(|i| ids[((seed as usize).wrapping_add(i.wrapping_mul(0x9E3779B9))) % ids.len()])
                        .collect::<HashSet<_>>()
                        .into_iter()
                        .collect();
                    SendTo::Only(picked)
                }
                _ => SendTo::Except(ingress),
            }
        };
        let jitter = if directed {
            self.rng.gen_range(10..60)
        } else if degree >= 6 {
            self.rng.gen_range(10..350)
        } else {
            self.rng.gen_range(10..220)
        };
        Some(Scheduled { at: now_ms + jitter, key, to, bytes })
    }

    // -------------------------------------------------------------- timer

    /// Run due work: scheduled relays, announces, retries, expiry.
    pub fn tick(&mut self, now_ms: u64) -> (Vec<Op>, Vec<Event>) {
        let mut ops = Vec::new();
        let mut evs = Vec::new();

        // Scheduled relays whose jitter elapsed; cancelled when a duplicate
        // arrived meanwhile (§4.2).
        let (due, keep): (Vec<Scheduled>, Vec<Scheduled>) = self.pending.drain(..).partition(|s| s.at <= now_ms);
        self.pending = keep;
        for s in due {
            if self.seen.arrivals(&s.key) <= 1 {
                ops.push(Op::Send { to: s.to, bytes: s.bytes });
            }
        }

        // Presence: every 4 s isolated, 15–30 s (jittered) when meshed.
        if now_ms >= self.next_announce {
            ops.push(self.announce(now_ms));
            let interval = if self.peers.values().any(|p| now_ms.saturating_sub(p.last_seen) <= PEER_FRESH_MS) {
                self.rng.gen_range(15_000..30_000)
            } else {
                4_000
            };
            self.next_announce = now_ms + interval;
        }

        // Stalled handshakes re-send their last flight (5 tries).
        let me = self.peer_id();
        let mut drop_sessions = Vec::new();
        for (pid, p) in self.peers.iter_mut() {
            let Some(ps) = p.session.as_mut() else { continue };
            if ps.s.is_transport() || ps.last_flight.is_empty() {
                continue;
            }
            if now_ms.saturating_sub(ps.touched_ms) >= 4_000 {
                if ps.retries >= 5 {
                    drop_sessions.push(*pid);
                } else {
                    ps.retries += 1;
                    ps.touched_ms = now_ms;
                    let mut pkt = Packet::new(PacketType::NoiseHandshake, me, now_ms, {
                        let mut b = Vec::with_capacity(ps.last_flight.len() + 1);
                        b.push(ps.flight_tag());
                        b.extend_from_slice(&ps.last_flight);
                        b
                    });
                    pkt.recipient = Some(*pid);
                    self.seen.arrive(pkt.dedup_key(), now_ms);
                    ops.push(Op::Send { to: SendTo::All, bytes: pkt.encode() });
                }
            }
        }
        for pid in drop_sessions {
            if let Some(p) = self.peers.get_mut(&pid) {
                p.session = None;
                for (id, _, _) in p.queue.drain(..) {
                    evs.push(Event::Failed { peer: pid, id });
                }
            }
        }

        // Peer expiry: stale after PEER_DROP_MS of silence.
        let before = self.peers.len();
        self.peers.retain(|_, p| now_ms.saturating_sub(p.last_seen) <= PEER_DROP_MS);
        if self.peers.len() != before {
            evs.push(Event::PeersChanged);
        }
        self.reasm.sweep(now_ms);

        (ops, evs)
    }

    /// ms until `tick` has work (driver's sleep bound, capped).
    pub fn next_wake(&self, now_ms: u64) -> u64 {
        let mut at = self.next_announce;
        if let Some(s) = self.pending.iter().map(|s| s.at).min() {
            at = at.min(s);
        }
        at.saturating_sub(now_ms).clamp(10, 1_000)
    }

    fn announce(&mut self, now_ms: u64) -> Op {
        let body = serde_json::to_vec(&AnnounceBody {
            n: self.nick.clone(),
            npk: super::hex(&self.id.noise_pk),
            spk: super::hex(&self.id.sign_pk),
            nb: self
                .peers
                .iter()
                .filter(|(_, p)| now_ms.saturating_sub(p.last_seen) <= PEER_FRESH_MS)
                .take(10)
                .map(|(id, _)| id.hex())
                .collect(),
        })
        .unwrap();
        let mut pkt = Packet::new(PacketType::Announce, self.peer_id(), now_ms, body);
        pkt.sign(&self.id.signing_key());
        self.seen.arrive(pkt.dedup_key(), now_ms);
        Op::Send { to: SendTo::All, bytes: pkt.encode() }
    }

    /// Split a wire frame for a link whose max write is `link_payload` bytes.
    /// Called by the transport per-link, not part of `Op` handling.
    pub fn shard(frame: &[u8], sender: PeerId, link_payload: usize, now_ms: u64) -> Vec<Vec<u8>> {
        fragment_packet(frame, sender, link_payload, now_ms)
    }
}

impl Peer {
    fn placeholder(id: PeerId, now_ms: u64) -> Peer {
        Peer {
            noise_pk: [0; 32],
            sign_pk: [0; 32],
            nick: format!("peer-{}", &id.hex()[..8]),
            last_seen: now_ms,
            neighbors: Vec::new(),
            session: None,
            queue: Vec::new(),
        }
    }
}

fn parse_32(s: &str) -> Option<[u8; 32]> {
    let b = super::unhex(s)?;
    if b.len() != 32 {
        return None;
    }
    let mut k = [0u8; 32];
    k.copy_from_slice(&b);
    Some(k)
}

#[cfg(test)]
mod tests {
    use super::*;

    const LA: LinkId = 1; // a's view of the wire
    const LB: LinkId = 2; // b's view of the wire

    /// Shuffle every pending `Send` from `src` into `dst` and back until the
    /// wire is quiet; collects events on both sides.
    fn shuttle(a: &mut Node, b: &mut Node, t: u64, seed_a: Vec<Op>, seed_b: Vec<Op>, aev: &mut Vec<Event>, bev: &mut Vec<Event>) {
        let mut qa: Vec<Vec<u8>> = seed_a
            .into_iter()
            .filter_map(|o| match o {
                Op::Send { bytes, .. } => Some(bytes),
            })
            .collect();
        let mut qb: Vec<Vec<u8>> = seed_b
            .into_iter()
            .filter_map(|o| match o {
                Op::Send { bytes, .. } => Some(bytes),
            })
            .collect();
        for _ in 0..24 {
            if qa.is_empty() && qb.is_empty() {
                return;
            }
            for bytes in qa.drain(..) {
                let (ops, e) = b.on_frame(LB, &bytes, t);
                bev.extend(e);
                qb.extend(ops.into_iter().filter_map(|o| match o {
                    Op::Send { bytes, .. } => Some(bytes),
                }).collect::<Vec<_>>());
            }
            for bytes in qb.drain(..) {
                let (ops, e) = a.on_frame(LA, &bytes, t);
                aev.extend(e);
                qa.extend(ops.into_iter().filter_map(|o| match o {
                    Op::Send { bytes, .. } => Some(bytes),
                }).collect::<Vec<_>>());
            }
        }
    }

    /// Advance both nodes to `t`, ticking + shuttling until quiet.
    fn settle(a: &mut Node, b: &mut Node, t: u64, aev: &mut Vec<Event>, bev: &mut Vec<Event>) {
        for step in 0..30 {
            let now = t + step * 250;
            let (ao, ae) = a.tick(now);
            let (bo, be) = b.tick(now);
            aev.extend(ae);
            bev.extend(be);
            if ao.is_empty() && bo.is_empty() {
                return;
            }
            shuttle(a, b, now, ao, bo, aev, bev);
        }
    }

    /// Two nodes wired directly; returns the events each side saw.
    fn wired(a: &mut Node, b: &mut Node, now: u64) -> (Vec<Event>, Vec<Event>) {
        a.on_link_up(LA, now);
        b.on_link_up(LB, now);
        let mut aev = Vec::new();
        let mut bev = Vec::new();
        settle(a, b, now + 1_000, &mut aev, &mut bev);
        (aev, bev)
    }

    #[test]
    fn announces_exchange_peers() {
        let mut a = Node::new(Identity::generate(), "alice".into(), 0);
        let mut b = Node::new(Identity::generate(), "bob".into(), 0);
        let (aev, bev) = wired(&mut a, &mut b, 0);
        let peers_a = a.peers(10_000);
        let peers_b = b.peers(10_000);
        assert_eq!(peers_a.len(), 1, "alice should see bob");
        assert_eq!(peers_b.len(), 1, "bob should see alice");
        assert_eq!(peers_a[0].nick, "bob");
        assert!(bev.iter().any(|e| matches!(e, Event::PeersChanged)));
        assert!(aev.iter().any(|e| matches!(e, Event::PeersChanged)));
    }

    #[test]
    fn public_message_delivers_and_signs() {
        let mut a = Node::new(Identity::generate(), "alice".into(), 0);
        let mut b = Node::new(Identity::generate(), "bob".into(), 0);
        wired(&mut a, &mut b, 0);
        let (ops, _echo) = a.send_public("hello mesh", 20_000);
        let mut got = None;
        for op in ops {
            {
                let Op::Send { bytes, .. } = op;
                let (_, evs) = b.on_frame(9, &bytes, 20_000);
                for e in evs {
                    if let Event::Public { text, nick, mine, .. } = e {
                        got = Some((text, nick, mine));
                    }
                }
            }
        }
        let (text, nick, mine) = got.expect("b received the public msg");
        assert_eq!(text, "hello mesh");
        assert_eq!(nick, "alice");
        assert!(!mine);
    }

    #[test]
    fn private_message_handshakes_then_delivers() {
        let mut a = Node::new(Identity::generate(), "alice".into(), 0);
        let mut b = Node::new(Identity::generate(), "bob".into(), 0);
        wired(&mut a, &mut b, 0);
        let b_id = b.peer_id();
        let (ops, evs) = a.send_private(b_id, "secret hi", 30_000);
        assert!(evs.iter().any(|e| matches!(e, Event::Private { mine: true, .. })));

        let mut aev = Vec::new();
        let mut bev = Vec::new();
        shuttle(&mut a, &mut b, 30_000, ops, vec![], &mut aev, &mut bev);

        let text = bev.iter().find_map(|e| match e {
            Event::Private { text, mine, .. } if !mine => Some(text.clone()),
            _ => None,
        });
        assert_eq!(text.as_deref(), Some("secret hi"));
        assert!(aev.iter().any(|e| matches!(e, Event::Secure(_))), "alice's session came up");
        assert!(bev.iter().any(|e| matches!(e, Event::Secure(_))), "bob's session came up");
        assert!(aev.iter().any(|e| matches!(e, Event::Delivered { .. })), "alice got the delivery ack");
    }

    #[test]
    fn duplicate_second_arrival_cancels_relay() {
        // Three links: packet comes in on L1; before the jitter elapses the
        // same packet arrives on L2 — the pending relay must cancel (§4.2).
        let mut a = Node::new(Identity::generate(), "alice".into(), 0);
        a.on_link_up(1, 0);
        a.on_link_up(2, 0);
        let src = Identity::generate();
        let mut p = Packet::new(PacketType::Message, src.peer_id(), 500, b"room".to_vec());
        p.sign(&src.signing_key());
        // Craft a valid public body so it isn't dropped earlier.
        p.payload = serde_json::to_vec(&PublicBody {
            id: "1".into(),
            t: "room".into(),
            n: "src".into(),
            npk: super::super::hex(&src.noise_pk),
            spk: super::super::hex(&src.sign_pk),
        })
        .unwrap();
        p.sign(&src.signing_key());
        let wire = p.encode();
        let _ = a.on_frame(1, &wire, 1_000);
        let _ = a.on_frame(2, &wire, 1_000); // duplicate from another relay
        let (ops, _) = a.tick(10_000);
        let relayed = ops
            .iter()
            .filter(|o| match o {
                Op::Send { bytes, .. } => Packet::decode(bytes).map(|p| p.ptype == PacketType::Message).unwrap_or(false),
            })
            .count();
        assert_eq!(relayed, 0, "relay was cancelled by the second arrival");
    }
}
