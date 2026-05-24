//! Content-addressable digests (`sha256:<hex>`).
//!
//! Ported from fastregistry `pkg/digest/digest.go`.

use std::fmt;

use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};

/// A content-addressable digest, e.g. `sha256:abc123…`. Serializes as the
/// bare string, matching how it appears in OCI manifests.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Digest(pub String);

impl Digest {
    /// Algorithm part (e.g. `sha256`), or `""` if malformed.
    pub fn algorithm(&self) -> &str {
        self.0.split_once(':').map(|(a, _)| a).unwrap_or("")
    }

    /// Hex part, or `""` if malformed.
    pub fn hex(&self) -> &str {
        self.0.split_once(':').map(|(_, h)| h).unwrap_or("")
    }

    /// First 12 hex chars, for logging.
    pub fn short_hex(&self) -> &str {
        let h = self.hex();
        &h[..h.len().min(12)]
    }

    /// Check the digest is a well-formed sha256.
    pub fn validate(&self) -> Result<(), DigestError> {
        if self.algorithm().is_empty() || self.hex().is_empty() {
            return Err(DigestError::Format(self.0.clone()));
        }
        if self.algorithm() != "sha256" {
            return Err(DigestError::Algorithm(self.algorithm().to_string()));
        }
        if self.hex().len() != 64 {
            return Err(DigestError::Length(self.hex().len()));
        }
        Ok(())
    }

    /// Compute the sha256 digest of a byte slice.
    pub fn from_bytes(data: &[u8]) -> Self {
        let mut h = Sha256::new();
        h.update(data);
        Digest(format!("sha256:{}", hex::encode(h.finalize())))
    }

    /// Parse and validate a digest string.
    pub fn parse(s: &str) -> Result<Self, DigestError> {
        let d = Digest(s.to_string());
        d.validate()?;
        Ok(d)
    }
}

impl fmt::Display for Digest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl AsRef<str> for Digest {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

/// Errors from digest parsing/validation.
#[derive(Debug, thiserror::Error)]
pub enum DigestError {
    /// Not in `algorithm:hex` form.
    #[error("invalid digest format: {0}")]
    Format(String),
    /// Algorithm other than sha256.
    #[error("unsupported digest algorithm: {0}")]
    Algorithm(String),
    /// Hex length is not 64.
    #[error("invalid sha256 hex length: {0}")]
    Length(usize),
}

/// Streaming verifier: feed bytes, then check against the expected digest.
pub struct Verifier {
    expected: Digest,
    hash: Sha256,
}

impl Verifier {
    /// Create a verifier for an expected (validated) digest.
    pub fn new(expected: Digest) -> Result<Self, DigestError> {
        expected.validate()?;
        Ok(Self {
            expected,
            hash: Sha256::new(),
        })
    }

    /// Feed content.
    pub fn update(&mut self, data: &[u8]) {
        self.hash.update(data);
    }

    /// True if the fed content matches the expected digest.
    pub fn verified(self) -> bool {
        hex::encode(self.hash.finalize()).eq_ignore_ascii_case(self.expected.hex())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parts() {
        let d = Digest("sha256:".to_string() + &"a".repeat(64));
        assert_eq!(d.algorithm(), "sha256");
        assert_eq!(d.hex().len(), 64);
        assert_eq!(d.short_hex(), "aaaaaaaaaaaa");
        assert!(d.validate().is_ok());
    }

    #[test]
    fn rejects_bad() {
        assert!(Digest("nope".into()).validate().is_err());
        assert!(Digest("md5:abc".into()).validate().is_err());
        assert!(Digest("sha256:short".into()).validate().is_err());
    }

    #[test]
    fn from_bytes_roundtrip() {
        let d = Digest::from_bytes(b"hello");
        assert!(d.validate().is_ok());
        let mut v = Verifier::new(d).unwrap();
        v.update(b"hello");
        assert!(v.verified());
    }

    #[test]
    fn verifier_detects_mismatch() {
        let d = Digest::from_bytes(b"hello");
        let mut v = Verifier::new(d).unwrap();
        v.update(b"world");
        assert!(!v.verified());
    }
}
