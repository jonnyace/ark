//! SHA-256 helpers. Streaming so large files (videos, models) never need to be in memory at once.
use sha2::{Digest, Sha256};

#[derive(Default, Clone)]
pub struct Hasher {
    inner: Sha256,
    bytes: u64,
}

impl Hasher {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn update(&mut self, data: &[u8]) {
        self.inner.update(data);
        self.bytes += data.len() as u64;
    }
    pub fn bytes(&self) -> u64 {
        self.bytes
    }
    pub fn finish_hex(self) -> String {
        hex::encode(self.inner.finalize())
    }
}

pub fn sha256_hex(data: &[u8]) -> String {
    hex::encode(Sha256::digest(data))
}

pub fn is_sha256_hex(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn streaming_matches_oneshot() {
        let mut h = Hasher::new();
        h.update(b"hello ");
        h.update(b"world");
        assert_eq!(h.bytes(), 11);
        assert_eq!(h.finish_hex(), sha256_hex(b"hello world"));
        assert!(is_sha256_hex(&sha256_hex(b"")));
        assert!(!is_sha256_hex("ABC"));
    }
}
