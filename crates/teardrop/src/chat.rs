//! Mesh chat service (bitchat whitepaper, BLE transport).
//!
//! Follows the `ai.rs` pattern: a worker (`mesh_ble`) owns the radio, a
//! global runtime keeps the user-visible state, `snapshot()` hands the UI a
//! cloneable `ChatView`. Identity keys persist DPAPI-sealed in
//! `%APPDATA%\Droppy\mesh-identity.json`; messages live in memory only
//! (ephemerality by default, whitepaper §1).

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Mutex;
use teardrop_core::mesh::{Event, Identity, Node, PeerId};

const ID_FILE: &str = "mesh-identity.json";
const MAX_MSGS: usize = 200;
const ROOM: &str = "@room";

#[derive(Debug, Clone, Default)]
pub struct Msg {
    pub id: u64,
    pub who: String,
    pub text: String,
    pub ts: i64,
    pub mine: bool,
    /// None = still on the wire / n.a.; Some(true) = peer's client acked.
    pub delivered: Option<bool>,
    /// The peer marked it read (DMs only).
    pub read: bool,
    /// Could not be delivered.
    pub failed: bool,
}

#[derive(Debug, Clone)]
pub struct PeerView {
    pub id: String,
    pub nick: String,
    pub fresh: bool,
    pub secure: bool,
    pub unread: u32,
}

/// What the Chat page renders (a `ViewModel` field).
#[derive(Debug, Clone, Default)]
pub struct ChatView {
    pub status: String,
    pub me: String,
    pub nick: String,
    pub peers: Vec<PeerView>,
    /// `None` = the public mesh room.
    pub sel: Option<String>,
    pub sel_nick: String,
    pub msgs: Vec<Msg>,
    pub unread_room: u32,
    pub unread_total: u32,
    /// Which field Enter commits: the message box or the nick editor.
    pub editing_nick: bool,
    pub radio_on: bool,
}

#[derive(Default)]
struct Rt {
    peers: BTreeMap<String, PeerView>,
    convs: BTreeMap<String, Vec<Msg>>,
    unread: BTreeMap<String, u32>,
    sel: Option<String>,
    nick: String,
    me: String,
    last_log: String,
    links: usize,
    editing_nick: bool,
    radio_on: bool,
}

static RT: Mutex<Option<Rt>> = Mutex::new(None);
static REVISION: AtomicU32 = AtomicU32::new(0);
static STARTED: AtomicBool = AtomicBool::new(false);

fn rt() -> std::sync::MutexGuard<'static, Option<Rt>> {
    RT.lock().unwrap()
}

fn with<R>(f: impl FnOnce(&mut Rt) -> R) -> R {
    let mut g = rt();
    f(g.get_or_insert_with(Rt::default))
}

/// Our persisted identity + nickname.
#[derive(Serialize, Deserialize, Default)]
struct StoredId {
    #[serde(default)]
    noise_sk: String,
    #[serde(default)]
    sign_sk: String,
    #[serde(default)]
    nick: String,
}

fn id_path() -> std::path::PathBuf {
    crate::paths::data_dir().join(ID_FILE)
}

fn load_or_create_identity() -> (Identity, String) {
    let p = id_path();
    if let Ok(raw) = std::fs::read_to_string(&p) {
        if let Ok(st) = serde_json::from_str::<StoredId>(&raw) {
            let sk = crate::ai::decrypt_secret(&st.noise_sk);
            let ek = crate::ai::decrypt_secret(&st.sign_sk);
            if sk.len() == 64 && ek.len() == 64 {
                if let (Some(a), Some(b)) = (hex_bytes(&sk), hex_bytes(&ek)) {
                    if a.len() == 32 && b.len() == 32 {
                        let mut noise = [0u8; 32];
                        let mut sign = [0u8; 32];
                        noise.copy_from_slice(&a);
                        sign.copy_from_slice(&b);
                        if let Some(id) = Identity::from_secrets(noise, sign) {
                            let nick = if st.nick.is_empty() { default_nick(&id) } else { st.nick };
                            return (id, nick);
                        }
                    }
                }
            }
        }
    }
    let id = Identity::generate();
    let nick = default_nick(&id);
    save_identity(&id, &nick);
    (id, nick)
}

fn save_identity(id: &Identity, nick: &str) {
    let st = StoredId {
        noise_sk: crate::ai::encrypt_secret(&teardrop_core::mesh::hex(&id.noise_sk)),
        sign_sk: crate::ai::encrypt_secret(&teardrop_core::mesh::hex(&id.sign_sk)),
        nick: nick.to_string(),
    };
    if let Ok(json) = serde_json::to_string_pretty(&st) {
        let _ = std::fs::write(id_path(), json);
    }
}

fn default_nick(id: &Identity) -> String {
    let mut nick = format!("droppy-{}", &id.peer_id().hex()[..4]);
    if let Some(name) = std::env::var_os("USERNAME").or_else(|| std::env::var_os("USER")) {
        let name = name.to_string_lossy().trim().to_string();
        if !name.is_empty() {
            nick = name.chars().take(16).collect();
        }
    }
    nick
}

fn hex_bytes(s: &str) -> Option<Vec<u8>> {
    teardrop_core::mesh::unhex(s)
}

/// Boot the mesh. Called once from window setup; safe to call again.
pub fn start(nick_hint: &str, now_ms: u64) {
    if STARTED.swap(true, Ordering::SeqCst) {
        return;
    }
    let (id, nick) = load_or_create_identity();
    let nick = if nick_hint.is_empty() { nick } else { nick_hint.to_string() };
    with(|r| {
        r.nick = nick.clone();
        r.me = id.peer_id().hex();
        r.radio_on = true;
    });
    let node = Node::new(id, nick, now_ms);
    crate::mesh_ble::start(node, now_ms);
}

/// Apply drained mesh events to the runtime. Called when the radio thread
/// wakes the UI (WM_APP_MESH) and once per second from the service tick so
/// peer freshness ages out even when the mesh is quiet.
pub fn pump() {
    let evs = crate::mesh_ble::take_events();
    if evs.is_empty() {
        return;
    }
    let mut dirty = false;
    with(|r| {
        for e in evs {
            dirty = true;
            match e {
                Event::PeersChanged => {
                    let live = crate::mesh_ble::peers();
                    r.peers.retain(|id, _| live.iter().any(|p| p.id.hex() == *id));
                    for p in live {
                        let id = p.id.hex();
                        let entry = r.peers.entry(id.clone()).or_insert(PeerView { id, nick: String::new(), fresh: false, secure: false, unread: 0 });
                        entry.nick = p.nick;
                        entry.fresh = p.fresh;
                        entry.secure = p.secure;
                    }
                }
                Event::Links(n) => r.links = n,
                Event::Public { from: _, nick, id, text, ts, mine } => {
                    push_msg(r, ROOM, Msg { id, who: if mine { "me".into() } else { nick }, text, ts: ts as i64, mine, ..Default::default() });
                    if !mine && r.sel.is_some() {
                        *r.unread.entry(ROOM.into()).or_default() += 1;
                    }
                }
                Event::Private { from, to, id, text, ts, mine } => {
                    let conv = if mine { to.hex() } else { from.hex() };
                    push_msg(r, &conv.clone(), Msg { id, who: if mine { "me".into() } else { peer_nick(r, &from) }, text, ts: ts as i64, mine, ..Default::default() });
                    if !mine {
                        if r.sel.as_deref() == Some(conv.as_str()) {
                            crate::mesh_ble::cmd(crate::mesh_ble::Cmd::Read { peer: from, id });
                        } else {
                            *r.unread.entry(conv).or_default() += 1;
                        }
                    }
                }
                Event::Delivered { peer, id } => mark(r, &peer.hex(), id, |m| m.delivered = Some(true)),
                Event::Read { peer, id } => mark(r, &peer.hex(), id, |m| {
                    m.delivered = Some(true);
                    m.read = true;
                }),
                Event::Failed { peer, id } => mark(r, &peer.hex(), id, |m| m.failed = true),
                Event::Secure(pid) => {
                    if let Some(p) = r.peers.get_mut(&pid.hex()) {
                        p.secure = true;
                    }
                }
                Event::Log(s) => r.last_log = s,
            }
        }
    });
    if dirty {
        REVISION.fetch_add(1, Ordering::SeqCst);
    }
}

fn push_msg(r: &mut Rt, conv: &str, m: Msg) {
    let v = r.convs.entry(conv.to_string()).or_default();
    if v.iter().any(|x| x.id == m.id && x.mine == m.mine) {
        return;
    }
    v.push(m);
    if v.len() > MAX_MSGS {
        v.remove(0);
    }
}

fn mark(r: &mut Rt, conv: &str, id: u64, f: impl Fn(&mut Msg)) {
    if let Some(v) = r.convs.get_mut(conv) {
        for m in v.iter_mut().rev() {
            if m.id == id {
                f(m);
            }
        }
    }
}

fn peer_nick(r: &Rt, pid: &PeerId) -> String {
    r.peers.get(&pid.hex()).map(|p| p.nick.clone()).unwrap_or_else(|| format!("peer-{}", &pid.hex()[..8]))
}

/// `revision()` bumped on every change — the UI re-snapshots when it moves.
pub fn revision() -> u32 {
    REVISION.load(Ordering::SeqCst)
}

/// Cloneable view for `ViewModel.chat`.
pub fn snapshot() -> ChatView {
    with(|r| {
        let sel = r.sel.clone();
        let msgs = r.convs.get(sel.as_deref().unwrap_or(ROOM)).cloned().unwrap_or_default();
        let unread_total = r.unread.values().sum();
        let mut peers: Vec<PeerView> = r.peers.values().cloned().collect();
        for p in &mut peers {
            p.unread = *r.unread.get(&p.id).unwrap_or(&0);
        }
        peers.sort_by(|a, b| b.fresh.cmp(&a.fresh).then(a.nick.cmp(&b.nick)));
        ChatView {
            status: {
                let s = crate::mesh_ble::status();
                if s.is_empty() {
                    if r.links == 0 { "mesh: idle".into() } else { format!("mesh: {} link{}", r.links, if r.links == 1 { "" } else { "s" }) }
                } else {
                    s
                }
            },
            me: r.me.clone(),
            nick: r.nick.clone(),
            peers,
            sel: sel.clone(),
            sel_nick: sel.as_deref().map(|id| r.peers.get(id).map(|p| p.nick.clone()).unwrap_or_else(|| id.to_string())).unwrap_or_else(|| "#room".into()),
            msgs,
            unread_room: *r.unread.get(ROOM).unwrap_or(&0),
            unread_total,
            editing_nick: r.editing_nick,
            radio_on: r.radio_on,
        }
    })
}

/// Select the room (`None`) or a peer's DM.
pub fn select(sel: Option<String>) {
    with(|r| {
        r.sel = sel.clone();
        r.editing_nick = false;
        if let Some(id) = &sel {
            r.unread.remove(id);
            // Read-receipt the newest incoming DM.
            if let Some(v) = r.convs.get(id) {
                if let Some(m) = v.iter().rev().find(|m| !m.mine) {
                    if let Some(pid) = PeerId::from_hex(id) {
                        crate::mesh_ble::cmd(crate::mesh_ble::Cmd::Read { peer: pid, id: m.id });
                    }
                }
            }
        } else {
            r.unread.remove(ROOM);
        }
    });
    REVISION.fetch_add(1, Ordering::SeqCst);
}

/// Send the input field's text to the current conversation.
pub fn send(text: String) {
    let t = text.trim();
    if t.is_empty() {
        return;
    }
    let sel = with(|r| r.sel.clone());
    match sel {
        None => crate::mesh_ble::cmd(crate::mesh_ble::Cmd::Public(t.to_string())),
        Some(id) => {
            if let Some(pid) = PeerId::from_hex(&id) {
                crate::mesh_ble::cmd(crate::mesh_ble::Cmd::Private(pid, t.to_string()));
            }
        }
    }
}

/// Set (and persist) our display name.
pub fn set_nick(nick: &str) {
    let n: String = nick.trim().chars().take(24).collect();
    if n.is_empty() {
        return;
    }
    with(|r| {
        r.nick = n.clone();
        r.editing_nick = false;
    });
    crate::mesh_ble::cmd(crate::mesh_ble::Cmd::Nick(n.clone()));
    // Nick lives next to the keys so a fresh profile still reads it.
    let p = id_path();
    if let Ok(raw) = std::fs::read_to_string(&p) {
        if let Ok(mut st) = serde_json::from_str::<StoredId>(&raw) {
            st.nick = n;
            if let Ok(json) = serde_json::to_string_pretty(&st) {
                let _ = std::fs::write(&p, json);
            }
        }
    }
    REVISION.fetch_add(1, Ordering::SeqCst);
}

/// The nick field is being edited (affects where Enter commits).
pub fn set_editing_nick(on: bool) {
    with(|r| r.editing_nick = on);
    REVISION.fetch_add(1, Ordering::SeqCst);
}
