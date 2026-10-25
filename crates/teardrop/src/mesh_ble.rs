//! BLE mesh transport (Windows): every Droppy is a GATT peripheral *and* a
//! GATT central, per the bitchat whitepaper §2/§4.
//!
//! Peripheral: a `GattServiceProvider` advertises the mesh service UUID and
//! hosts one characteristic (write + notify). Remote centrals write frames
//! to it; we push frames back as notifications to subscribed clients.
//! Central: a `BluetoothLEAdvertisementWatcher` filtered on the service UUID
//! finds nearby Droppys; we connect, subscribe to the same characteristic
//! and write frames to it.
//!
//! The whole BLE stack is driven from one worker thread so the UI never
//! blocks: commands arrive over a channel, `teardrop_core::mesh::Node`
//! decides what goes on the wire, and mesh events queue back to the UI.

use std::sync::{Mutex, OnceLock};
use teardrop_core::mesh::{Event, LinkId, Node, Op, PeerId, PeerInfo, SendTo};

/// A command the UI layer hands to the mesh worker.
pub enum Cmd {
    /// Signed public broadcast to the mesh.
    Public(String),
    /// Encrypted private message (starts a Noise session when needed).
    Private(PeerId, String),
    /// Read receipt for a delivered private message.
    Read { peer: PeerId, id: String },
    /// New display name (re-announces).
    Nick(String),
}

static CMD: Mutex<Option<std::sync::mpsc::Sender<Cmd>>> = Mutex::new(None);
static EVENTS: Mutex<Vec<Event>> = Mutex::new(Vec::new());
static STATUS: Mutex<String> = Mutex::new(String::new());
static NOTIFY: OnceLock<Box<dyn Fn() + Send + Sync>> = OnceLock::new();
static PEERS: Mutex<Vec<PeerInfo>> = Mutex::new(Vec::new());

/// Register the wake-the-UI callback (same pattern as btdev/sysmon).
pub fn set_notify(f: impl Fn() + Send + Sync + 'static) {
    let _ = NOTIFY.set(Box::new(f));
}

/// Hand a command to the mesh worker. No-op before `start` or after a fatal
/// radio error.
pub fn cmd(c: Cmd) {
    if let Some(tx) = CMD.lock().unwrap().as_ref() {
        let _ = tx.send(c);
    }
}

/// Drain mesh events since the last call.
pub fn take_events() -> Vec<Event> {
    std::mem::take(&mut *EVENTS.lock().unwrap())
}

/// The peer list the worker last computed (fresh after `PeersChanged`).
pub fn peers() -> Vec<PeerInfo> {
    PEERS.lock().unwrap().clone()
}

/// Human-readable one-liner for the Chat page header.
pub fn status() -> String {
    STATUS.lock().unwrap().clone()
}

fn set_status(s: impl Into<String>) {
    let s = s.into();
    let (_d, cache) = crate::paths::ensure_dirs();
    crate::store::log_line(&cache, &s);
    *STATUS.lock().unwrap() = s.into();
    notify();
}

fn push_events(evs: impl IntoIterator<Item = Event>) {
    let mut q = EVENTS.lock().unwrap();
    let mut any = false;
    for e in evs {
        if let Event::Log(s) = &e {
            let (_d, cache) = crate::paths::ensure_dirs();
            crate::store::log_line(&cache, s);
        }
        any = true;
        q.push(e);
    }
    drop(q);
    if any {
        notify();
    }
}

fn notify() {
    if let Some(n) = NOTIFY.get() {
        n();
    }
}

/// Start the BLE mesh on a worker thread. `node` carries our identity.
/// Idempotent: the second call is a no-op.
pub fn start(node: Node, now_ms: u64) {
    static UP: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    if UP.swap(true, std::sync::atomic::Ordering::SeqCst) {
        return;
    }
    let (tx, rx) = std::sync::mpsc::channel();
    *CMD.lock().unwrap() = Some(tx);
    std::thread::Builder::new()
        .name("droppy-mesh".into())
        .spawn(move || {
            #[cfg(windows)]
            imp::run(node, rx);
            #[cfg(not(windows))]
            {
                let _ = (node, rx, now_ms);
                set_status("BLE mesh needs Windows");
            }
        })
        .ok();
    let _ = now_ms;
}

#[cfg(windows)]
mod imp {
    use super::*;
    use std::collections::{HashMap, HashSet};
    use std::sync::mpsc::{Receiver, Sender};
    use windows::core::{GUID, HSTRING, Interface};
    use windows::Devices::Bluetooth::Advertisement::*;
    use windows::Devices::Bluetooth::GenericAttributeProfile::*;
    use windows::Devices::Bluetooth::{BluetoothAddressType, BluetoothCacheMode, BluetoothConnectionStatus, BluetoothLEDevice};
    use windows::Foundation::TypedEventHandler;
    use windows::Storage::Streams::{DataReader, DataWriter, IBuffer};
    use windows::Win32::System::WinRT::{RoInitialize, RO_INIT_MULTITHREADED};

    /// Droppy mesh GATT service + its single multiplexed characteristic.
    /// BitChat mainnet UUIDs so we interoperate with the real app
    /// (BLEService.swift upstream: testnet ends …A4B5A).
    const SERVICE: GUID = GUID::from_u128(0xf47b5e2d_4a9e_4c5a_9b3f_8e1d2c3a4b5c);
    const MESH_CHAR: GUID = GUID::from_u128(0xa1b2c3d4_e5f6_4a5b_8c9d_0e1f2a3b4c5d);
    /// Re-dial backoff: base doubles per consecutive failure up to the cap
    /// (an advertiser that never exposes our service must not be hammered).
    const RETRY_MS: u64 = 10_000;
    const RETRY_MAX_MS: u64 = 5 * 60_000;
    /// Re-sync the subscriber set this often even without a change event.
    const SYNC_MS: u64 = 5_000;
    /// ATT payload when the negotiated value is unknown.
    const DEFAULT_MTU: usize = 182;

    /// Wire-side happenings, funnelled into the single driver loop.
    enum Msg {
        /// Watcher saw the service advertised.
        Seen(u64, BluetoothAddressType),
        /// Central-side connect finished: write-capable characteristic + mtu.
        Up { addr: u64, gen: u64, ch: GattCharacteristic, dev: BluetoothLEDevice, mtu: usize },
        /// Central-side link dropped; `gen` ties it to the dial that made it
        /// so a stale handler can't kill a newer link.
        Down { addr: u64, gen: u64 },
        /// A dial attempt failed before a link existed.
        DialFailed(u64),
        /// Peripheral-side subscribed-client set changed.
        Subs,
        /// Frame arrived on link `addr`.
        Frame { addr: u64, bytes: Vec<u8> },
        /// UI command.
        Cmd(Cmd),
    }

    /// One remote link; the same remote may have both a central-side
    /// connection from us and a peripheral-side subscription to us.
    struct Link {
        /// Central side: the remote's characteristic we write to.
        ch: Option<GattCharacteristic>,
        /// Keeps the BLE connection referenced & alive.
        _dev: Option<BluetoothLEDevice>,
        gen: u64,
        /// Peripheral side: the remote's subscription we notify.
        client: Option<GattSubscribedClient>,
        mtu_ch: usize,
        mtu_client: usize,
    }

    impl Link {
        fn new() -> Self {
            Self { ch: None, _dev: None, gen: 0, client: None, mtu_ch: DEFAULT_MTU, mtu_client: DEFAULT_MTU }
        }

        fn live(&self) -> bool {
            self.ch.is_some() || self.client.is_some()
        }
    }

    fn to_buffer(bytes: &[u8]) -> windows::core::Result<IBuffer> {
        let w = DataWriter::new()?;
        w.WriteBytes(bytes)?;
        w.DetachBuffer()
    }

    fn from_buffer(buf: &IBuffer) -> Vec<u8> {
        let Ok(r) = DataReader::FromBuffer(buf) else { return Vec::new() };
        let n = r.UnconsumedBufferLength().unwrap_or(0) as usize;
        let mut v = vec![0u8; n];
        let _ = r.ReadBytes(&mut v);
        v
    }

    /// `BluetoothLE#BluetoothLE<adapter>-<remote mac>` → remote mac as u64.
    fn addr_of_device_id(id: &HSTRING) -> Option<u64> {
        let s = id.to_string();
        let tail = s.rsplit('-').next()?;
        let hex: String = tail.chars().filter(|c| c.is_ascii_hexdigit()).collect();
        if hex.is_empty() || hex.len() > 16 {
            return None;
        }
        u64::from_str_radix(&hex, 16).ok()
    }

    pub fn run(mut node: Node, cmd_rx: Receiver<Cmd>) {
        unsafe {
            let _ = RoInitialize(RO_INIT_MULTITHREADED);
        }
        let (tx, rx) = std::sync::mpsc::channel::<Msg>();
        // Bridge UI commands into the same queue.
        {
            let tx = tx.clone();
            std::thread::Builder::new()
                .name("droppy-mesh-cmd".into())
                .spawn(move || {
                    while let Ok(c) = cmd_rx.recv() {
                        if tx.send(Msg::Cmd(c)).is_err() {
                            return;
                        }
                    }
                })
                .ok();
        }

        // These must stay alive for the whole run: drop them and the radio
        // stops serving.
        let mut keep: Vec<windows::core::IInspectable> = Vec::new();
        match peripheral(&tx) {
            Ok((provider, ch)) => {
                keep.push(provider.cast::<windows::core::IInspectable>().unwrap());
                keep.push(ch.cast::<windows::core::IInspectable>().unwrap());
                set_status("mesh: advertising + scanning");
            }
            Err(e) => set_status(format!("mesh: GATT server failed ({e})")),
        }
        match watcher(&tx) {
            Ok(w) => {
                let _ = w.Start();
                keep.push(w.cast::<windows::core::IInspectable>().unwrap());
            }
            Err(e) => set_status(format!("mesh: BLE scan failed ({e})")),
        }

        let mut links: HashMap<u64, Link> = HashMap::new();
        let mut dial: HashMap<u64, (u64, u32)> = HashMap::new(); // addr -> (last attempt ms, consecutive fails)
        let mut next_gen: u64 = 0;
        let mut next_sync: u64 = 0;

        // Packet timestamps are wall-clock epoch ms (peers compare them
        // against their own dedup windows).
        let epoch = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        let start = std::time::Instant::now();
        let now = move || epoch + start.elapsed().as_millis() as u64;
        let _ = keep;

        loop {
            let sleep = std::time::Duration::from_millis(node.next_wake(now()));
            let msg = match rx.recv_timeout(sleep) {
                Ok(m) => Some(m),
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => None,
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => return,
            };
            let mut evs = Vec::new();
            let mut ops = Vec::new();
            match msg {
                Some(Msg::Seen(addr, atype)) => {
                    if links.get(&addr).map(|l| l.ch.is_some()).unwrap_or(false) {
                        continue; // already dialed
                    }
                    let (last, fails) = dial.get(&addr).copied().unwrap_or((0, 0));
                    let wait = (RETRY_MS << fails.min(5)).min(RETRY_MAX_MS);
                    if last != 0 && now().saturating_sub(last) < wait {
                        continue;
                    }
                    next_gen += 1;
                    let gen = next_gen;
                    dial.insert(addr, (now(), fails));
                    let tx = tx.clone();
                    std::thread::spawn(move || {
                        if let Err(e) = dial_out(addr, gen, atype, tx.clone()) {
                            let _ = tx.send(Msg::DialFailed(addr));
                            push_events([Event::Log(format!("mesh: connect {addr:012x} failed ({e})"))]);
                        }
                    });
                    continue;
                }
                Some(Msg::DialFailed(addr)) => {
                    let e = dial.entry(addr).or_insert((now(), 0));
                    e.1 = e.1.saturating_add(1);
                    continue;
                }
                Some(Msg::Up { addr, gen, ch, dev, mtu }) => {
                    dial.insert(addr, (now(), 0));
                    let l = links.entry(addr).or_insert_with(Link::new);
                    let was_live = l.live();
                    l.ch = Some(ch);
                    l._dev = Some(dev);
                    l.gen = gen;
                    l.mtu_ch = mtu.clamp(23, 512);
                    if !was_live {
                        evs.extend(node.on_link_up(addr as LinkId, now()));
                    }
                    push_events([Event::Log(format!("mesh: link up {addr:012x} (mtu {mtu})"))]);
                }
                Some(Msg::Down { addr, gen }) => {
                    if let Some(l) = links.get_mut(&addr) {
                        if l.gen != gen || l.ch.is_none() {
                            continue; // stale handler from an older dial
                        }
                        l.ch = None;
                        l._dev = None;
                        if !l.live() {
                            links.remove(&addr);
                            evs.extend(node.on_link_down(addr as LinkId));
                        }
                        push_events([Event::Log(format!("mesh: link down {addr:012x}"))]);
                    }
                }
                Some(Msg::Subs) => sync_subs(&mut links, &mut node, &mut evs, now()),
                Some(Msg::Frame { addr, bytes }) => {
                    if !links.contains_key(&addr) {
                        // A write can beat the subscription event; learn the
                        // subscriber now so replies have a route.
                        sync_subs(&mut links, &mut node, &mut evs, now());
                    }
                    let (o, e) = node.on_frame(addr as LinkId, &bytes, now());
                    ops.extend(o);
                    evs.extend(e);
                }
                Some(Msg::Cmd(c)) => match c {
                    Cmd::Public(text) => {
                        let (o, e) = node.send_public(&text, now());
                        ops.extend(o);
                        evs.push(e);
                    }
                    Cmd::Private(peer, text) => {
                        let (o, e) = node.send_private(peer, &text, now());
                        ops.extend(o);
                        evs.extend(e);
                    }
                    Cmd::Read { peer, id } => ops.extend(node.mark_read(peer, &id, now())),
                    Cmd::Nick(n) => node.set_nick(n, now()),
                },
                None => {}
            }
            if now() >= next_sync {
                next_sync = now() + SYNC_MS;
                sync_subs(&mut links, &mut node, &mut evs, now());
            }
            let (o, e) = node.tick(now());
            ops.extend(o);
            evs.extend(e);
            for op in ops {
                exec(&node, &links, op, now());
            }
            *PEERS.lock().unwrap() = node.peers(now());
            push_events(evs);
        }
    }

    /// Reconcile `links[..].client` with the characteristic's live subscribers.
    fn sync_subs(links: &mut HashMap<u64, Link>, node: &mut Node, evs: &mut Vec<Event>, now_ms: u64) {
        let Some(lc) = local_char() else { return };
        let mut seen: HashSet<u64> = HashSet::new();
        if let Ok(list) = lc.SubscribedClients() {
            for c in list {
                let addr = c
                    .Session()
                    .and_then(|s| s.DeviceId())
                    .and_then(|d| d.Id())
                    .ok()
                    .and_then(|id| addr_of_device_id(&id));
                let Some(addr) = addr else { continue };
                seen.insert(addr);
                let l = links.entry(addr).or_insert_with(Link::new);
                let was_live = l.live();
                if let Ok(m) = c.MaxNotificationSize() {
                    l.mtu_client = (m as usize).clamp(23, 512);
                }
                if l.client.is_none() {
                    push_events([Event::Log(format!("mesh: subscriber {addr:012x}"))]);
                }
                l.client = Some(c); // refresh: the object is replaced on resubscribe
                if !was_live {
                    evs.extend(node.on_link_up(addr as LinkId, now_ms));
                }
            }
        }
        // Departed subscribers drop their side of the link.
        let gone: Vec<u64> = links.iter().filter(|(a, l)| l.client.is_some() && !seen.contains(*a)).map(|(a, _)| *a).collect();
        for addr in gone {
            if let Some(l) = links.get_mut(&addr) {
                l.client = None;
                if !l.live() {
                    links.remove(&addr);
                    evs.extend(node.on_link_down(addr as LinkId));
                    push_events([Event::Log(format!("mesh: subscriber gone {addr:012x}"))]);
                }
            }
        }
    }

    /// Execute one mesh `Op::Send` against the live link map.
    fn exec(node: &Node, links: &HashMap<u64, Link>, op: Op, now_ms: u64) {
        let Op::Send { to, bytes } = op;
        let targets: Vec<u64> = match &to {
            SendTo::All => links.keys().copied().collect(),
            SendTo::Except(x) => links.keys().copied().filter(|l| l != x).collect(),
            SendTo::Only(v) => v.clone(),
        };
        for addr in targets {
            let Some(link) = links.get(&addr) else { continue };
            send(node, link, &bytes, now_ms);
        }
    }

    /// Write one wire frame: prefer the central-side characteristic write,
    /// else a notification to the subscribed client. Fragmented per-link MTU.
    fn send(node: &Node, link: &Link, bytes: &[u8], now_ms: u64) {
        if let Some(ch) = &link.ch {
            let mut ok = true;
            for frame in Node::shard(bytes, node.peer_id(), link.mtu_ch.saturating_sub(3).max(20), now_ms) {
                let sent = to_buffer(&frame)
                    .and_then(|buf| ch.WriteValueWithResultAndOptionAsync(&buf, GattWriteOption::WriteWithoutResponse))
                    .and_then(|op| op.get())
                    .and_then(|r| r.Status())
                    .map(|s| s == GattCommunicationStatus::Success)
                    .unwrap_or(false);
                if !sent {
                    ok = false;
                    break;
                }
            }
            if ok {
                return;
            }
            push_events([Event::Log("mesh: central write failed".into())]);
            if link.client.is_none() {
                return;
            }
            // Central write died; the subscriber path may still work.
        }
        if let (Some(client), Some(lc)) = (&link.client, local_char()) {
            for frame in Node::shard(bytes, node.peer_id(), link.mtu_client.saturating_sub(3).max(20), now_ms) {
                let sent = to_buffer(&frame)
                    .and_then(|buf| lc.NotifyValueForSubscribedClientAsync(&buf, client))
                    .and_then(|op| op.get())
                    .and_then(|r| r.Status())
                    .map(|s| s == GattCommunicationStatus::Success)
                    .unwrap_or(false);
                if !sent {
                    push_events([Event::Log("mesh: notify failed".into())]);
                    break;
                }
            }
        }
    }

    // ------------------------------------------------------------------
    // Peripheral: local GATT service. The provider and characteristic are
    // returned so the caller can keep them alive for the run's lifetime.
    // ------------------------------------------------------------------

    fn peripheral(tx: &Sender<Msg>) -> windows::core::Result<(GattServiceProvider, GattLocalCharacteristic)> {
        let provider = GattServiceProvider::CreateAsync(SERVICE)?.get()?.ServiceProvider()?;
        let params = GattLocalCharacteristicParameters::new()?;
        params.SetCharacteristicProperties(
            GattCharacteristicProperties::Write | GattCharacteristicProperties::WriteWithoutResponse | GattCharacteristicProperties::Notify,
        )?;
        params.SetWriteProtectionLevel(GattProtectionLevel::Plain)?;
        let ch = provider
            .Service()?
            .CreateCharacteristicAsync(MESH_CHAR, &params)?
            .get()?
            .Characteristic()?;

        // Remote writes → frames into the loop.
        {
            let tx = tx.clone();
            ch.WriteRequested(&TypedEventHandler::new(move |_c: &Option<GattLocalCharacteristic>, args: &Option<GattWriteRequestedEventArgs>| {
                let Some(args) = args else { return Ok(()) };
                let addr = args
                    .Session()
                    .and_then(|s| s.DeviceId())
                    .and_then(|d| d.Id())
                    .ok()
                    .and_then(|id| addr_of_device_id(&id))
                    .unwrap_or(0);
                if let Ok(req) = args.GetRequestAsync().and_then(|op| op.get()) {
                    let bytes = req.Value().map(|b| from_buffer(&b)).unwrap_or_default();
                    if !bytes.is_empty() {
                        let _ = tx.send(Msg::Frame { addr, bytes });
                    }
                    // WithResponse writes need an explicit ack.
                    if req.Option().map(|o| o == GattWriteOption::WriteWithResponse).unwrap_or(true) {
                        let _ = req.Respond();
                    }
                }
                Ok(())
            }))?;
        }
        {
            let tx = tx.clone();
            ch.SubscribedClientsChanged(&TypedEventHandler::new(move |_c: &Option<GattLocalCharacteristic>, _a: &Option<windows::core::IInspectable>| {
                let _ = tx.send(Msg::Subs);
                Ok(())
            }))?;
        }

        let adv = GattServiceProviderAdvertisingParameters::new()?;
        adv.SetIsConnectable(true)?;
        adv.SetIsDiscoverable(true)?;
        provider.StartAdvertisingWithParameters(&adv)?;
        LOCAL_CHAR.with(|c| *c.borrow_mut() = Some(ch.clone()));
        Ok((provider, ch))
    }

    thread_local! {
        static LOCAL_CHAR: std::cell::RefCell<Option<GattLocalCharacteristic>> = const { std::cell::RefCell::new(None) };
    }

    fn local_char() -> Option<GattLocalCharacteristic> {
        LOCAL_CHAR.with(|c| c.borrow().clone())
    }

    // ------------------------------------------------------------------
    // Central: watcher + connect.
    // ------------------------------------------------------------------

    fn watcher(tx: &Sender<Msg>) -> windows::core::Result<BluetoothLEAdvertisementWatcher> {
        let w = BluetoothLEAdvertisementWatcher::new()?;
        let filter = BluetoothLEAdvertisementFilter::new()?;
        filter.Advertisement()?.ServiceUuids()?.Append(SERVICE)?;
        w.SetAdvertisementFilter(&filter)?;
        w.SetScanningMode(BluetoothLEScanningMode::Active)?;
        let tx = tx.clone();
        w.Received(&TypedEventHandler::new(move |_w: &Option<BluetoothLEAdvertisementWatcher>, args: &Option<BluetoothLEAdvertisementReceivedEventArgs>| {
            if let Some(a) = args {
                if let Ok(addr) = a.BluetoothAddress() {
                    // Unspecified → guess Random (phones); the dial retries
                    // Public anyway on a bad-command error.
                    let atype = match a.BluetoothAddressType() {
                        Ok(BluetoothAddressType::Public) => BluetoothAddressType::Public,
                        _ => BluetoothAddressType::Random,
                    };
                    static SEEN_LOG: std::sync::OnceLock<std::sync::Mutex<std::collections::HashSet<u64>>> = std::sync::OnceLock::new();
                    let log_set = SEEN_LOG.get_or_init(|| std::sync::Mutex::new(std::collections::HashSet::new()));
                    if log_set.lock().unwrap().insert(addr) {
                        let t = a.AdvertisementType().map(|t| format!("{t:?}")).unwrap_or_default();
                        let rssi = a.RawSignalStrengthInDBm().unwrap_or(0);
                        push_events([Event::Log(format!("mesh: ad {addr:012x} type={t} rssi={rssi}"))]);
                    }
                    let _ = tx.send(Msg::Seen(addr, atype));
                }
            }
            Ok(())
        }))?;
        Ok(w)
    }

    /// Wrap an HRESULT with the dial step it came from, so droppy.log shows
    /// exactly where connects die.
    fn step(s: impl Into<String>, e: windows::core::Error) -> windows::core::Error {
        let m = s.into();
        windows::core::Error::new(e.code(), format!("{m}: {}", e.message()))
    }

    /// Dial one advertiser: connect, subscribe to its mesh characteristic.
    /// Reports `Msg::Up` on success. Runs on a short-lived thread.
    fn dial_out(addr: u64, gen: u64, atype: BluetoothAddressType, tx: Sender<Msg>) -> windows::core::Result<()> {
        unsafe {
            let _ = RoInitialize(RO_INIT_MULTITHREADED);
        }
        let dev = BluetoothLEDevice::FromBluetoothAddressWithBluetoothAddressTypeAsync(addr, atype)
            .map_err(|e| step("connect", e))?
            .get()
            .map_err(|e| step("connect-await", e))?;
        // Uncached: without it, unpaired devices return empty service lists.
        let services = match dev
            .GetGattServicesForUuidWithCacheModeAsync(SERVICE, BluetoothCacheMode::Uncached)
            .map_err(|e| step("services", e))
        {
            Ok(op) => op.get().or_else(|e| {
                // Some stacks reject Uncached outright — retry Cached once.
                let first = format!("{e}");
                match dev.GetGattServicesForUuidWithCacheModeAsync(SERVICE, BluetoothCacheMode::Cached) {
                    Ok(op2) => op2.get().map_err(|e2| step(format!("services-await ({first}) / cached"), e2)),
                    Err(e2) => Err(step(format!("services ({first}) / cached"), e2)),
                }
            })?,
            Err(e) => return Err(e),
        };
        let svc = services.Services().map_err(|e| step("svc-list", e))?.GetAt(0).map_err(|e| step("svc-none", e))?;
        let chars = svc
            .GetCharacteristicsForUuidWithCacheModeAsync(MESH_CHAR, BluetoothCacheMode::Uncached)
            .map_err(|e| step("chars", e))?
            .get()
            .map_err(|e| step("chars-await", e))?;
        let ch = chars.Characteristics().map_err(|e| step("char-list", e))?.GetAt(0).map_err(|e| step("char-none", e))?;

        // Notifications → frames into the loop.
        {
            let tx = tx.clone();
            ch.ValueChanged(&TypedEventHandler::new(move |_c: &Option<GattCharacteristic>, args: &Option<GattValueChangedEventArgs>| {
                if let Some(a) = args {
                    let bytes = a.CharacteristicValue().map(|b| from_buffer(&b)).unwrap_or_default();
                    if !bytes.is_empty() {
                        let _ = tx.send(Msg::Frame { addr, bytes });
                    }
                }
                Ok(())
            }))?;
        }
        ch.WriteClientCharacteristicConfigurationDescriptorAsync(GattClientCharacteristicConfigurationDescriptorValue::Notify)?.get()?;

        // Disconnects drop the link.
        {
            let tx = tx.clone();
            dev.ConnectionStatusChanged(&TypedEventHandler::new(move |d: &Option<BluetoothLEDevice>, _a: &Option<windows::core::IInspectable>| {
                if let Some(d) = d {
                    if d.ConnectionStatus().map(|s| s == BluetoothConnectionStatus::Disconnected).unwrap_or(false) {
                        let _ = tx.send(Msg::Down { addr, gen });
                    }
                }
                Ok(())
            }))?;
        }

        let mtu = svc.Session().and_then(|s| s.MaxPduSize()).map(|m| m as usize).unwrap_or(DEFAULT_MTU + 3);
        let _ = tx.send(Msg::Up { addr, gen, ch, dev, mtu });
        Ok(())
    }
}
