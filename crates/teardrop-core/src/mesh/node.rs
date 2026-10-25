//! The mesh node: presence, controlled-flood relaying, Noise sessions and
//! private messaging — pure logic. The transport layer calls `on_frame` /
//! `on_link_*` / `tick` and executes the returned `Op`s on real links.

use super::sync;
use super::{fragment_packet, Identity, Packet, PacketType, PeerId, Reassembler, Seen, Session, PEER_DROP_MS, PEER_FRESH_MS};
use ed25519_dalek::VerifyingKey;
use rand::Rng;
use std::collections::{HashMap, HashSet, VecDeque};

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
    Public { from: PeerId, nick: String, id: String, text: String, ts: u64, mine: bool },
    /// Decrypted private message (`mine` = we sent it, `to` = the peer).
    Private { from: PeerId, to: PeerId, id: String, text: String, ts: u64, mine: bool },
    /// The peer's client acknowledged our message id.
    Delivered { peer: PeerId, id: String },
    /// The peer read our message id.
    Read { peer: PeerId, id: String },
    /// Our message id could not be delivered (handshake failed).
    Failed { peer: PeerId, id: String },
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

// ---- BitChat wire codecs (localPackages/BitFoundation/Protocols) ---------
//
// Announce = TLVs: 0x01 nickname, 0x02 noise pk (32), 0x03 sign pk (32),
// 0x04 direct neighbours (8B each, ≤10). Tolerant decode (skip unknown TLVs).
//
// Public message payload = BitchatMessage binary: flags u8, ts u64 BE ms,
// id u8len+utf8, sender u8len+utf8, content u16len BE+utf8, optionals.
//
// Inside a Noise session, payloads are `[type u8][body]`:
//   0x01 privateMessage → PrivateMessagePacket TLV (0x00 id, 0x01 content)
//   0x02 readReceipt    → message id utf8
//   0x03 delivered      → message id utf8

fn tlv_push(out: &mut Vec<u8>, t: u8, v: &[u8]) {
    out.push(t);
    out.push(v.len() as u8);
    out.extend_from_slice(v);
}

/// Walk `[type u8][len u8][value]` TLVs; unknown types skipped.
fn tlv_each(mut d: &[u8], mut f: impl FnMut(u8, &[u8])) {
    while d.len() >= 2 {
        let (t, n) = (d[0], d[1] as usize);
        if d.len() < 2 + n {
            break;
        }
        f(t, &d[2..2 + n]);
        d = &d[2 + n..];
    }
}

fn announce_encode(nick: &str, npk: &[u8; 32], spk: &[u8; 32], nb: &[PeerId]) -> Vec<u8> {
    let mut d = Vec::with_capacity(80);
    tlv_push(&mut d, 0x01, nick.as_bytes());
    tlv_push(&mut d, 0x02, npk);
    tlv_push(&mut d, 0x03, spk);
    if !nb.is_empty() {
        let mut n = Vec::with_capacity(nb.len() * 8);
        for p in nb {
            n.extend_from_slice(&p.0);
        }
        tlv_push(&mut d, 0x04, &n);
    }
    d
}

fn announce_decode(d: &[u8]) -> Option<(String, [u8; 32], [u8; 32], Vec<PeerId>)> {
    let (mut nick, mut npk, mut spk, mut nb) = (None, None, None, Vec::new());
    tlv_each(d, |t, v| match t {
        0x01 => nick = std::str::from_utf8(v).ok().map(String::from),
        0x02 if v.len() == 32 => npk = Some(<[u8; 32]>::try_from(v).unwrap()),
        0x03 if v.len() == 32 => spk = Some(<[u8; 32]>::try_from(v).unwrap()),
        0x04 => {
            for c in v.chunks_exact(8).take(10) {
                nb.push(PeerId(c.try_into().unwrap()));
            }
        }
        _ => {}
    });
    Some((nick?, npk?, spk?, nb))
}

/// Public message payloads on the current wire are the raw UTF-8 text —
/// sender/timestamp live in the packet header, nick in the announce.
/// (The richer BitchatMessage envelope is for archives/Nostr, not BLE.)
fn public_encode(_id: &str, _nick: &str, text: &str, _ts_ms: u64) -> Vec<u8> {
    text.as_bytes().to_vec()
}

/// `(id, sender_nick, content)` — id is synthesized from nothing on the wire;
/// callers substitute the dedup key. Returns `None` for non-UTF-8 payloads.
fn public_decode(d: &[u8]) -> Option<(String, String, String)> {
    let text = std::str::from_utf8(d).ok()?;
    Some((String::new(), String::new(), text.to_string()))
}

/// Private message inside the session (PrivateMessagePacket TLV).
fn private_encode(id: &str, text: &str) -> Vec<u8> {
    let mut d = vec![0x01]; // NoisePayloadType.privateMessage
    tlv_push(&mut d, 0x00, id.as_bytes());
    tlv_push(&mut d, 0x01, text.as_bytes());
    d
}

/// delivered → 0x03, read → 0x02; body is the message id as utf8.
fn receipt_encode(delivered: bool, id: &str) -> Vec<u8> {
    let mut d = vec![if delivered { 0x03 } else { 0x02 }];
    d.extend_from_slice(id.as_bytes());
    d
}

enum Inner {
    Msg { id: String, text: String },
    Read { id: String },
    Delivered { id: String },
    Other,
}

fn inner_decode(plain: &[u8]) -> Option<Inner> {
    let (t, d) = (*plain.first()?, &plain[1..]);
    match t {
        0x01 => {
            let (mut id, mut text) = (None, None);
            tlv_each(d, |t, v| match t {
                0x00 => id = std::str::from_utf8(v).ok().map(String::from),
                0x01 => text = std::str::from_utf8(v).ok().map(String::from),
                _ => {}
            });
            Some(Inner::Msg { id: id?, text: text? })
        }
        0x02 => Some(Inner::Read { id: std::str::from_utf8(d).ok()?.to_string() }),
        0x03 => Some(Inner::Delivered { id: std::str::from_utf8(d).ok()?.to_string() }),
        _ => Some(Inner::Other),
    }
}

/// Wire id for a freshly generated message (hex of a random u64).
fn wire_id(rng: &mut impl Rng) -> String {
    format!("{:016x}", rng.gen::<u64>())
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

struct Peer {
    noise_pk: [u8; 32],
    sign_pk: [u8; 32],
    nick: String,
    last_seen: u64,
    neighbors: Vec<PeerId>,
    session: Option<PeerSession>,
    /// (wire id, text, ts) private payloads waiting on a session.
    queue: Vec<(String, String, u64)>,
}

/// §6.3 gossip sync tuning (GossipSyncManager defaults).
const SYNC_INTERVAL_MS: u64 = 15_000;
const SYNC_FIRST_MS: u64 = 5_000;
const HISTORY_CAP: usize = 1000;
const HISTORY_MAX_AGE_MS: u64 = 15 * 60_000;
const GCS_MAX_BYTES: usize = 400;
const GCS_FPR: f64 = 0.01;
/// At most this many sync responses per requester per window.
const SYNC_RESP_MAX: usize = 8;
const SYNC_RESP_WINDOW_MS: u64 = 30_000;

/// A packet kept for sync, with our arrival time (clock-skew proof ageing).
struct Stored {
    at: u64,
    pkt: Packet,
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
    /// Recent public broadcasts, oldest first.
    history: VecDeque<Stored>,
    /// Newest announce per sender.
    announces: HashMap<PeerId, Stored>,
    sync_rate: HashMap<PeerId, Vec<u64>>,
    next_sync: u64,
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
            history: VecDeque::new(),
            announces: HashMap::new(),
            sync_rate: HashMap::new(),
            next_sync: now_ms + SYNC_FIRST_MS,
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
        self.next_sync = self.next_sync.min(now_ms + SYNC_FIRST_MS);
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
            PacketType::Message => self.on_message(pkt, now_ms),
            PacketType::Leave => {
                self.peers.remove(&pkt.sender);
                (vec![], vec![Event::PeersChanged])
            }
            PacketType::NoiseHandshake => self.on_handshake(pkt, now_ms),
            PacketType::NoiseEncrypted => self.on_encrypted(pkt, now_ms),
            PacketType::RequestSync => self.on_request_sync(link, pkt, now_ms),
            PacketType::Other(_) => (vec![], vec![]),
        }
    }

    fn on_announce(&mut self, pkt: Packet, now_ms: u64) -> (Vec<Op>, Vec<Event>) {
        let Some((nick, npk, spk, nb)) = announce_decode(&pkt.payload) else {
            return (vec![], vec![]);
        };
        // Peer id binds to the announced Noise key.
        if PeerId::of_noise_pk(&npk) != pkt.sender {
            return (vec![], vec![Event::Log("mesh: announce id/key mismatch".into())]);
        }
        // Real BitChat announces are unsigned; verify only when signed.
        if pkt.signature.is_some() {
            let Ok(vk) = VerifyingKey::from_bytes(&spk) else { return (vec![], vec![]) };
            if !pkt.verify(&vk) {
                return (vec![], vec![Event::Log("mesh: announce bad signature".into())]);
            }
        }
        // Sign-key pinning: the first announce wins for this peer id.
        if self.peers.get(&pkt.sender).map(|p| p.sign_pk != spk).unwrap_or(false) {
            return (vec![], vec![Event::Log(format!("mesh: sign key flip for {}", pkt.sender))]);
        }
        self.announces.insert(pkt.sender, Stored { at: now_ms, pkt: pkt.clone() });
        let changed = match self.peers.get_mut(&pkt.sender) {
            Some(p) => {
                let changed = p.nick != nick;
                p.nick = nick;
                p.last_seen = now_ms;
                p.noise_pk = npk;
                p.sign_pk = spk;
                p.neighbors = nb;
                changed
            }
            None => {
                self.peers.insert(
                    pkt.sender,
                    Peer {
                        noise_pk: npk,
                        sign_pk: spk,
                        nick,
                        last_seen: now_ms,
                        neighbors: nb,
                        session: None,
                        queue: Vec::new(),
                    },
                );
                // A newcomer may have missed our announce (link raced the
                // subscription): answer soon so presence is symmetric.
                self.next_announce = self.next_announce.min(now_ms + 200 + self.rng.gen_range(0..300));
                true
            }
        };
        (vec![], if changed { vec![Event::PeersChanged] } else { vec![] })
    }

    fn on_message(&mut self, pkt: Packet, now_ms: u64) -> (Vec<Op>, Vec<Event>) {
        let Some((_id, nick, text)) = public_decode(&pkt.payload) else {
            let head = pkt.payload.iter().take(48).map(|b| format!("{b:02x}")).collect::<String>();
            return (vec![], vec![Event::Log(format!("mesh: public decode fail {}B {head}", pkt.payload.len()))]);
        };
        // The signature is verified against the sign key from the sender's
        // announce — until that arrives we can't prove authorship, so accept
        // but log it (BitChat queues the same way).
        match self.peers.get(&pkt.sender) {
            Some(p) if p.sign_pk != [0; 32] => {
                let Ok(vk) = VerifyingKey::from_bytes(&p.sign_pk) else { return (vec![], vec![]) };
                if !pkt.verify(&vk) {
                    return (vec![], vec![Event::Log("mesh: public message bad signature".into())]);
                }
            }
            Some(_) | None => {}
        }
        self.remember(&pkt, now_ms);
        // No wire id or nick in the payload: id = dedup key, nick = announce.
        let id = format!("{:x}", u64::from_be_bytes(pkt.dedup_key()[..8].try_into().unwrap_or_default()));
        let nick = self.nick_of(&pkt.sender).unwrap_or_else(|| nick);
        (vec![], vec![Event::Public { from: pkt.sender, nick: if nick.is_empty() { format!("anon{}", &pkt.sender.hex()[..8]) } else { nick }, id, text, ts: pkt.timestamp, mine: false }])
    }

    fn on_handshake(&mut self, pkt: Packet, now_ms: u64) -> (Vec<Op>, Vec<Event>) {
        if pkt.recipient != Some(self.peer_id()) || pkt.payload.len() < 32 {
            if pkt.recipient == Some(self.peer_id()) {
                return (vec![], vec![Event::Log(format!("mesh: handshake from {} too short ({}B)", pkt.sender, pkt.payload.len()))]);
            }
            return (vec![], vec![]);
        }
        let from = pkt.sender;
        let state = match self.peers.get(&from).and_then(|p| p.session.as_ref()) {
            None => "none",
            Some(ps) if ps.s.is_transport() => "transport",
            Some(ps) if ps.initiator => "initiator",
            Some(_) => "responder",
        };
        let mut evs0 = vec![Event::Log(format!("mesh: handshake rx {}B from {from} (ours: {state})", pkt.payload.len()))];
        // XX message 1 is exactly one ephemeral key (32 B); later flights
        // are longer. That's all we need for the tie-break/stale checks.
        let flight = if pkt.payload.len() == 32 { 1 } else { 0 };
        let mut ops = Vec::new();
        let mut evs = Vec::new();
        evs.append(&mut evs0);

        if !self.peers.contains_key(&from) {
            self.peers.insert(from, Peer::placeholder(from, now_ms));
        }
        {
            let peer = self.peers.get(&from).unwrap();
            if let Some(ps) = &peer.session {
                if ps.s.is_transport() && flight != 1 {
                    // Stale flight for a session we already finished.
                    return (vec![], vec![Event::Log("mesh: handshake ignored (stale flight)".into())]);
                }
                if !ps.s.is_transport() && ps.initiator && !ps.last_flight.is_empty() && flight == 1 && self.peer_id() < from {
                    // Simultaneous open: the lower peer id keeps its initiator
                    // role, the other side becomes responder (deterministic).
                    return (vec![], vec![Event::Log("mesh: handshake ignored (we win simultaneous open)".into())]);
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
        match ps.s.read_handshake(&pkt.payload) {
            Ok((reply, done)) => {
                evs.push(Event::Log(format!("mesh: handshake ok, reply={} done={done}", reply.as_ref().map(|r| r.len()).unwrap_or(0))));
                if let Some(flight_bytes) = reply {
                    ps.last_flight = flight_bytes.clone();
                    ops.push(self.directed_op(PacketType::NoiseHandshake, from, flight_bytes, now_ms));
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
        for (id, text, _ts) in queued {
            let inner = private_encode(&id, &text);
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
        if !self.peers.contains_key(&from) {
            self.peers.insert(from, Peer::placeholder(from, now_ms));
        }
        // BitChat handshake recovery: a ciphertext we can't decrypt means the
        // sessions have drifted (stale session on their side, restart on
        // ours) — kick a fresh XX handshake to resync rather than drop.
        // Note: decrypt mutates the transport counter, so try it exactly once.
        let no_session = self.peers.get(&from).map(|p| p.session.is_none()).unwrap_or(true);
        let plain = if no_session {
            None
        } else {
            self.peers.get_mut(&from).unwrap().session.as_mut().unwrap().s.decrypt(&pkt.payload)
        };
        let Some(plain) = plain else {
            evs.push(Event::Log(format!("mesh: undecryptable from {from} — re-handshaking")));
            self.ensure_handshake(from, now_ms, &mut ops, &mut evs);
            return (ops, evs);
        };
        let Some(inner) = inner_decode(&plain) else { return (vec![], vec![]) };
        match inner {
            Inner::Msg { id, text } => {
                evs.push(Event::Private { from, to: self.peer_id(), id: id.clone(), text, ts: pkt.timestamp, mine: false });
                // Delivery receipt rides back inside the same session.
                let ack = receipt_encode(true, &id);
                if let Some(op) = self.encrypt_to(from, &ack, now_ms) {
                    ops.push(op);
                }
            }
            Inner::Delivered { id } => evs.push(Event::Delivered { peer: from, id }),
            Inner::Read { id } => evs.push(Event::Read { peer: from, id }),
            Inner::Other => {}
        }
        (ops, evs)
    }

    // -------------------------------------------------------------- send

    /// Signed public broadcast.
    pub fn send_public(&mut self, text: &str, now_ms: u64) -> (Vec<Op>, Event) {
        let id = wire_id(&mut self.rng);
        let body = public_encode(&id, &self.nick, text, now_ms);
        let mut pkt = Packet::new(PacketType::Message, self.peer_id(), now_ms, body);
        pkt.sign(&self.id.signing_key());
        self.seen.arrive(pkt.dedup_key(), now_ms);
        self.remember(&pkt, now_ms);
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
        let id = wire_id(&mut self.rng);
        let echo = Event::Private { from: self.peer_id(), to, id: id.clone(), text: text.to_string(), ts: now_ms, mine: true };
        let inner = private_encode(&id, text);
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
    pub fn mark_read(&mut self, peer: PeerId, id: &str, now_ms: u64) -> Vec<Op> {
        let inner = receipt_encode(false, id);
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

    /// Directed packet: payload goes on the wire verbatim (Noise frames are
    /// raw handshake/transport bytes — no envelope).
    fn directed_op(&mut self, ptype: PacketType, to: PeerId, payload: Vec<u8>, now_ms: u64) -> Op {
        let mut pkt = Packet::new(ptype, self.peer_id(), now_ms, payload);
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
                evs.push(Event::Log(format!("mesh: handshake tx msg1 {}B to {to}", flight.len())));
                ps.last_flight = flight.clone();
                ps.touched_ms = now_ms;
                ops.push(self.directed_op(PacketType::NoiseHandshake, to, flight, now_ms));
            }
            Err(e) => evs.push(Event::Log(format!("mesh: cannot handshake with {to}: {e}"))),
        }
    }

    // -------------------------------------------------------------- relay

    /// §4.2 controlled flood. Returns the scheduled relay (None = absorbed).
    fn relay(&mut self, pkt: &Packet, ingress: LinkId, now_ms: u64, key: [u8; 16]) -> Option<Scheduled> {
        if pkt.ttl == 0 || matches!(pkt.ptype, PacketType::Fragment | PacketType::RequestSync) {
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
                // Broadcast messages flood to every link but the ingress,
                // matching BitChat's relay policy.
                PacketType::Message => SendTo::Except(ingress),
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

    // -------------------------------------------------------- gossip sync

    /// Keep a public broadcast for sync (bounded, ageing out).
    fn remember(&mut self, pkt: &Packet, now_ms: u64) {
        if pkt.recipient.is_some() {
            return;
        }
        let id = sync::packet_id(pkt);
        if self.history.iter().any(|s| sync::packet_id(&s.pkt) == id) {
            return;
        }
        self.history.push_back(Stored { at: now_ms, pkt: pkt.clone() });
        while self.history.len() > HISTORY_CAP {
            self.history.pop_front();
        }
    }

    /// Broadcast (TTL 0) REQUEST_SYNC advertising what we already hold.
    fn request_sync(&mut self, now_ms: u64) -> Op {
        let mut cands: Vec<(u64, [u8; 16])> = self
            .history
            .iter()
            .map(|s| &s.pkt)
            .chain(self.announces.values().map(|s| &s.pkt))
            .map(|p| (p.timestamp, sync::packet_id(p)))
            .collect();
        cands.sort_by(|a, b| b.0.cmp(&a.0)); // newest first
        let ids: Vec<[u8; 16]> = cands.iter().map(|c| c.1).collect();
        let p = sync::derive_p(GCS_FPR);
        let take = ids.len().min(sync::estimate_max_elements(GCS_MAX_BYTES, p)).min(HISTORY_CAP);
        let built = sync::build(&ids[..take], GCS_MAX_BYTES, GCS_FPR);
        // Filter trimmed the oldest tail: those are outside it, not missing.
        let since = (built.included > 0 && built.included < cands.len()).then(|| cands[built.included - 1].0);
        let req = sync::Request { p: built.p, m: built.m, data: built.data, types: Some(sync::TYPE_ANNOUNCE | sync::TYPE_MESSAGE), since };
        let mut pkt = Packet::new(PacketType::RequestSync, self.peer_id(), now_ms, req.encode());
        pkt.ttl = 0;
        pkt.sign(&self.id.signing_key());
        Op::Send { to: SendTo::All, bytes: pkt.encode() }
    }

    /// A neighbour's request: send back what its filter lacks, on that link.
    fn on_request_sync(&mut self, link: LinkId, pkt: Packet, now_ms: u64) -> (Vec<Op>, Vec<Event>) {
        if pkt.recipient.is_some() && pkt.recipient != Some(self.peer_id()) {
            return (vec![], vec![]);
        }
        let Some(req) = sync::Request::decode(&pkt.payload) else { return (vec![], vec![]) };
        // Rate limit per requester.
        let hits = self.sync_rate.entry(pkt.sender).or_default();
        hits.retain(|t| now_ms.saturating_sub(*t) < SYNC_RESP_WINDOW_MS);
        if hits.len() >= SYNC_RESP_MAX {
            return (vec![], vec![]);
        }
        hits.push(now_ms);

        let types = req.types.unwrap_or(sync::TYPE_ANNOUNCE | sync::TYPE_MESSAGE);
        let has = sync::decode_sorted(req.p, req.m, &req.data);
        let missing = |p: &Packet| !sync::contains(&has, sync::bucket(&sync::packet_id(p), req.m));
        let mut out: Vec<Packet> = Vec::new();
        if types & sync::TYPE_ANNOUNCE != 0 {
            // Latest announce per peer; the since cursor never applies.
            out.extend(self.announces.values().filter(|s| s.pkt.sender != pkt.sender && missing(&s.pkt)).map(|s| s.pkt.clone()));
        }
        if types & sync::TYPE_MESSAGE != 0 {
            let since = req.since.unwrap_or(0);
            out.extend(self.history.iter().filter(|s| s.pkt.timestamp >= since && s.pkt.sender != pkt.sender && missing(&s.pkt)).map(|s| s.pkt.clone()));
        }
        let ops = out
            .into_iter()
            .map(|mut p| {
                p.ttl = 0; // local-only: the requester already floods onward itself
                Op::Send { to: SendTo::Only(vec![link]), bytes: p.encode() }
            })
            .collect::<Vec<_>>();
        let evs = if ops.is_empty() { vec![] } else { vec![Event::Log(format!("mesh: sync reply {} pkts to {}", ops.len(), pkt.sender))] };
        (ops, evs)
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

        // Gossip sync: ask neighbours for public history we're missing.
        if now_ms >= self.next_sync {
            self.next_sync = now_ms + SYNC_INTERVAL_MS;
            if !self.links.is_empty() {
                ops.push(self.request_sync(now_ms));
            }
        }
        self.history.retain(|s| now_ms.saturating_sub(s.at) <= HISTORY_MAX_AGE_MS);
        self.announces.retain(|_, s| now_ms.saturating_sub(s.at) <= PEER_FRESH_MS);

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
        let mut drop_sessions = Vec::new();
        let mut resends: Vec<(PeerId, Vec<u8>)> = Vec::new();
        for (pid, p) in self.peers.iter_mut() {
            let Some(ps) = p.session.as_mut() else { continue };
            if ps.s.is_transport() || ps.last_flight.is_empty() {
                continue;
            }
            if now_ms.saturating_sub(ps.touched_ms) >= 4_000 {
                if ps.retries >= 5 {
                    evs.push(Event::Log(format!("mesh: handshake with {pid} timed out (no reply after 5 resends)")));
                    drop_sessions.push(*pid);
                } else {
                    ps.retries += 1;
                    ps.touched_ms = now_ms;
                    resends.push((*pid, ps.last_flight.clone()));
                }
            }
        }
        for (pid, body) in resends {
            ops.push(self.directed_op(PacketType::NoiseHandshake, pid, body, now_ms));
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
        let mut at = self.next_announce.min(self.next_sync);
        if let Some(s) = self.pending.iter().map(|s| s.at).min() {
            at = at.min(s);
        }
        at.saturating_sub(now_ms).clamp(10, 1_000)
    }

    fn announce(&mut self, now_ms: u64) -> Op {
        let nb: Vec<PeerId> = self
            .peers
            .iter()
            .filter(|(_, p)| now_ms.saturating_sub(p.last_seen) <= PEER_FRESH_MS)
            .take(10)
            .map(|(id, _)| *id)
            .collect();
        let body = announce_encode(&self.nick, &self.id.noise_pk, &self.id.sign_pk, &nb);
        let mut pkt = Packet::new(PacketType::Announce, self.peer_id(), now_ms, body);
        pkt.sign(&self.id.signing_key());
        self.seen.arrive(pkt.dedup_key(), now_ms);
        self.announces.insert(self.peer_id(), Stored { at: now_ms, pkt: pkt.clone() });
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
    fn gossip_sync_backfills_a_missed_message() {
        let mut a = Node::new(Identity::generate(), "alice".into(), 0);
        let mut b = Node::new(Identity::generate(), "bob".into(), 0);
        wired(&mut a, &mut b, 0);
        // Alice speaks while Bob's radio hears nothing.
        let _lost = a.send_public("you missed this", 20_000);
        let (mut aev, mut bev) = (Vec::new(), Vec::new());
        settle(&mut a, &mut b, 40_000, &mut aev, &mut bev);
        let got = bev.iter().any(|e| matches!(e, Event::Public { text, mine: false, .. } if text == "you missed this"));
        assert!(got, "bob backfilled alice's message via sync: {bev:?}");
    }

    #[test]
    fn duplicate_second_arrival_cancels_relay() {
        // Three links: packet comes in on L1; before the jitter elapses the
        // same packet arrives on L2 — the pending relay must cancel (§4.2).
        let mut a = Node::new(Identity::generate(), "alice".into(), 0);
        a.on_link_up(1, 0);
        a.on_link_up(2, 0);
        let src = Identity::generate();
        // Craft a valid public body so it isn't dropped earlier.
        let mut p = Packet::new(PacketType::Message, src.peer_id(), 500, public_encode("1", "src", "room", 500));
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
