use std::{
    net::IpAddr,
    time::{Duration, Instant},
};
use subtle::ConstantTimeEq;

pub fn secret() -> String {
    rand::random::<[u8; 32]>()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
fn code() -> String {
    const ALPHABET: &[u8] = b"23456789ABCDEFGHJKLMNPQRSTUVWXYZ";
    (0..10)
        .map(|_| ALPHABET[rand::random_range(0..ALPHABET.len())] as char)
        .collect()
}
fn equal(a: &str, b: &str) -> bool {
    bool::from(a.as_bytes().ct_eq(b.as_bytes()))
}

#[derive(Clone)]
pub struct Session {
    pub token: String,
    pub ip: IpAddr,
    pub approved: bool,
    pub created: Instant,
    pub last_seen: Instant,
}

pub struct Pairing {
    pub code: String,
    issued: Instant,
    window: Instant,
    attempts: u8,
    pub session: Option<Session>,
}

impl Default for Pairing {
    fn default() -> Self {
        Self::new(Instant::now())
    }
}
impl Pairing {
    pub fn new(now: Instant) -> Self {
        Self {
            code: code(),
            issued: now,
            window: now,
            attempts: 0,
            session: None,
        }
    }
    pub fn expire(&mut self, now: Instant) -> bool {
        let expired = self.session.as_ref().is_some_and(|s| {
            now.duration_since(s.last_seen) > Duration::from_secs(15)
                || (!s.approved && now.duration_since(s.created) > Duration::from_secs(60))
        });
        if expired {
            self.disconnect(now);
        }
        expired
    }
    pub fn pair(
        &mut self,
        supplied: &str,
        ip: IpAddr,
        now: Instant,
    ) -> Result<String, &'static str> {
        self.expire(now);
        if now.duration_since(self.window) >= Duration::from_secs(60) {
            self.attempts = 0;
            self.window = now;
        }
        if self.attempts >= 5 {
            return Err("Too many attempts. Wait one minute.");
        }
        self.attempts += 1;
        if now.duration_since(self.issued) > Duration::from_secs(300) {
            return Err("Pairing code expired. Restart sharing on the host.");
        }
        if !equal(&self.code, supplied) {
            return Err("Incorrect pairing code");
        }
        if self.session.is_some() {
            return Err("A viewer is already paired");
        }
        let token = secret();
        self.session = Some(Session {
            token: token.clone(),
            ip,
            approved: false,
            created: now,
            last_seen: now,
        });
        Ok(token)
    }
    pub fn authenticate(&mut self, token: &str, ip: IpAddr, now: Instant) -> Option<bool> {
        self.expire(now);
        let session = self.session.as_mut()?;
        if session.ip != ip || !equal(&session.token, token) {
            return None;
        }
        session.last_seen = now;
        Some(session.approved)
    }
    pub fn approve(&mut self, now: Instant) {
        self.expire(now);
        if let Some(s) = &mut self.session {
            s.approved = true;
            s.last_seen = now;
        }
    }
    pub fn disconnect(&mut self, now: Instant) {
        self.session = None;
        self.code = code();
        self.issued = now;
        // Keep the global attempt window: disconnect must not bypass throttling.
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn authorization_approval_expiry_and_revocation() {
        let now = Instant::now();
        let ip = "127.0.0.1".parse().unwrap();
        let mut p = Pairing::new(now);
        let token = p.pair(&p.code.clone(), ip, now).unwrap();
        assert_eq!(p.authenticate(&token, ip, now), Some(false));
        assert_eq!(
            p.authenticate(&token, "127.0.0.2".parse().unwrap(), now),
            None
        );
        p.approve(now);
        assert_eq!(p.authenticate(&token, ip, now), Some(true));
        assert_eq!(
            p.authenticate(&token, ip, now + Duration::from_secs(16)),
            None
        );
        assert!(p.session.is_none());
    }
    #[test]
    fn attempts_and_pairing_code_expire() {
        let now = Instant::now();
        let ip = "127.0.0.1".parse().unwrap();
        let mut p = Pairing::new(now);
        for _ in 0..5 {
            assert!(p.pair("wrong", ip, now).is_err());
        }
        assert!(
            p.pair(&p.code.clone(), ip, now)
                .unwrap_err()
                .contains("Too many")
        );
        assert!(
            p.pair(&p.code.clone(), ip, now + Duration::from_secs(301))
                .unwrap_err()
                .contains("expired")
        );
    }
    #[test]
    fn pending_approval_has_a_fixed_deadline_despite_heartbeats() {
        let now = Instant::now();
        let ip = "127.0.0.1".parse().unwrap();
        let mut p = Pairing::new(now);
        let token = p.pair(&p.code.clone(), ip, now).unwrap();
        for second in [10, 20, 30, 40, 50, 60] {
            assert_eq!(
                p.authenticate(&token, ip, now + Duration::from_secs(second)),
                Some(false)
            );
        }
        assert_eq!(
            p.authenticate(&token, ip, now + Duration::from_secs(61)),
            None
        );
    }
}
