//! §5.1 Live sessions: Noise `XX` — Curve25519, ChaCha20-Poly1305, SHA-256 —
//! via `snow`. Three handshake frames ride in `NoiseHandshake` packets;
//! after that every private payload is session ciphertext.

use snow::{HandshakeState, TransportState};

const PATTERN: &str = "Noise_XX_25519_ChaChaPoly_SHA256";

enum Inner {
    Hs(HandshakeState),
    Ts(TransportState),
}

/// One peer session: either mid-handshake or an established transport.
/// (`Option` only so `promote` can move the handshake state out.)
pub struct Session {
    inner: Option<Inner>,
    /// Next outgoing transport nonce (BitChat prefixes it, 4 bytes BE).
    send_n: u32,
    /// Highest received nonce + 1024-wide replay bitmap (BitChat's window).
    recv_hi: Option<u32>,
    recv_seen: std::collections::HashSet<u32>,
}

const REPLAY_WINDOW: u32 = 1024;

fn params() -> snow::params::NoiseParams {
    PATTERN.parse().expect("static pattern is valid")
}

impl Session {
    pub fn initiator(static_sk: &[u8; 32]) -> Result<Self, snow::Error> {
        Ok(Self { inner: Some(Inner::Hs(snow::Builder::new(params()).local_private_key(static_sk)?.build_initiator()?)), send_n: 0, recv_hi: None, recv_seen: Default::default() })
    }

    pub fn responder(static_sk: &[u8; 32]) -> Result<Self, snow::Error> {
        Ok(Self { inner: Some(Inner::Hs(snow::Builder::new(params()).local_private_key(static_sk)?.build_responder()?)), send_n: 0, recv_hi: None, recv_seen: Default::default() })
    }

    /// Produce the next handshake frame (initiator's 1st and 3rd flight).
    /// Returns `(bytes, finished)`.
    pub fn write_handshake(&mut self) -> Result<(Vec<u8>, bool), snow::Error> {
        match self.inner.as_mut() {
            Some(Inner::Hs(hs)) => {
                let mut buf = vec![0u8; 1024];
                let n = hs.write_message(&[], &mut buf)?;
                buf.truncate(n);
                let done = hs.is_handshake_finished();
                if done {
                    self.promote()?;
                }
                Ok((buf, done))
            }
            _ => Err(snow::Error::State(snow::error::StateProblem::HandshakeAlreadyFinished)),
        }
    }

    /// Consume a handshake frame; returns a reply frame when we owe one
    /// (responder's 2nd flight) plus the finished flag.
    pub fn read_handshake(&mut self, msg: &[u8]) -> Result<(Option<Vec<u8>>, bool), snow::Error> {
        match self.inner.as_mut() {
            Some(Inner::Hs(hs)) => {
                let mut buf = vec![0u8; 1024];
                hs.read_message(msg, &mut buf)?;
                let mut reply = None;
                if !hs.is_handshake_finished() && hs.is_my_turn() {
                    let n = hs.write_message(&[], &mut buf)?;
                    buf.truncate(n);
                    reply = Some(buf);
                }
                let done = hs.is_handshake_finished();
                if done {
                    self.promote()?;
                }
                Ok((reply, done))
            }
            _ => Err(snow::Error::State(snow::error::StateProblem::HandshakeAlreadyFinished)),
        }
    }

    fn promote(&mut self) -> Result<(), snow::Error> {
        if let Some(Inner::Hs(hs)) = self.inner.take() {
            self.inner = Some(Inner::Ts(hs.into_transport_mode()?));
            Ok(())
        } else {
            Err(snow::Error::State(snow::error::StateProblem::HandshakeAlreadyFinished))
        }
    }

    /// Remote's static Noise key — available once its handshake flight
    /// carried `s` (XX msg 2/3). `None` earlier or pre-handshake.
    pub fn remote_static(&self) -> Option<[u8; 32]> {
        let raw = match self.inner.as_ref()? {
            Inner::Hs(h) => h.get_remote_static(),
            Inner::Ts(t) => t.get_remote_static(),
        }?;
        if raw.len() != 32 {
            return None;
        }
        let mut k = [0u8; 32];
        k.copy_from_slice(raw);
        Some(k)
    }

    pub fn is_transport(&self) -> bool {
        matches!(self.inner, Some(Inner::Ts(_)))
    }

    /// Encrypt a private payload. Wire form (BitChat `useExtractedNonce`):
    /// `[nonce u32 BE][ciphertext+tag]`.
    pub fn encrypt(&mut self, plain: &[u8]) -> Option<Vec<u8>> {
        let Some(Inner::Ts(t)) = self.inner.as_mut() else { return None };
        if self.send_n == u32::MAX {
            return None;
        }
        let mut buf = vec![0u8; plain.len() + 64];
        let n = t.write_message(plain, &mut buf).ok()?;
        let mut out = Vec::with_capacity(4 + n);
        out.extend_from_slice(&self.send_n.to_be_bytes());
        out.extend_from_slice(&buf[..n]);
        self.send_n += 1;
        Some(out)
    }

    pub fn decrypt(&mut self, wire: &[u8]) -> Option<Vec<u8>> {
        let Some(Inner::Ts(t)) = self.inner.as_mut() else { return None };
        if wire.len() < 4 + 16 {
            return None;
        }
        let nonce = u32::from_be_bytes(wire[..4].try_into().ok()?);
        if let Some(hi) = self.recv_hi {
            if nonce <= hi && hi - nonce >= REPLAY_WINDOW {
                return None;
            }
            if self.recv_seen.contains(&nonce) {
                return None;
            }
        }
        t.set_receiving_nonce(nonce as u64);
        let cipher = &wire[4..];
        let mut buf = vec![0u8; cipher.len().max(64)];
        let n = t.read_message(cipher, &mut buf).ok()?;
        buf.truncate(n);
        self.recv_seen.insert(nonce);
        let hi = self.recv_hi.map_or(nonce, |h| h.max(nonce));
        self.recv_hi = Some(hi);
        self.recv_seen.retain(|&x| hi - x < REPLAY_WINDOW);
        Some(buf)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mesh::Identity;

    fn pair() -> (Session, Session) {
        let a = Identity::generate();
        let b = Identity::generate();
        (Session::initiator(&a.noise_sk).unwrap(), Session::responder(&b.noise_sk).unwrap())
    }

    #[test]
    fn xx_three_flights_then_transport() {
        let (mut alice, mut bob) = pair();
        let (m1, d) = alice.write_handshake().unwrap();
        assert!(!d);
        let (r2, d2) = bob.read_handshake(&m1).unwrap();
        assert!(!d2);
        let (m3, _) = alice.read_handshake(&r2.unwrap()).unwrap();
        let (_, done_bob) = bob.read_handshake(&m3.unwrap()).unwrap();
        assert!(done_bob);
        assert!(alice.is_transport() && bob.is_transport());

        let ct = alice.encrypt(b"hi bob").unwrap();
        assert_eq!(bob.decrypt(&ct).unwrap(), b"hi bob");
        let ct = bob.encrypt(b"hi alice").unwrap();
        assert_eq!(alice.decrypt(&ct).unwrap(), b"hi alice");
    }

    #[test]
    fn remote_static_pins_identity() {
        let a = Identity::generate();
        let b = Identity::generate();
        let mut alice = Session::initiator(&a.noise_sk).unwrap();
        let mut bob = Session::responder(&b.noise_sk).unwrap();
        let (m1, _) = alice.write_handshake().unwrap();
        let (r2, _) = bob.read_handshake(&m1).unwrap();
        let (m3, _) = alice.read_handshake(&r2.unwrap()).unwrap();
        let _ = bob.read_handshake(&m3.unwrap()).unwrap();
        assert_eq!(alice.remote_static(), Some(b.noise_pk));
        assert_eq!(bob.remote_static(), Some(a.noise_pk));
    }
}
