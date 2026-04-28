//! Deterministic hashing helpers used across cache keys, auth scope keys,
//! reactor diffing, and migration checksums.
//!
//! `std::collections::hash_map::DefaultHasher` is explicitly documented as
//! unstable across Rust versions and process restarts, so it cannot be used
//! anywhere correctness depends on cross-process or cross-deploy stability
//! (cache keys during rolling deploys, persisted migration checksums, content
//! diffing between nodes). All callers should reach for the helpers here
//! instead.

use sha2::{Digest, Sha256};

/// Compute a SHA-256 digest as a lowercase hex string.
pub fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    hex_encode(&digest)
}

/// Compute a stable 64-bit fingerprint from arbitrary bytes by truncating a
/// SHA-256 digest. Suitable for cache keys and other in-memory lookups where
/// stability across binaries matters but cryptographic strength does not.
pub fn stable_u64(bytes: &[u8]) -> u64 {
    let digest = Sha256::digest(bytes);
    let mut buf = [0u8; 8];
    // SHA-256 output is always 32 bytes, so the first 8 bytes are guaranteed
    // present. Use `get` rather than slice indexing to satisfy
    // `clippy::indexing_slicing`.
    if let Some(prefix) = digest.get(..8) {
        buf.copy_from_slice(prefix);
    }
    u64::from_be_bytes(buf)
}

fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        let hi = (b >> 4) as usize;
        let lo = (b & 0x0f) as usize;
        if let (Some(&h), Some(&l)) = (HEX.get(hi), HEX.get(lo)) {
            out.push(h as char);
            out.push(l as char);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn sha256_hex_matches_known_vector() {
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[tokio::test]
    async fn stable_u64_is_deterministic() {
        assert_eq!(stable_u64(b"forge"), stable_u64(b"forge"));
        assert_ne!(stable_u64(b"forge"), stable_u64(b"forgex"));
    }
}
