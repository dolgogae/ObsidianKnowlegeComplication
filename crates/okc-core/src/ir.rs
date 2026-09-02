use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::diagnostic::SourceSpan;
use crate::identity::{
    AssetId, BaseArtifactId, BlockId, CanvasId, ContentHash, DocumentId, LinkId, SectionId,
    SnapshotId, SourceFileId,
};
use crate::source::SourceId;

pub const IR_SCHEMA_VERSION: u32 = 2;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FileKind {
    Markdown,
    Canvas,
    Base,
    Asset,
}

impl FileKind {
    pub fn classify(path: &str) -> Self {
        let extension = std::path::Path::new(path)
            .extension()
            .and_then(|value| value.to_str());
        if extension.is_some_and(|value| value.eq_ignore_ascii_case("md")) {
            Self::Markdown
        } else if extension.is_some_and(|value| value.eq_ignore_ascii_case("canvas")) {
            Self::Canvas
        } else if extension.is_some_and(|value| value.eq_ignore_ascii_case("base")) {
            Self::Base
        } else {
            Self::Asset
        }
    }

    pub fn media_family(&self) -> &'static str {
        match self {
            Self::Markdown => "text/markdown",
            Self::Canvas => "application/vnd.obsidian.canvas+json",
            Self::Base => "application/vnd.obsidian.base",
            Self::Asset => "application/octet-stream",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourcePathEncoding {
    Utf8,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceFile {
    pub source_id: SourceId,
    pub snapshot_id: SnapshotId,
    pub file_id: SourceFileId,
    pub original_path: String,
    pub logical_path: String,
    pub path_encoding: SourcePathEncoding,
    pub kind: FileKind,
    pub byte_len: u64,
    pub raw_sha256: String,
    pub content_hash: ContentHash,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Document {
    pub schema_version: u32,
    pub document_id: DocumentId,
    pub source_file: SourceFile,
    pub title: Option<String>,
    pub aliases: Vec<String>,
    pub tags: Vec<String>,
    pub frontmatter: Option<Value>,
    pub frontmatter_hash: ContentHash,
    pub body_hash: ContentHash,
    pub comparison_text: String,
    pub sections: Vec<Section>,
    pub blocks: Vec<Block>,
    pub links: Vec<Link>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Section {
    pub section_id: SectionId,
    pub heading_level: u8,
    pub heading: String,
    pub heading_path: Vec<String>,
    pub blocks: Vec<BlockId>,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Block {
    pub block_id: BlockId,
    pub kind: BlockKind,
    pub span: SourceSpan,
    pub content_hash: ContentHash,
    pub comparison_text: String,
    #[serde(default)]
    pub explicit_id: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BlockKind {
    Heading,
    Paragraph,
    Code,
    Quote,
    List,
    Callout,
    Math,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Link {
    pub link_id: LinkId,
    pub syntax: LinkSyntax,
    pub span: SourceSpan,
    pub raw_target: String,
    pub path: Option<String>,
    pub heading: Option<String>,
    pub block_id: Option<String>,
    pub display: Option<String>,
    pub embed: bool,
    pub resolution: LinkResolution,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LinkSyntax {
    Wiki,
    Markdown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum LinkResolution {
    Pending,
    Resolved { document_id: DocumentId },
    Unresolved,
    Ambiguous { candidates: Vec<DocumentId> },
    Asset { asset_id: AssetId },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Canvas {
    pub canvas_id: CanvasId,
    pub source_file: SourceFile,
    pub value: Value,
    pub nodes: Vec<CanvasNode>,
    pub edges: Vec<CanvasEdge>,
    pub file_references: Vec<CanvasFileReference>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CanvasNode {
    pub id: String,
    pub node_type: String,
    pub file: Option<String>,
    pub x: Option<i64>,
    pub y: Option<i64>,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub unknown_fields: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CanvasEdge {
    pub id: String,
    pub from_node: String,
    pub to_node: String,
    pub from_side: Option<String>,
    pub to_side: Option<String>,
    pub label: Option<String>,
    pub unknown_fields: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CanvasFileReference {
    pub node_id: String,
    pub raw_path: String,
    pub resolution: CanvasReferenceResolution,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "kind", content = "id", rename_all = "snake_case")]
pub enum CanvasReferenceTarget {
    Document(DocumentId),
    Asset(AssetId),
    Canvas(CanvasId),
    Base(BaseArtifactId),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum CanvasReferenceResolution {
    Pending,
    Resolved {
        target: CanvasReferenceTarget,
    },
    Unresolved,
    Ambiguous {
        candidates: Vec<CanvasReferenceTarget>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Asset {
    pub asset_id: AssetId,
    pub media_type: String,
    pub byte_len: u64,
    /// Every immutable source occurrence of these exact attachment bytes.
    /// The vector is sorted by `(source_id, logical_path, file_id)`.
    pub sources: Vec<SourceFile>,
    /// Basename of the first canonical source occurrence.
    pub basename: String,
}

impl Asset {
    pub fn from_source(asset_id: AssetId, source_file: SourceFile) -> Self {
        let basename = source_basename(&source_file);
        let byte_len = source_file.byte_len;
        Self {
            asset_id,
            media_type: asset_media_type(&source_file.logical_path).into(),
            byte_len,
            sources: vec![source_file],
            basename,
        }
    }

    pub fn merge_sources(&mut self, sources: impl IntoIterator<Item = SourceFile>) {
        self.sources.extend(sources);
        self.sources.sort_by(|left, right| {
            left.source_id
                .cmp(&right.source_id)
                .then_with(|| {
                    left.logical_path
                        .as_bytes()
                        .cmp(right.logical_path.as_bytes())
                })
                .then_with(|| left.file_id.cmp(&right.file_id))
        });
        self.sources.dedup_by(|left, right| {
            left.snapshot_id == right.snapshot_id && left.file_id == right.file_id
        });
        if let Some(source) = self.sources.first() {
            self.basename = source_basename(source);
        }
    }

    pub fn canonical_source(&self) -> Option<&SourceFile> {
        self.sources.first()
    }
}

fn asset_media_type(path: &str) -> &'static str {
    let extension = std::path::Path::new(path)
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default();
    if extension.eq_ignore_ascii_case("png") {
        "image/png"
    } else if matches!(extension.to_ascii_lowercase().as_str(), "jpg" | "jpeg") {
        "image/jpeg"
    } else if extension.eq_ignore_ascii_case("gif") {
        "image/gif"
    } else if extension.eq_ignore_ascii_case("svg") {
        "image/svg+xml"
    } else if extension.eq_ignore_ascii_case("pdf") {
        "application/pdf"
    } else if extension.eq_ignore_ascii_case("mp3") {
        "audio/mpeg"
    } else if extension.eq_ignore_ascii_case("mp4") {
        "video/mp4"
    } else {
        "application/octet-stream"
    }
}

fn source_basename(source_file: &SourceFile) -> String {
    std::path::Path::new(&source_file.logical_path)
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("asset")
        .to_owned()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BaseArtifact {
    pub base_artifact_id: BaseArtifactId,
    pub source_file: SourceFile,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvidenceRef {
    pub snapshot_id: SnapshotId,
    pub document_id: DocumentId,
    pub block_id: Option<BlockId>,
    pub span: Option<SourceSpan>,
    pub content_hash: ContentHash,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CanonicalWorkspace {
    pub schema_version: u32,
    pub documents: BTreeMap<DocumentId, Document>,
    pub canvases: BTreeMap<CanvasId, Canvas>,
    pub assets: BTreeMap<AssetId, Asset>,
    pub bases: Vec<BaseArtifact>,
}

impl Default for CanonicalWorkspace {
    fn default() -> Self {
        Self {
            schema_version: IR_SCHEMA_VERSION,
            documents: BTreeMap::new(),
            canvases: BTreeMap::new(),
            assets: BTreeMap::new(),
            bases: Vec::new(),
        }
    }
}
