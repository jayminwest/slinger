//! Blob ids: the content hash that names a blob in the record.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use sha2::{Digest, Sha256};

use crate::{Error, Result};

const PREFIX: &str = "sha256:";
const HEX_LEN: usize = 64;

/// The id of a blob: the SHA-256 of its bytes, written `sha256:<64 lowercase hex>`.
///
/// Why SHA-256 and not BLAKE3: anyone can check an id with the stock
/// `shasum -a 256` on every machine, git and OCI name content the same way,
/// and the blobs (packets, UI code) are small enough that BLAKE3's speed
/// buys nothing. The `sha256:` prefix names the algorithm, so a later one
/// can be added without breaking a reader of old ids (B5).
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct BlobId([u8; 32]);

impl BlobId {
    /// The id of `bytes`. The same bytes always give the same id.
    #[must_use]
    pub fn of(bytes: &[u8]) -> Self {
        Self(Sha256::digest(bytes).into())
    }
}

impl fmt::Display for BlobId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(PREFIX)?;
        self.0.iter().try_for_each(|b| write!(f, "{b:02x}"))
    }
}

impl fmt::Debug for BlobId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "BlobId({self})")
    }
}

impl FromStr for BlobId {
    type Err = Error;

    /// Parses `sha256:<64 lowercase hex>`, the form [`Display`](fmt::Display) writes.
    fn from_str(s: &str) -> Result<Self> {
        let invalid = || Error::Invalid {
            message: format!("`{s}` is not a blob id"),
            hint: Some("a blob id is `sha256:` and 64 lowercase hex digits".into()),
        };
        let hex = s.strip_prefix(PREFIX).ok_or_else(invalid)?;
        if hex.len() != HEX_LEN {
            return Err(invalid());
        }
        let mut out = [0u8; 32];
        for (byte, [hi, lo]) in out.iter_mut().zip(hex.as_bytes().as_chunks::<2>().0) {
            *byte = (nibble(*hi).ok_or_else(invalid)? << 4) | nibble(*lo).ok_or_else(invalid)?;
        }
        Ok(Self(out))
    }
}

/// One lowercase hex digit. Upper case is refused so each id has one spelling.
fn nibble(c: u8) -> Option<u8> {
    match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'a'..=b'f' => Some(c - b'a' + 10),
        _ => None,
    }
}

impl Serialize for BlobId {
    fn serialize<S: Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        s.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for BlobId {
    fn deserialize<D: Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        s.parse().map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const EMPTY: &str = "sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

    #[test]
    fn hashes_bytes_with_sha256() {
        assert_eq!(BlobId::of(b"").to_string(), EMPTY);
        assert_eq!(
            BlobId::of(b"abc").to_string(),
            "sha256:ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn parse_reverses_display() {
        let id = BlobId::of(b"packet");
        assert_eq!(id.to_string().parse::<BlobId>().unwrap(), id);
        assert_eq!(EMPTY.parse::<BlobId>().unwrap(), BlobId::of(b""));
    }

    #[test]
    fn parse_refuses_other_spellings() {
        let hex = &EMPTY[PREFIX.len()..];
        for bad in [
            hex.to_string(),
            format!("blake3:{hex}"),
            EMPTY.to_uppercase(),
            format!("sha256:{}", &hex[1..]),
            format!("{EMPTY}0"),
            format!("sha256:{}g", &hex[1..]),
        ] {
            let err = bad.parse::<BlobId>().unwrap_err();
            assert_eq!(err.code(), "invalid", "{bad}");
            assert!(err.hint().is_some());
        }
    }

    #[test]
    fn serializes_as_its_string() {
        let id = BlobId::of(b"x");
        let json = serde_json::to_string(&id).unwrap();
        assert_eq!(json, format!("\"{id}\""));
        assert_eq!(serde_json::from_str::<BlobId>(&json).unwrap(), id);
        assert!(serde_json::from_str::<BlobId>("\"nope\"").is_err());
        assert_eq!(format!("{id:?}"), format!("BlobId({id})"));
    }
}
