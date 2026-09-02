use std::fmt::{Display, Formatter};
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use sha2::{Digest as _, Sha256};

use crate::error::{Result, VaultcError};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ContentHash([u8; 32]);

impl ContentHash {
    pub fn from_bytes(bytes: &[u8]) -> Self {
        Self::from_domain_bytes("vaultc:content:v1\0", bytes)
    }

    pub fn from_domain_bytes(domain: &str, bytes: &[u8]) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(domain.as_bytes());
        hasher.update(bytes);
        Self(hasher.finalize().into())
    }

    pub fn from_parts(domain: &str, parts: &[&[u8]]) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(domain.as_bytes());
        for part in parts {
            write_uleb128(&mut hasher, part.len() as u64);
            hasher.update(part);
        }
        Self(hasher.finalize().into())
    }

    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    pub fn hex(&self) -> String {
        hex::encode(self.0)
    }

    pub fn parse_hex(value: &str) -> Result<Self> {
        if value.len() != 64
            || !value
                .bytes()
                .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
        {
            return Err(VaultcError::MalformedInput {
                path: "identity".into(),
                reason: "expected exactly 64 lowercase hexadecimal characters".into(),
            });
        }
        let decoded = hex::decode(value).map_err(|error| VaultcError::MalformedInput {
            path: "identity".into(),
            reason: error.to_string(),
        })?;
        let bytes: [u8; 32] = decoded
            .try_into()
            .map_err(|_| VaultcError::MalformedInput {
                path: "identity".into(),
                reason: "expected a 32-byte SHA-256 value".into(),
            })?;
        Ok(Self(bytes))
    }
}

impl Display for ContentHash {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.hex())
    }
}

impl Serialize for ContentHash {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.hex())
    }
}

impl<'de> Deserialize<'de> for ContentHash {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::parse_hex(&value).map_err(serde::de::Error::custom)
    }
}

macro_rules! typed_id {
    ($name:ident, $prefix:literal) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(pub(crate) ContentHash);

        impl $name {
            pub fn from_hash(hash: ContentHash) -> Self {
                Self(hash)
            }

            pub fn from_parts(domain: &str, parts: &[&[u8]]) -> Self {
                Self(ContentHash::from_parts(domain, parts))
            }

            pub fn hash(&self) -> ContentHash {
                self.0
            }

            pub fn suffix_base32(&self, characters: usize) -> String {
                crockford_base32(self.0.as_bytes())
                    .chars()
                    .take(characters.min(52))
                    .collect()
            }
        }

        impl Display for $name {
            fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
                write!(formatter, concat!($prefix, "_{}"), self.0)
            }
        }

        impl FromStr for $name {
            type Err = VaultcError;

            fn from_str(value: &str) -> Result<Self> {
                let hex = value.strip_prefix(concat!($prefix, "_")).ok_or_else(|| {
                    VaultcError::MalformedInput {
                        path: "identity".into(),
                        reason: format!("expected {}_ prefix", $prefix),
                    }
                })?;
                Ok(Self(ContentHash::parse_hex(hex)?))
            }
        }

        impl Serialize for $name {
            fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
            where
                S: Serializer,
            {
                serializer.serialize_str(&self.to_string())
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
            where
                D: Deserializer<'de>,
            {
                let value = String::deserialize(deserializer)?;
                value.parse().map_err(serde::de::Error::custom)
            }
        }
    };
}

typed_id!(SnapshotId, "snap");
typed_id!(SourceFileId, "file");
typed_id!(DocumentId, "doc");
typed_id!(BlockId, "block");
typed_id!(LinkId, "link");
typed_id!(AssetId, "asset");
typed_id!(CanvasId, "canvas");
typed_id!(BaseArtifactId, "base");
typed_id!(PlanId, "plan");
typed_id!(OperationId, "op");
typed_id!(EvidenceId, "evidence");
typed_id!(RecordId, "record");

impl SourceFileId {
    /// Compute ALG-SNP-001's file identity. Unlike the generic `from_parts`
    /// helper, the fixed-width content hash is not length-prefixed.
    pub fn from_file(path: &str, kind: &str, content_hash: ContentHash) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(b"vaultc:file:v1\0");
        write_length_prefixed(&mut hasher, path.as_bytes());
        write_length_prefixed(&mut hasher, kind.as_bytes());
        hasher.update(content_hash.as_bytes());
        Self(ContentHash(hasher.finalize().into()))
    }
}

impl SnapshotId {
    /// Compute the V1 snapshot identity from an already path-sorted manifest.
    ///
    /// The manifest member encoding is deliberately not generic JSON: the
    /// normative identity formula commits `lp(path) || SourceFileId` for each
    /// member, after the two length-prefixed snapshot attributes.
    pub fn from_manifest<'a>(
        source_id: &str,
        policy_id: &str,
        manifest: impl IntoIterator<Item = (&'a str, SourceFileId)>,
    ) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(b"vaultc:snapshot:v1\0");
        write_length_prefixed(&mut hasher, source_id.as_bytes());
        write_length_prefixed(&mut hasher, policy_id.as_bytes());
        for (path, file_id) in manifest {
            write_length_prefixed(&mut hasher, path.as_bytes());
            hasher.update(file_id.hash().as_bytes());
        }
        Self(ContentHash(hasher.finalize().into()))
    }
}

impl EvidenceId {
    /// Compute ALG-PRV-001's fixed-width, mode-tagged evidence identity.
    /// Display prefixes and the generic length-prefixed encoder are
    /// deliberately excluded from this identity formula.
    pub fn from_evidence(evidence: &vaultc_protocol::EvidenceRefWire) -> Result<Self> {
        let snapshot_id = evidence.snapshot_id.parse::<SnapshotId>()?;
        let document_id = evidence.document_id.parse::<DocumentId>()?;
        let block_id = evidence
            .block_id
            .as_deref()
            .map(str::parse::<BlockId>)
            .transpose()?;
        let span = match (evidence.byte_start, evidence.byte_end) {
            (None, None) => None,
            (Some(start), Some(end)) => Some((start, end)),
            _ => {
                return Err(VaultcError::MalformedInput {
                    path: "evidence".into(),
                    reason: "evidence span must contain both start and end".into(),
                });
            }
        };
        let content_hash = ContentHash::parse_hex(&evidence.content_hash)?;
        Self::from_components(snapshot_id, document_id, block_id, span, content_hash)
    }

    fn from_components(
        snapshot_id: SnapshotId,
        document_id: DocumentId,
        block_id: Option<BlockId>,
        span: Option<(u64, u64)>,
        content_hash: ContentHash,
    ) -> Result<Self> {
        let mode = match (block_id, span) {
            (None, None) => 0_u8,
            (Some(_), None) => 1_u8,
            (Some(_), Some((start, end))) if start <= end => 2_u8,
            (Some(_), Some(_)) => {
                return Err(VaultcError::MalformedInput {
                    path: "evidence".into(),
                    reason: "evidence span start must not exceed end".into(),
                });
            }
            (None, Some(_)) => {
                return Err(VaultcError::MalformedInput {
                    path: "evidence".into(),
                    reason: "file-level evidence cannot carry a byte span".into(),
                });
            }
        };
        let mut hasher = Sha256::new();
        hasher.update(b"vaultc:evidence:v1\0");
        hasher.update(snapshot_id.hash().as_bytes());
        hasher.update(document_id.hash().as_bytes());
        hasher.update([mode]);
        if let Some(block_id) = block_id {
            hasher.update(block_id.hash().as_bytes());
        }
        if let Some((start, end)) = span {
            hasher.update(start.to_be_bytes());
            hasher.update(end.to_be_bytes());
        }
        hasher.update(content_hash.as_bytes());
        Ok(Self(ContentHash(hasher.finalize().into())))
    }
}

fn write_length_prefixed(hasher: &mut Sha256, bytes: &[u8]) {
    write_uleb128(hasher, bytes.len() as u64);
    hasher.update(bytes);
}

fn crockford_base32(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";
    let mut encoded = String::with_capacity(bytes.len().div_ceil(5) * 8);
    let mut buffer = 0_u16;
    let mut bits = 0_u8;
    for &byte in bytes {
        buffer = (buffer << 8) | u16::from(byte);
        bits += 8;
        while bits >= 5 {
            bits -= 5;
            let index = ((buffer >> bits) & 0x1f) as usize;
            encoded.push(ALPHABET[index] as char);
            buffer &= (1_u16 << bits).saturating_sub(1);
        }
    }
    if bits > 0 {
        let index = ((buffer << (5 - bits)) & 0x1f) as usize;
        encoded.push(ALPHABET[index] as char);
    }
    encoded
}

fn write_uleb128(hasher: &mut Sha256, mut value: u64) {
    loop {
        let mut byte = (value & 0x7f) as u8;
        value >>= 7;
        if value != 0 {
            byte |= 0x80;
        }
        hasher.update([byte]);
        if value == 0 {
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn standard_sha256_vectors_are_preserved_for_raw_hash_check() {
        let empty = Sha256::digest([]);
        assert_eq!(
            hex::encode(empty),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        let abc = Sha256::digest(b"abc");
        assert_eq!(
            hex::encode(abc),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn typed_ids_round_trip() {
        let id = SnapshotId::from_parts("test\0", &[b"alpha"]);
        let decoded: SnapshotId = id.to_string().parse().expect("parse snapshot ID");
        assert_eq!(decoded, id);
    }

    #[test]
    fn typed_ids_reject_noncanonical_hex() {
        let id = SnapshotId::from_parts("test\0", &[b"alpha"]);
        assert!(
            id.to_string()
                .to_ascii_uppercase()
                .parse::<SnapshotId>()
                .is_err()
        );
        assert!("snap_00".parse::<SnapshotId>().is_err());
    }

    #[test]
    fn suffix_uses_crockford_base32() {
        let id = DocumentId::from_hash(ContentHash([0xff; 32]));
        assert_eq!(id.suffix_base32(8), "ZZZZZZZZ");
        assert!(!id.suffix_base32(52).contains(['I', 'L', 'O', 'U']));
    }

    #[test]
    fn snapshot_manifest_order_is_caller_controlled() {
        let a = SourceFileId::from_parts("test:file\0", &[b"a"]);
        let b = SourceFileId::from_parts("test:file\0", &[b"b"]);
        let first = SnapshotId::from_manifest("source", "policy", [("a.md", a), ("b.md", b)]);
        let second = SnapshotId::from_manifest("source", "policy", [("b.md", b), ("a.md", a)]);
        assert_ne!(first, second);
    }
}
