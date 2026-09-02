use serde::{Deserialize, Serialize};

use crate::identity::{DocumentId, SourceFileId};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Info,
    Warning,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DiagnosticCode {
    ExcludedPath,
    UnsafePath,
    ResourceLimit,
    InvalidUtf8,
    MalformedFrontmatter,
    MalformedMarkdown,
    MalformedCanvas,
    OpaqueBaseUnvalidated,
    ExactDuplicate,
    NearDuplicateCandidate,
    PathExact,
    PathCasefold,
    UnicodeNormalization,
    PathSanitized,
    TitleAmbiguity,
    AliasAmbiguity,
    FrontmatterValue,
    LinkUnresolved,
    LinkAmbiguity,
    ProposalRejected,
    ApprovalRequired,
    SourceChanged,
    VerificationFailed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceSpan {
    pub byte_start: u64,
    pub byte_end: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Diagnostic {
    pub code: DiagnosticCode,
    pub severity: Severity,
    pub message: String,
    pub logical_path: Option<String>,
    pub source_file_id: Option<SourceFileId>,
    pub document_id: Option<DocumentId>,
    pub span: Option<SourceSpan>,
    #[serde(default)]
    pub remediation: Option<String>,
}

impl Diagnostic {
    pub fn warning(code: DiagnosticCode, message: impl Into<String>) -> Self {
        Self {
            code,
            severity: Severity::Warning,
            message: message.into(),
            logical_path: None,
            source_file_id: None,
            document_id: None,
            span: None,
            remediation: None,
        }
    }

    #[must_use]
    pub fn for_path(mut self, path: impl Into<String>) -> Self {
        self.logical_path = Some(path.into());
        self
    }
}
