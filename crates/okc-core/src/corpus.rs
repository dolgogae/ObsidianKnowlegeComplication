use std::collections::BTreeMap;
use std::path::PathBuf;

use serde_json::Value;

use crate::canonical::to_canonical_json;
use crate::config::CompilerPolicy;
use crate::error::{OkcError, Result};
use crate::identity::{BlockId, ContentHash, DocumentId};
use crate::integration::{IntegrationCorpus, IntegrationDocument, MetadataValue, SourceBlock};
use crate::source::SourceSpec;

pub type BlockTextMap = BTreeMap<(DocumentId, BlockId), String>;

/// The sealed current-schema corpus and deterministic source text projection.
#[derive(Debug, Clone)]
pub struct PreparedCorpus {
    pub corpus: IntegrationCorpus,
    pub block_texts: BlockTextMap,
    pub source_count: usize,
}

/// Builds the current integration corpus through the stable snapshot/parser
/// safety pipeline. Intermediate inspection and planning records stay private.
#[derive(Debug, Clone, Default)]
pub struct CorpusBuilder {
    workspace: Option<PathBuf>,
}

impl CorpusBuilder {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn workspace(mut self, path: impl Into<PathBuf>) -> Self {
        self.workspace = Some(path.into());
        self
    }

    pub fn build(&self, sources: impl IntoIterator<Item = SourceSpec>) -> Result<PreparedCorpus> {
        let policy = CompilerPolicy::default();
        policy.validate()?;
        let inspection =
            crate::snapshot::inspect_sources(sources, &policy, self.workspace.as_deref())?;
        let source_count = inspection.snapshots.len();
        let plan = crate::plan::build_plan(&inspection, &policy)?;
        let (corpus, block_texts) = prepare_from_plan(&plan)?;
        Ok(PreparedCorpus {
            corpus,
            block_texts,
            source_count,
        })
    }
}

fn prepare_from_plan(plan: &crate::plan::DraftPlan) -> Result<(IntegrationCorpus, BlockTextMap)> {
    let mut block_texts = BTreeMap::new();
    let mut documents = Vec::new();
    for document in plan.workspace.documents.values() {
        let document_id = DocumentId::from_parts(
            "okc:document:v3\0",
            &[document.document_id.hash().as_bytes()],
        );
        let mut blocks = Vec::new();
        for block in &document.blocks {
            let block_id = BlockId::from_parts(
                "okc:block:v3\0",
                &[
                    document_id.hash().as_bytes(),
                    block.block_id.hash().as_bytes(),
                ],
            );
            let content_hash = ContentHash::from_domain_bytes(
                "okc:block-content:v3\0",
                block.comparison_text.as_bytes(),
            );
            blocks.push(SourceBlock {
                block_id,
                content_hash,
                text: block.comparison_text.clone(),
            });
            block_texts.insert((document_id, block_id), block.comparison_text.clone());
        }
        let mut metadata = Vec::new();
        if let Some(Value::Object(frontmatter)) = &document.frontmatter {
            for (key, value) in frontmatter {
                if let Value::Array(values) = value {
                    for (index, value) in values.iter().enumerate() {
                        metadata.push(MetadataValue::new(
                            document_id,
                            key,
                            u32::try_from(index).map_err(|_| {
                                OkcError::InvalidConfig(
                                    "frontmatter array exceeds u32::MAX values".into(),
                                )
                            })?,
                            value,
                        )?);
                    }
                } else {
                    metadata.push(MetadataValue::new(document_id, key, 0, value)?);
                }
            }
        }
        documents.push(IntegrationDocument {
            source_id: document.source_file.source_id.clone(),
            document_id,
            original_path: document.source_file.original_path.clone(),
            document_hash: ContentHash::from_domain_bytes(
                "okc:document-body:v3\0",
                document.comparison_text.as_bytes(),
            ),
            blocks,
            metadata,
        });
    }
    let policy_hash =
        ContentHash::from_domain_bytes("okc:policy:v3\0", &to_canonical_json(&plan.policy)?);
    Ok((
        IntegrationCorpus::seal(policy_hash, documents)?,
        block_texts,
    ))
}
