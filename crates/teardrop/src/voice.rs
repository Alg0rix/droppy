//! Voice recorder: WASAPI shared-mode microphone capture to 16-bit PCM WAV.
//!
//! Recording only ever starts from a click. The capture runs on its own MTA
//! thread; `snapshot()` is what the UI polls. The WAV header is patched with
//! the real sizes when recording stops (also on quit), so files always play.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum VoicePhase {
    #[default]
    Idle,
    Recording,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct VoiceView {
    pub phase: VoicePhase,
    pub elapsed_ms: u64,
    /// Smoothed microphone level 0..1.
    pub level: f32,
    /// File name of the last finished recording.
    pub saved: Option<String>,
    /// Why recording failed (when `phase` is `Failed`).
    pub error: String,
}

static STATE: Mutex<Option<VoiceView>> = Mutex::new(None);
static STOP: AtomicBool = AtomicBool::new(false);
/// Full path of a recording nobody has collected yet.
static FINISHED: Mutex<Option<PathBuf>> = Mutex::new(None);

/// Auto-stop so a forgotten recording can't fill the disk.
const MAX_MS: u64 = 2 * 60 * 60 * 1000;

fn update(f: impl FnOnce(&mut VoiceView)) {
    if let Ok(mut s) = STATE.lock() {
        f(s.get_or_insert_with(VoiceView::default));
    }
}

pub fn snapshot() -> VoiceView {
    STATE.lock().ok().and_then(|s| s.clone()).unwrap_or_default()
}

pub fn recording() -> bool {
    snapshot().phase == VoicePhase::Recording
}

/// A finished recording's full path, once (so the app can add it to the shelf).
pub fn take_finished() -> Option<PathBuf> {
    FINISHED.lock().ok().and_then(|mut f| f.take())
}

pub fn recordings_dir() -> PathBuf {
    let home = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME")).map(PathBuf::from).unwrap_or_else(std::env::temp_dir);
    let music = home.join("Music");
    let dir = music.join("Droppy Recordings");
    // Recordings made before the rename live in "Teardrop Recordings".
    crate::paths::migrate_dir(&music.join("Teardrop Recordings"), &dir);
    dir
}

pub fn toggle() {
    if recording() {
        STOP.store(true, Ordering::SeqCst);
    } else {
        start();
    }
}

/// Stop and wait (briefly) for the file to be finalized; used on quit.
pub fn stop_and_wait() {
    if !recording() {
        return;
    }
    STOP.store(true, Ordering::SeqCst);
    for _ in 0..40 {
        if !recording() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
}

fn start() {
    STOP.store(false, Ordering::SeqCst);
    update(|v| {
        v.phase = VoicePhase::Recording;
        v.elapsed_ms = 0;
        v.level = 0.0;
        v.error.clear();
    });
    std::thread::spawn(|| {
        let name = format!("Recording {}.wav", chrono::Local::now().format("%Y-%m-%d %H-%M-%S"));
        let path = recordings_dir().join(name);
        match capture(&path, false) {
            Ok(()) => {
                let saved = path.file_name().map(|n| n.to_string_lossy().into_owned());
                update(|v| {
                    v.phase = VoicePhase::Idle;
                    v.level = 0.0;
                    v.saved = saved;
                });
                if let Ok(mut f) = FINISHED.lock() {
                    *f = Some(path);
                }
            }
            Err(msg) => {
                let _ = std::fs::remove_file(&path);
                update(|v| {
                    v.phase = VoicePhase::Failed;
                    v.level = 0.0;
                    v.error = msg;
                });
            }
        }
    });
}

// ------------------------------------------------------------------ WAV bits

/// 44-byte canonical PCM WAV header.
pub fn wav_header(channels: u16, rate: u32, bits: u16, data_len: u32) -> [u8; 44] {
    let block = channels * bits / 8;
    let mut h = [0u8; 44];
    h[0..4].copy_from_slice(b"RIFF");
    h[4..8].copy_from_slice(&(36 + data_len).to_le_bytes());
    h[8..16].copy_from_slice(b"WAVEfmt ");
    h[16..20].copy_from_slice(&16u32.to_le_bytes());
    h[20..22].copy_from_slice(&1u16.to_le_bytes());
    h[22..24].copy_from_slice(&channels.to_le_bytes());
    h[24..28].copy_from_slice(&rate.to_le_bytes());
    h[28..32].copy_from_slice(&(rate * block as u32).to_le_bytes());
    h[32..34].copy_from_slice(&block.to_le_bytes());
    h[34..36].copy_from_slice(&bits.to_le_bytes());
    h[36..40].copy_from_slice(b"data");
    h[40..44].copy_from_slice(&data_len.to_le_bytes());
    h
}

pub fn f32_to_i16(x: f32) -> i16 {
    (x.clamp(-1.0, 1.0) * 32767.0).round() as i16
}

/// Sample encodings WASAPI shared mode hands us.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sample {
    F32,
    I16,
    I32,
}

/// Convert one capture packet to little-endian 16-bit PCM; returns the peak (0..1).
pub fn to_pcm16(raw: &[u8], kind: Sample, out: &mut Vec<u8>) -> f32 {
    let mut peak = 0.0f32;
    let mut push = |v: i16| {
        peak = peak.max((v as f32 / 32768.0).abs());
        out.extend_from_slice(&v.to_le_bytes());
    };
    match kind {
        Sample::F32 => raw.chunks_exact(4).for_each(|b| push(f32_to_i16(f32::from_le_bytes([b[0], b[1], b[2], b[3]])))),
        Sample::I16 => raw.chunks_exact(2).for_each(|b| push(i16::from_le_bytes([b[0], b[1]]))),
        Sample::I32 => raw.chunks_exact(4).for_each(|b| push((i32::from_le_bytes([b[0], b[1], b[2], b[3]]) >> 16) as i16)),
    }
    peak
}

/// Rewrite the header of a finished file with its real data length.
fn finalize(path: &Path, channels: u16, rate: u32, data_len: u64) -> std::io::Result<()> {
    use std::io::{Seek, SeekFrom, Write};
    let mut f = std::fs::OpenOptions::new().write(true).open(path)?;
    f.seek(SeekFrom::Start(0))?;
    f.write_all(&wav_header(channels, rate, 16, data_len.min(u32::MAX as u64 - 64) as u32))?;
    f.flush()
}

#[cfg(not(windows))]
fn capture(_path: &Path, _loopback: bool) -> Result<(), String> {
    Err("Recording needs Windows".into())
}

/// `loopback` records what the speakers play instead of the microphone; only
/// the tests use it (it needs no microphone permission).
#[cfg(windows)]
fn capture(path: &Path, loopback: bool) -> Result<(), String> {
    use std::io::Write;
    use std::time::{Duration, Instant};
    use windows::core::HRESULT;
    use windows::Win32::Media::Audio::{
        eCapture, eConsole, eRender, IAudioCaptureClient, IAudioClient, IMMDeviceEnumerator, MMDeviceEnumerator, AUDCLNT_SHAREMODE_SHARED, WAVEFORMATEX,
    };
    use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CoTaskMemFree, CLSCTX_ALL, COINIT_MULTITHREADED};

    const E_ACCESSDENIED: i32 = 0x8007_0005u32 as i32;
    const E_NOTFOUND: i32 = 0x8007_0490u32 as i32;
    let explain = |code: HRESULT, what: &str| -> String {
        match code.0 {
            E_ACCESSDENIED => "Windows denied microphone access. Check Settings \u{203a} Privacy \u{203a} Microphone.".into(),
            E_NOTFOUND => "No microphone found.".into(),
            _ => format!("Couldn't open the microphone ({what})."),
        }
    };

    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        let en: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL).map_err(|e| explain(e.code(), "enumerator"))?;
        let dev = en.GetDefaultAudioEndpoint(if loopback { eRender } else { eCapture }, eConsole).map_err(|e| explain(e.code(), "device"))?;
        let client: IAudioClient = dev.Activate(CLSCTX_ALL, None).map_err(|e| explain(e.code(), "activate"))?;
        let fmt = client.GetMixFormat().map_err(|e| explain(e.code(), "format"))?;
        let wf: WAVEFORMATEX = std::ptr::read_unaligned(fmt);
        let (tag, channels, rate, bits, block) = (wf.wFormatTag, wf.nChannels, wf.nSamplesPerSec, wf.wBitsPerSample, wf.nBlockAlign as usize);
        // 0x0003 = IEEE float; 0xFFFE = extensible (shared mode mixes in float32).
        let kind = match (bits, tag) {
            (32, 3) | (32, 0xFFFE) => Some(Sample::F32),
            (16, _) => Some(Sample::I16),
            (32, 1) => Some(Sample::I32),
            _ => None,
        };
        let Some(kind) = kind else {
            CoTaskMemFree(Some(fmt as *const _));
            return Err("This microphone's format isn't supported.".into());
        };
        let stream_flags = if loopback { 0x0002_0000 } else { 0 }; // AUDCLNT_STREAMFLAGS_LOOPBACK
        let init = client.Initialize(AUDCLNT_SHAREMODE_SHARED, stream_flags, 10_000_000, 0, fmt, None);
        CoTaskMemFree(Some(fmt as *const _));
        init.map_err(|e| explain(e.code(), "initialize"))?;
        let cap: IAudioCaptureClient = client.GetService().map_err(|e| explain(e.code(), "service"))?;

        std::fs::create_dir_all(path.parent().unwrap_or(Path::new("."))).map_err(|_| "Couldn't create the recordings folder.".to_string())?;
        let mut file = std::io::BufWriter::new(std::fs::File::create(path).map_err(|_| "Couldn't create the recording file.".to_string())?);
        file.write_all(&wav_header(channels, rate, 16, 0)).map_err(|_| "Couldn't write the recording.".to_string())?;

        client.Start().map_err(|e| explain(e.code(), "start"))?;
        let t0 = Instant::now();
        let (mut data_len, mut level, mut pcm) = (0u64, 0.0f32, Vec::<u8>::new());
        let mut failed = None;
        'rec: loop {
            std::thread::sleep(Duration::from_millis(15));
            let mut peak = 0.0f32;
            loop {
                let n = match cap.GetNextPacketSize() {
                    Ok(n) => n,
                    Err(e) => {
                        failed = Some(explain(e.code(), "read"));
                        break 'rec;
                    }
                };
                if n == 0 {
                    break;
                }
                let (mut ptr, mut frames, mut flags) = (std::ptr::null_mut::<u8>(), 0u32, 0u32);
                if cap.GetBuffer(&mut ptr, &mut frames, &mut flags, None, None).is_err() {
                    break 'rec;
                }
                let bytes = frames as usize * block;
                pcm.clear();
                if flags & 0x2 != 0 || ptr.is_null() {
                    // AUDCLNT_BUFFERFLAGS_SILENT
                    pcm.resize(frames as usize * channels as usize * 2, 0);
                } else {
                    peak = peak.max(to_pcm16(std::slice::from_raw_parts(ptr, bytes), kind, &mut pcm));
                }
                let _ = cap.ReleaseBuffer(frames);
                if file.write_all(&pcm).is_err() {
                    failed = Some("Couldn't write the recording (disk full?).".into());
                    break 'rec;
                }
                data_len += pcm.len() as u64;
            }
            // Fast attack, slow release so the meter reads like a meter.
            level = if peak > level { peak } else { level * 0.82 };
            let ms = t0.elapsed().as_millis() as u64;
            update(|v| {
                v.elapsed_ms = ms;
                v.level = level;
            });
            if STOP.load(Ordering::SeqCst) || ms >= MAX_MS {
                break;
            }
        }
        let _ = client.Stop();
        let flushed = file.flush();
        drop(file);
        if let Some(msg) = failed {
            if data_len == 0 {
                return Err(msg);
            }
        }
        flushed.map_err(|_| "Couldn't finish the recording.".to_string())?;
        finalize(path, channels, rate, data_len).map_err(|_| "Couldn't finish the recording.".to_string())?;
        Ok(())
    }
}

/// "0:07", "12:34", "1:02:03".
pub fn clock(ms: u64) -> String {
    let s = ms / 1000;
    if s >= 3600 {
        format!("{}:{:02}:{:02}", s / 3600, s % 3600 / 60, s % 60)
    } else {
        format!("{}:{:02}", s / 60, s % 60)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wav_header_layout() {
        let h = wav_header(2, 48_000, 16, 1000);
        assert_eq!(&h[0..4], b"RIFF");
        assert_eq!(u32::from_le_bytes([h[4], h[5], h[6], h[7]]), 1036);
        assert_eq!(&h[8..16], b"WAVEfmt ");
        assert_eq!(u16::from_le_bytes([h[22], h[23]]), 2);
        assert_eq!(u32::from_le_bytes([h[24], h[25], h[26], h[27]]), 48_000);
        assert_eq!(u32::from_le_bytes([h[28], h[29], h[30], h[31]]), 192_000, "byte rate");
        assert_eq!(u16::from_le_bytes([h[32], h[33]]), 4, "block align");
        assert_eq!(&h[36..40], b"data");
        assert_eq!(u32::from_le_bytes([h[40], h[41], h[42], h[43]]), 1000);
    }

    #[test]
    fn float_samples_convert_and_clip() {
        assert_eq!(f32_to_i16(0.0), 0);
        assert_eq!(f32_to_i16(1.0), 32767);
        assert_eq!(f32_to_i16(-1.0), -32767);
        assert_eq!(f32_to_i16(3.0), 32767);
        let raw: Vec<u8> = [0.5f32, -0.25].iter().flat_map(|f| f.to_le_bytes()).collect();
        let mut out = Vec::new();
        let peak = to_pcm16(&raw, Sample::F32, &mut out);
        assert_eq!(out.len(), 4);
        assert!((peak - 0.5).abs() < 0.001);
        assert_eq!(i16::from_le_bytes([out[0], out[1]]), 16384);
    }

    #[test]
    fn finalize_patches_sizes() {
        let dir = std::env::temp_dir().join(format!("teardrop-voice-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("t.wav");
        let mut bytes = wav_header(1, 16_000, 16, 0).to_vec();
        bytes.extend_from_slice(&[1, 0, 2, 0, 3, 0]);
        std::fs::write(&p, &bytes).unwrap();
        finalize(&p, 1, 16_000, 6).unwrap();
        let back = std::fs::read(&p).unwrap();
        assert_eq!(u32::from_le_bytes([back[40], back[41], back[42], back[43]]), 6);
        assert_eq!(u32::from_le_bytes([back[4], back[5], back[6], back[7]]), 42);
        assert_eq!(back.len(), 50);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn clock_text() {
        assert_eq!(clock(7_400), "0:07");
        assert_eq!(clock(754_000), "12:34");
        assert_eq!(clock(3_723_000), "1:02:03");
    }
}

#[cfg(all(test, windows))]
mod live {
    /// Real microphone, briefly, file deleted afterwards:
    /// `cargo test -p teardrop live_mic -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn live_mic() {
        run(false);
    }

    /// Records the speakers while a short beep plays (no mic permission needed),
    /// to exercise packet reading and WAV writing end to end.
    #[test]
    #[ignore]
    fn live_loopback() {
        run(true);
    }

    fn run(loopback: bool) {
        let path = std::env::temp_dir().join("teardrop-live-capture.wav");
        super::STOP.store(false, std::sync::atomic::Ordering::SeqCst);
        let stopper = std::thread::spawn(move || {
            if loopback {
                std::thread::sleep(std::time::Duration::from_millis(300));
                unsafe {
                    let _ = windows::Win32::System::Diagnostics::Debug::Beep(660, 700);
                }
                std::thread::sleep(std::time::Duration::from_millis(300));
            } else {
                std::thread::sleep(std::time::Duration::from_millis(1200));
            }
            super::STOP.store(true, std::sync::atomic::Ordering::SeqCst);
        });
        let r = super::capture(&path, loopback);
        stopper.join().unwrap();
        println!("capture result: {r:?}");
        if r.is_ok() {
            let b = std::fs::read(&path).unwrap();
            let data = u32::from_le_bytes([b[40], b[41], b[42], b[43]]) as usize;
            let peak = b[44..].chunks_exact(2).map(|c| i16::from_le_bytes([c[0], c[1]]).unsigned_abs()).max().unwrap_or(0);
            println!(
                "file {} bytes, data {} bytes, channels {}, rate {}, peak {}",
                b.len(),
                data,
                u16::from_le_bytes([b[22], b[23]]),
                u32::from_le_bytes([b[24], b[25], b[26], b[27]]),
                peak
            );
            assert_eq!(b.len(), 44 + data);
            assert!(data > 0);
        }
        let _ = std::fs::remove_file(&path);
    }
}
