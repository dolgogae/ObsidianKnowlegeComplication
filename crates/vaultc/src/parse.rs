use std::collections::BTreeSet;
use std::fmt::Formatter;
use std::path::Path;

use comrak::{Options, markdown_to_html};
use serde::de::{Deserialize, Deserializer, MapAccess, SeqAccess, Visitor};
use serde_json::Value;
use unicode_normalization::UnicodeNormalization;

use crate::canonical::to_canonical_json;
use crate::config::CompilerPolicy;
use crate::diagnostic::{Diagnostic, DiagnosticCode, SourceSpan};
use crate::error::{Result, VaultcError};
use crate::identity::{
    AssetId, BaseArtifactId, BlockId, CanvasId, ContentHash, DocumentId, LinkId,
};
use crate::ir::{
    Asset, BaseArtifact, Block, BlockKind, CanonicalWorkspace, Canvas, CanvasFileReference,
    CanvasReferenceResolution, Document, FileKind, IR_SCHEMA_VERSION, Link, LinkResolution,
    LinkSyntax, Section, SourceFile,
};

type ParsedFrontmatter = (
    Option<Value>,
    Vec<String>,
    Vec<String>,
    Option<String>,
    ContentHash,
);

const _: () = {
    assert!(caseless::UNICODE_VERSION.0 == 16);
    assert!(caseless::UNICODE_VERSION.1 == 0);
    assert!(caseless::UNICODE_VERSION.2 == 0);
    assert!(unicode_normalization::UNICODE_VERSION.0 == 17);
    assert!(unicode_normalization::UNICODE_VERSION.1 == 0);
    assert!(unicode_normalization::UNICODE_VERSION.2 == 0);
};

pub(crate) fn parse_file(
    source_file: SourceFile,
    bytes: &[u8],
    policy: &CompilerPolicy,
    workspace: &mut CanonicalWorkspace,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<()> {
    crate::snapshot::validate_source_path_pair(
        &source_file.original_path,
        &source_file.logical_path,
        source_file.path_encoding,
        policy,
    )?;
    match source_file.kind {
        FileKind::Markdown => {
            let document = parse_markdown(source_file, bytes, policy, diagnostics)?;
            workspace.documents.insert(document.document_id, document);
        }
        FileKind::Canvas => {
            let canvas = parse_canvas(source_file, bytes, diagnostics)?;
            workspace.canvases.insert(canvas.canvas_id, canvas);
        }
        FileKind::Base => {
            diagnostics.push(
                Diagnostic::warning(
                    DiagnosticCode::OpaqueBaseUnvalidated,
                    "Obsidian Base is preserved opaquely; internal references are not validated",
                )
                .for_path(source_file.logical_path.clone()),
            );
            let base_artifact_id = BaseArtifactId::from_parts(
                "vaultc:base:v1\0",
                &[
                    source_file.snapshot_id.hash().as_bytes(),
                    source_file.file_id.hash().as_bytes(),
                ],
            );
            workspace.bases.push(BaseArtifact {
                base_artifact_id,
                source_file,
            });
        }
        FileKind::Asset => {
            let asset_id =
                AssetId::from_parts("vaultc:asset:v1\0", &[source_file.content_hash.as_bytes()]);
            match workspace.assets.entry(asset_id) {
                std::collections::btree_map::Entry::Vacant(entry) => {
                    entry.insert(Asset::from_source(asset_id, source_file));
                }
                std::collections::btree_map::Entry::Occupied(mut entry) => {
                    entry.get_mut().merge_sources([source_file]);
                }
            }
        }
    }
    Ok(())
}

pub(crate) fn parse_markdown(
    source_file: SourceFile,
    bytes: &[u8],
    policy: &CompilerPolicy,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<Document> {
    crate::snapshot::validate_source_path_pair(
        &source_file.original_path,
        &source_file.logical_path,
        source_file.path_encoding,
        policy,
    )?;
    let text = std::str::from_utf8(bytes).map_err(|error| VaultcError::MalformedInput {
        path: source_file.logical_path.clone(),
        reason: format!("Markdown must be UTF-8: {error}"),
    })?;
    let (frontmatter_raw, body, body_offset) = split_frontmatter(text);
    let (frontmatter, aliases, tags, explicit_title, frontmatter_hash) =
        parse_frontmatter(frontmatter_raw, &source_file.logical_path, diagnostics)?;

    let (body, body_offset) = if body_offset == 0 {
        body.strip_prefix('\u{feff}')
            .map_or((body, body_offset), |without_bom| {
                (without_bom, body_offset + '\u{feff}'.len_utf8())
            })
    } else {
        (body, body_offset)
    };
    let normalized_body = normalize_text(body);
    let options = Options::default();
    let mut structural_projection = markdown_to_html(&normalized_body, &options);
    // Comrak treats a leading U+FEFF as a transport BOM. At this point the
    // only transport BOM has already been removed at byte offset zero, so a
    // remaining leading U+FEFF is semantic body text and must stay in N_body.
    if normalized_body.starts_with('\u{feff}') {
        structural_projection.insert(0, '\u{feff}');
    }
    let body_hash =
        ContentHash::from_domain_bytes("vaultc:body:v1\0", structural_projection.as_bytes());
    let document_id = DocumentId::from_parts(
        "vaultc:document:v1\0",
        &[
            source_file.snapshot_id.hash().as_bytes(),
            source_file.file_id.hash().as_bytes(),
        ],
    );
    let (sections, blocks) = scan_blocks(document_id, body, body_offset);
    let links = scan_links(document_id, body, body_offset);
    let heading_title = sections.first().map(|section| section.heading.clone());
    let filename_title = Path::new(&source_file.logical_path)
        .file_stem()
        .and_then(|value| value.to_str())
        .map(str::to_owned);
    let title = explicit_title.or(heading_title).or(filename_title);

    Ok(Document {
        schema_version: IR_SCHEMA_VERSION,
        document_id,
        source_file,
        title,
        aliases,
        tags,
        frontmatter,
        frontmatter_hash,
        body_hash,
        comparison_text: normalized_body,
        sections,
        blocks,
        links,
    })
}

fn split_frontmatter(text: &str) -> (Option<&str>, &str, usize) {
    let (prefix_len, candidate) = text
        .strip_prefix('\u{feff}')
        .map_or((0, text), |candidate| ('\u{feff}'.len_utf8(), candidate));
    let Some(first_line) = raw_lines(candidate).next() else {
        return (None, text, 0);
    };
    let first_line_content = trim_raw_line_ending(first_line);
    if first_line_content != "---" || first_line_content.len() == first_line.len() {
        return (None, text, 0);
    }
    let first_line_end = first_line.len();
    let mut offset = prefix_len + first_line_end;
    for line in raw_lines(&text[offset..]) {
        let trimmed = trim_raw_line_ending(line);
        if trimmed == "---" || trimmed == "..." {
            let body_offset = offset + line.len();
            return (
                Some(&text[prefix_len + first_line_end..offset]),
                &text[body_offset..],
                body_offset,
            );
        }
        offset += line.len();
    }
    (None, text, 0)
}

fn parse_frontmatter(
    raw: Option<&str>,
    path: &str,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<ParsedFrontmatter> {
    let Some(raw) = raw else {
        return Ok((
            None,
            Vec::new(),
            Vec::new(),
            None,
            ContentHash::from_domain_bytes("vaultc:frontmatter:v1\0", b"null"),
        ));
    };
    match serde_yaml_ng::from_str::<serde_yaml_ng::Value>(raw) {
        Ok(yaml) => {
            let json = match serde_json::to_value(yaml) {
                Ok(Value::Object(map)) => Value::Object(map),
                Ok(_) => {
                    return Ok(opaque_frontmatter(
                        raw,
                        path,
                        "frontmatter root must be a YAML mapping",
                        diagnostics,
                    ));
                }
                Err(error) => {
                    return Ok(opaque_frontmatter(
                        raw,
                        path,
                        &format!("frontmatter cannot be represented canonically: {error}"),
                        diagnostics,
                    ));
                }
            };
            let canonical = to_canonical_json(&json)?;
            let aliases = string_list_field(&json, "aliases");
            let tags = string_list_field(&json, "tags");
            let title = json.get("title").and_then(Value::as_str).map(str::to_owned);
            Ok((
                Some(json),
                aliases,
                tags,
                title,
                ContentHash::from_domain_bytes("vaultc:frontmatter:v1\0", &canonical),
            ))
        }
        Err(error) if error.to_string().contains("duplicate entry") => {
            Err(VaultcError::MalformedInput {
                path: path.to_owned(),
                reason: format!("duplicate YAML frontmatter key: {error}"),
            })
        }
        Err(error) => Ok(opaque_frontmatter(
            raw,
            path,
            &error.to_string(),
            diagnostics,
        )),
    }
}

fn opaque_frontmatter(
    raw: &str,
    path: &str,
    reason: &str,
    diagnostics: &mut Vec<Diagnostic>,
) -> ParsedFrontmatter {
    diagnostics.push(
        Diagnostic::warning(
            DiagnosticCode::MalformedFrontmatter,
            format!("frontmatter kept opaque: {reason}"),
        )
        .for_path(path),
    );
    (
        None,
        Vec::new(),
        Vec::new(),
        None,
        ContentHash::from_domain_bytes("vaultc:frontmatter-opaque:v1\0", raw.as_bytes()),
    )
}

fn string_list_field(value: &Value, key: &str) -> Vec<String> {
    let mut values = match value.get(key) {
        Some(Value::String(value)) => vec![value.clone()],
        Some(Value::Array(items)) => items
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_owned)
            .collect(),
        _ => Vec::new(),
    };
    values.sort_by(|left, right| left.as_bytes().cmp(right.as_bytes()));
    values.dedup();
    values
}

fn normalize_text(value: &str) -> String {
    value
        .replace("\r\n", "\n")
        .replace('\r', "\n")
        .nfc()
        .collect()
}

struct RawLines<'a> {
    text: &'a str,
    offset: usize,
}

impl<'a> Iterator for RawLines<'a> {
    type Item = &'a str;

    fn next(&mut self) -> Option<Self::Item> {
        if self.offset >= self.text.len() {
            return None;
        }
        let start = self.offset;
        let bytes = self.text.as_bytes();
        while self.offset < bytes.len() && !matches!(bytes[self.offset], b'\r' | b'\n') {
            self.offset += 1;
        }
        if self.offset < bytes.len() {
            if bytes[self.offset] == b'\r' && bytes.get(self.offset + 1) == Some(&b'\n') {
                self.offset += 2;
            } else {
                self.offset += 1;
            }
        }
        Some(&self.text[start..self.offset])
    }
}

fn raw_lines(text: &str) -> RawLines<'_> {
    RawLines { text, offset: 0 }
}

fn trim_raw_line_ending(line: &str) -> &str {
    line.strip_suffix("\r\n")
        .or_else(|| line.strip_suffix(['\r', '\n']))
        .unwrap_or(line)
}

fn scan_blocks(
    document_id: DocumentId,
    body: &str,
    base_offset: usize,
) -> (Vec<Section>, Vec<Block>) {
    let mut sections = Vec::new();
    let mut blocks = Vec::new();
    let mut offset = 0_usize;
    let mut paragraph_start: Option<usize> = None;
    let mut fence: Option<(u8, usize, usize)> = None;

    let flush_paragraph = |end: usize, blocks: &mut Vec<Block>, start: &mut Option<usize>| {
        if let Some(begin) = start.take()
            && end > begin
        {
            push_block(
                document_id,
                BlockKind::Paragraph,
                body,
                base_offset,
                begin,
                end,
                blocks,
            );
        }
    };

    for line in raw_lines(body) {
        let line_start = offset;
        let line_end = offset + line.len();
        let trimmed = line.trim_end_matches(['\r', '\n']);
        if let Some((marker, minimum, fence_start)) = fence {
            if is_closing_fence(trimmed, marker, minimum) {
                push_block(
                    document_id,
                    BlockKind::Code,
                    body,
                    base_offset,
                    fence_start,
                    line_end,
                    &mut blocks,
                );
                fence = None;
            }
        } else if let Some((marker, minimum)) = opening_fence(trimmed) {
            flush_paragraph(line_start, &mut blocks, &mut paragraph_start);
            fence = Some((marker, minimum, line_start));
        } else {
            let hash_count = trimmed.bytes().take_while(|byte| *byte == b'#').count();
            if (1..=6).contains(&hash_count)
                && trimmed
                    .as_bytes()
                    .get(hash_count)
                    .is_some_and(u8::is_ascii_whitespace)
            {
                flush_paragraph(line_start, &mut blocks, &mut paragraph_start);
                let heading = trimmed[hash_count..].trim().to_owned();
                sections.push(Section {
                    heading_level: u8::try_from(hash_count).unwrap_or(6),
                    heading,
                    span: SourceSpan {
                        byte_start: (base_offset + line_start) as u64,
                        byte_end: (base_offset + line_end) as u64,
                    },
                });
                push_block(
                    document_id,
                    BlockKind::Heading,
                    body,
                    base_offset,
                    line_start,
                    line_end,
                    &mut blocks,
                );
            } else if trimmed.trim().is_empty() {
                flush_paragraph(line_start, &mut blocks, &mut paragraph_start);
            } else if paragraph_start.is_none() {
                paragraph_start = Some(line_start);
            }
        }
        offset = line_end;
    }
    if let Some((_, _, fence_start)) = fence {
        push_block(
            document_id,
            BlockKind::Code,
            body,
            base_offset,
            fence_start,
            body.len(),
            &mut blocks,
        );
    }
    flush_paragraph(body.len(), &mut blocks, &mut paragraph_start);
    (sections, blocks)
}

fn push_block(
    document_id: DocumentId,
    kind: BlockKind,
    body: &str,
    base_offset: usize,
    start: usize,
    end: usize,
    blocks: &mut Vec<Block>,
) {
    let slice = &body[start..end];
    let content_hash =
        ContentHash::from_domain_bytes("vaultc:block-content:v1\0", slice.as_bytes());
    let index = blocks.len() as u64;
    let block_id = BlockId::from_parts(
        "vaultc:block:v1\0",
        &[
            document_id.hash().as_bytes(),
            &index.to_be_bytes(),
            content_hash.as_bytes(),
        ],
    );
    blocks.push(Block {
        block_id,
        kind,
        span: SourceSpan {
            byte_start: (base_offset + start) as u64,
            byte_end: (base_offset + end) as u64,
        },
        content_hash,
        comparison_text: normalize_text(slice),
        explicit_id: explicit_block_id(slice),
    });
}

fn explicit_block_id(block: &str) -> Option<String> {
    let normalized = normalize_text(block);
    let candidate = normalized
        .lines()
        .rev()
        .find(|line| !line.trim().is_empty())?
        .split_whitespace()
        .last()?
        .strip_prefix('^')?;
    (!candidate.is_empty()
        && candidate
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_')))
    .then(|| candidate.to_owned())
}

fn scan_links(document_id: DocumentId, text: &str, base_offset: usize) -> Vec<Link> {
    let mut links = Vec::new();
    let mut line_offset = base_offset;
    let mut fence: Option<(u8, usize)> = None;
    for line_with_ending in raw_lines(text) {
        let line = trim_raw_line_ending(line_with_ending);
        if let Some((marker, minimum)) = fence {
            if is_closing_fence(line, marker, minimum) {
                fence = None;
            }
            line_offset += line_with_ending.len();
            continue;
        }
        if let Some(opening) = opening_fence(line) {
            fence = Some(opening);
            line_offset += line_with_ending.len();
            continue;
        }
        if !line.starts_with("    ") && !line.starts_with('\t') {
            scan_inline_links(document_id, line, line_offset, &mut links);
        }
        line_offset += line_with_ending.len();
    }
    links
}

fn opening_fence(line: &str) -> Option<(u8, usize)> {
    let bytes = line.as_bytes();
    let indent = bytes.iter().take_while(|byte| **byte == b' ').count();
    if indent > 3 {
        return None;
    }
    let marker = *bytes.get(indent)?;
    if !matches!(marker, b'`' | b'~') {
        return None;
    }
    let length = bytes[indent..]
        .iter()
        .take_while(|byte| **byte == marker)
        .count();
    (length >= 3).then_some((marker, length))
}

fn is_closing_fence(line: &str, marker: u8, minimum: usize) -> bool {
    let bytes = line.as_bytes();
    let indent = bytes.iter().take_while(|byte| **byte == b' ').count();
    if indent > 3 || bytes.get(indent) != Some(&marker) {
        return false;
    }
    let length = bytes[indent..]
        .iter()
        .take_while(|byte| **byte == marker)
        .count();
    length >= minimum && bytes[indent + length..].iter().all(u8::is_ascii_whitespace)
}

fn scan_inline_links(
    document_id: DocumentId,
    line: &str,
    line_offset: usize,
    links: &mut Vec<Link>,
) {
    let bytes = line.as_bytes();
    let mut index = 0_usize;
    while index < bytes.len() {
        if bytes[index] == b'\\' {
            index = (index + 2).min(bytes.len());
            continue;
        }
        if bytes[index] == b'`' {
            let run = bytes[index..]
                .iter()
                .take_while(|byte| **byte == b'`')
                .count();
            if let Some(end) = find_backtick_close(bytes, index + run, run) {
                index = end + run;
            } else {
                index += run;
            }
            continue;
        }

        let (wiki_embed, wiki_open) = if bytes[index..].starts_with(b"![[") {
            (true, Some(index + 3))
        } else if bytes[index..].starts_with(b"[[") {
            (false, Some(index + 2))
        } else {
            (false, None)
        };
        if let Some(open) = wiki_open
            && let Some(relative_end) = line[open..].find("]]")
        {
            let end = open + relative_end;
            let raw = &line[open..end];
            links.push(parse_wiki_link(
                document_id,
                links.len(),
                raw,
                line_offset + open,
                line_offset + end,
                wiki_embed,
            ));
            index = end + 2;
            continue;
        }

        let (markdown_embed, label_open) = if bytes[index..].starts_with(b"![") {
            (true, Some(index + 2))
        } else if bytes[index] == b'[' && !bytes[index..].starts_with(b"[[") {
            (false, Some(index + 1))
        } else {
            (false, None)
        };
        if let Some(label_open) = label_open
            && let Some((next, link)) = parse_inline_markdown_link(
                document_id,
                links.len(),
                line,
                line_offset,
                label_open,
                markdown_embed,
            )
        {
            links.push(link);
            index = next;
            continue;
        }
        index += 1;
    }
}

fn find_backtick_close(bytes: &[u8], mut index: usize, expected: usize) -> Option<usize> {
    while index < bytes.len() {
        if bytes[index] != b'`' {
            index += 1;
            continue;
        }
        let run = bytes[index..]
            .iter()
            .take_while(|byte| **byte == b'`')
            .count();
        if run == expected {
            return Some(index);
        }
        index += run;
    }
    None
}

fn parse_inline_markdown_link(
    document_id: DocumentId,
    ordinal: usize,
    line: &str,
    line_offset: usize,
    label_start: usize,
    embed: bool,
) -> Option<(usize, Link)> {
    let bytes = line.as_bytes();
    let label_end = find_balanced_delimiter(bytes, label_start, b'[', b']')?;
    let open_parenthesis = label_end + 1;
    if bytes.get(open_parenthesis) != Some(&b'(') {
        return None;
    }
    let mut cursor = open_parenthesis + 1;
    while bytes.get(cursor).is_some_and(u8::is_ascii_whitespace) {
        cursor += 1;
    }
    let angle_wrapped = bytes.get(cursor) == Some(&b'<');
    if angle_wrapped {
        cursor += 1;
    }
    let target_start = cursor;
    let target_end = if angle_wrapped {
        while cursor < bytes.len() && bytes[cursor] != b'>' {
            if bytes[cursor] == b'\\' {
                cursor = (cursor + 2).min(bytes.len());
            } else {
                cursor += 1;
            }
        }
        (bytes.get(cursor) == Some(&b'>')).then_some(cursor)?
    } else {
        let mut nested = 0_usize;
        while cursor < bytes.len() {
            match bytes[cursor] {
                b'\\' => cursor = (cursor + 2).min(bytes.len()),
                b'(' => {
                    nested += 1;
                    cursor += 1;
                }
                b')' if nested > 0 => {
                    nested -= 1;
                    cursor += 1;
                }
                b')' | b' ' | b'\t' => break,
                _ => cursor += 1,
            }
        }
        cursor
    };
    if target_end == target_start {
        return None;
    }
    cursor = target_end + usize::from(angle_wrapped);
    let close_parenthesis = find_markdown_close(bytes, cursor)?;
    let raw = &line[target_start..target_end];
    let display = line[label_start..label_end].to_owned();
    let (path, heading, block_id) = split_markdown_target(raw);
    let ordinal = ordinal as u64;
    let link_id = LinkId::from_parts(
        "vaultc:link:v1\0",
        &[
            document_id.hash().as_bytes(),
            &ordinal.to_be_bytes(),
            raw.as_bytes(),
        ],
    );
    Some((
        close_parenthesis + 1,
        Link {
            link_id,
            syntax: LinkSyntax::Markdown,
            span: SourceSpan {
                byte_start: (line_offset + target_start) as u64,
                byte_end: (line_offset + target_end) as u64,
            },
            raw_target: raw.to_owned(),
            path,
            heading,
            block_id,
            display: Some(display),
            embed,
            resolution: LinkResolution::Pending,
        },
    ))
}

fn find_balanced_delimiter(bytes: &[u8], mut index: usize, open: u8, close: u8) -> Option<usize> {
    let mut nested = 0_usize;
    while index < bytes.len() {
        match bytes[index] {
            b'\\' => index = (index + 2).min(bytes.len()),
            value if value == open => {
                nested += 1;
                index += 1;
            }
            value if value == close && nested > 0 => {
                nested -= 1;
                index += 1;
            }
            value if value == close => return Some(index),
            _ => index += 1,
        }
    }
    None
}

fn find_markdown_close(bytes: &[u8], mut index: usize) -> Option<usize> {
    let mut quote = None;
    while index < bytes.len() {
        match bytes[index] {
            b'\\' => index = (index + 2).min(bytes.len()),
            value @ (b'\'' | b'"') => {
                quote = if quote == Some(value) {
                    None
                } else if quote.is_none() {
                    Some(value)
                } else {
                    quote
                };
                index += 1;
            }
            b')' if quote.is_none() => return Some(index),
            _ => index += 1,
        }
    }
    None
}

fn split_markdown_target(raw: &str) -> (Option<String>, Option<String>, Option<String>) {
    if is_external_target(raw) {
        return (None, None, None);
    }
    let (path, heading) = raw.split_once('#').map_or((raw, None), |(path, heading)| {
        (path, Some(heading.to_owned()))
    });
    let (heading, block_id) = match heading {
        Some(fragment) if fragment.starts_with('^') => {
            (None, Some(fragment.trim_start_matches('^').to_owned()))
        }
        other => (other, None),
    };
    let path =
        (!path.is_empty()).then(|| percent_decode_path(path).unwrap_or_else(|| path.to_owned()));
    (path, heading, block_id)
}

fn percent_decode_path(value: &str) -> Option<String> {
    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0_usize;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            let high = hex_value(*bytes.get(index + 1)?)?;
            let low = hex_value(*bytes.get(index + 2)?)?;
            decoded.push((high << 4) | low);
            index += 3;
        } else {
            decoded.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(decoded).ok()
}

fn hex_value(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        b'A'..=b'F' => Some(value - b'A' + 10),
        _ => None,
    }
}

fn is_external_target(target: &str) -> bool {
    let lower = target.to_ascii_lowercase();
    lower.starts_with("http://")
        || lower.starts_with("https://")
        || lower.starts_with("mailto:")
        || lower.starts_with("data:")
        || lower.starts_with("obsidian:")
        || lower.starts_with("//")
}

fn parse_wiki_link(
    document_id: DocumentId,
    ordinal: usize,
    raw: &str,
    start: usize,
    end: usize,
    embed: bool,
) -> Link {
    let (target, display) = raw
        .split_once('|')
        .map_or((raw, None), |(target, display)| {
            (target, Some(display.to_owned()))
        });
    let (before_block, block_id) = target
        .split_once('^')
        .map_or((target, None), |(path, block)| {
            (path, Some(block.to_owned()))
        });
    let (path, heading) = before_block
        .split_once('#')
        .map_or((before_block, None), |(path, heading)| {
            (path, Some(heading.to_owned()))
        });
    let ordinal = ordinal as u64;
    let link_id = LinkId::from_parts(
        "vaultc:link:v1\0",
        &[
            document_id.hash().as_bytes(),
            &ordinal.to_be_bytes(),
            raw.as_bytes(),
        ],
    );
    Link {
        link_id,
        syntax: LinkSyntax::Wiki,
        span: SourceSpan {
            byte_start: start as u64,
            byte_end: end as u64,
        },
        raw_target: raw.to_owned(),
        path: (!path.is_empty()).then(|| path.to_owned()),
        heading,
        block_id,
        display,
        embed,
        resolution: LinkResolution::Pending,
    }
}

fn parse_canvas(
    source_file: SourceFile,
    bytes: &[u8],
    _diagnostics: &mut Vec<Diagnostic>,
) -> Result<Canvas> {
    let (value, file_references) = parse_canvas_json(&source_file.logical_path, bytes)?;
    let canvas_id = CanvasId::from_parts(
        "vaultc:canvas:v1\0",
        &[
            source_file.snapshot_id.hash().as_bytes(),
            source_file.file_id.hash().as_bytes(),
        ],
    );
    Ok(Canvas {
        canvas_id,
        source_file,
        value,
        file_references,
    })
}

pub(crate) fn parse_canvas_json(
    logical_path: &str,
    bytes: &[u8],
) -> Result<(Value, Vec<CanvasFileReference>)> {
    let text = std::str::from_utf8(bytes).map_err(|error| VaultcError::MalformedInput {
        path: logical_path.to_owned(),
        reason: format!("Canvas must be UTF-8: {error}"),
    })?;
    let UniqueJsonValue(value) =
        serde_json::from_str(text).map_err(|error| VaultcError::MalformedInput {
            path: logical_path.to_owned(),
            reason: format!("invalid JSON Canvas: {error}"),
        })?;
    let file_references = canvas_file_references(&value, logical_path)?;
    Ok((value, file_references))
}

pub(crate) fn canvas_file_references(
    value: &Value,
    logical_path: &str,
) -> Result<Vec<CanvasFileReference>> {
    let object = value
        .as_object()
        .ok_or_else(|| VaultcError::MalformedInput {
            path: logical_path.to_owned(),
            reason: "JSON Canvas root must be an object".into(),
        })?;
    let nodes: &[Value] = match object.get("nodes") {
        Some(nodes) => nodes
            .as_array()
            .ok_or_else(|| VaultcError::MalformedInput {
                path: logical_path.to_owned(),
                reason: "JSON Canvas `nodes` must be an array".into(),
            })?,
        None => &[],
    };
    if object.get("edges").is_some_and(|edges| !edges.is_array()) {
        return Err(VaultcError::MalformedInput {
            path: logical_path.to_owned(),
            reason: "JSON Canvas `edges` must be an array".into(),
        });
    }

    let mut node_ids = BTreeSet::new();
    let mut file_references = Vec::new();
    for node in nodes {
        let node = node
            .as_object()
            .ok_or_else(|| VaultcError::MalformedInput {
                path: logical_path.to_owned(),
                reason: "JSON Canvas nodes must be objects".into(),
            })?;
        let node_id =
            node.get("id")
                .and_then(Value::as_str)
                .ok_or_else(|| VaultcError::MalformedInput {
                    path: logical_path.to_owned(),
                    reason: "JSON Canvas node is missing a string `id`".into(),
                })?;
        if node_id.is_empty() {
            return Err(VaultcError::MalformedInput {
                path: logical_path.to_owned(),
                reason: "JSON Canvas node ID must not be empty".into(),
            });
        }
        if !node_ids.insert(node_id) {
            return Err(VaultcError::MalformedInput {
                path: logical_path.to_owned(),
                reason: format!("JSON Canvas contains duplicate node ID `{node_id}`"),
            });
        }
        if node.get("type").and_then(Value::as_str) == Some("file") {
            let raw_path = node.get("file").and_then(Value::as_str).ok_or_else(|| {
                VaultcError::MalformedInput {
                    path: logical_path.to_owned(),
                    reason: format!("JSON Canvas file node `{node_id}` is missing a string `file`"),
                }
            })?;
            file_references.push(CanvasFileReference {
                node_id: node_id.to_owned(),
                raw_path: raw_path.to_owned(),
                resolution: CanvasReferenceResolution::Pending,
            });
        }
    }
    file_references.sort_by(|left, right| left.node_id.as_bytes().cmp(right.node_id.as_bytes()));
    Ok(file_references)
}

struct UniqueJsonValue(Value);

impl<'de> Deserialize<'de> for UniqueJsonValue {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_any(UniqueJsonVisitor)
    }
}

struct UniqueJsonVisitor;

impl<'de> Visitor<'de> for UniqueJsonVisitor {
    type Value = UniqueJsonValue;

    fn expecting(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("a JSON value without duplicate object keys")
    }

    fn visit_bool<E>(self, value: bool) -> std::result::Result<Self::Value, E> {
        Ok(UniqueJsonValue(Value::Bool(value)))
    }

    fn visit_i64<E>(self, value: i64) -> std::result::Result<Self::Value, E> {
        Ok(UniqueJsonValue(Value::Number(value.into())))
    }

    fn visit_u64<E>(self, value: u64) -> std::result::Result<Self::Value, E> {
        Ok(UniqueJsonValue(Value::Number(value.into())))
    }

    fn visit_f64<E>(self, value: f64) -> std::result::Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        let number = serde_json::Number::from_f64(value)
            .ok_or_else(|| E::custom("JSON number must be finite"))?;
        Ok(UniqueJsonValue(Value::Number(number)))
    }

    fn visit_str<E>(self, value: &str) -> std::result::Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        self.visit_string(value.to_owned())
    }

    fn visit_string<E>(self, value: String) -> std::result::Result<Self::Value, E> {
        Ok(UniqueJsonValue(Value::String(value)))
    }

    fn visit_none<E>(self) -> std::result::Result<Self::Value, E> {
        Ok(UniqueJsonValue(Value::Null))
    }

    fn visit_unit<E>(self) -> std::result::Result<Self::Value, E> {
        Ok(UniqueJsonValue(Value::Null))
    }

    fn visit_seq<A>(self, mut sequence: A) -> std::result::Result<Self::Value, A::Error>
    where
        A: SeqAccess<'de>,
    {
        let mut values = Vec::new();
        while let Some(UniqueJsonValue(value)) = sequence.next_element()? {
            values.push(value);
        }
        Ok(UniqueJsonValue(Value::Array(values)))
    }

    fn visit_map<A>(self, mut object: A) -> std::result::Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        let mut values = serde_json::Map::new();
        while let Some(key) = object.next_key::<String>()? {
            if values.contains_key(&key) {
                return Err(serde::de::Error::custom(format!(
                    "duplicate JSON object key `{key}`"
                )));
            }
            let UniqueJsonValue(value) = object.next_value()?;
            values.insert(key, value);
        }
        Ok(UniqueJsonValue(Value::Object(values)))
    }
}

pub(crate) fn lookup_keys(document: &Document) -> BTreeSet<String> {
    let mut keys = BTreeSet::new();
    if let Some(title) = &document.title {
        keys.insert(normalize_lookup_key(title));
    }
    for alias in &document.aliases {
        keys.insert(normalize_lookup_key(alias));
    }
    if let Some(stem) = Path::new(&document.source_file.logical_path)
        .file_stem()
        .and_then(|value| value.to_str())
    {
        keys.insert(normalize_lookup_key(stem));
    }
    keys
}

pub(crate) fn normalize_lookup_key(value: &str) -> String {
    full_casefold_nfc(value.trim())
}

pub(crate) fn full_casefold_nfc(value: &str) -> String {
    let normalized: String = value.nfc().collect();
    caseless::default_case_fold_str(&normalized)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::{SnapshotId, SourceFileId};
    use crate::source::SourceId;

    fn fixture_file(path: &str, bytes: &[u8]) -> SourceFile {
        let hash = ContentHash::from_bytes(bytes);
        SourceFile {
            source_id: SourceId::new("fixture").expect("source ID"),
            snapshot_id: SnapshotId::from_parts("fixture\0", &[b"snapshot"]),
            file_id: SourceFileId::from_parts("fixture\0", &[path.as_bytes(), hash.as_bytes()]),
            original_path: path.into(),
            logical_path: path.into(),
            path_encoding: crate::ir::SourcePathEncoding::Utf8,
            kind: FileKind::Markdown,
            byte_len: bytes.len() as u64,
            content_hash: hash,
        }
    }

    fn span_bytes<'a>(source: &'a [u8], span: &SourceSpan) -> &'a [u8] {
        let start = usize::try_from(span.byte_start).expect("test span start fits usize");
        let end = usize::try_from(span.byte_end).expect("test span end fits usize");
        &source[start..end]
    }

    #[test]
    fn ignores_wikilinks_inside_inline_code_and_fences() {
        let source = b"[[Real]] `[[Inline]]`\n```\n[[Fence]]\n```\n";
        let mut diagnostics = Vec::new();
        let document = parse_markdown(
            fixture_file("A.md", source),
            source,
            &CompilerPolicy::default(),
            &mut diagnostics,
        )
        .expect("parse Markdown");
        assert_eq!(document.links.len(), 1);
        assert_eq!(document.links[0].path.as_deref(), Some("Real"));
    }

    #[test]
    fn parses_embed_components() {
        let source = b"![[A.png#Section^block|label]]";
        let mut diagnostics = Vec::new();
        let document = parse_markdown(
            fixture_file("A.md", source),
            source,
            &CompilerPolicy::default(),
            &mut diagnostics,
        )
        .expect("parse Markdown");
        let link = &document.links[0];
        assert!(link.embed);
        assert_eq!(link.path.as_deref(), Some("A.png"));
        assert_eq!(link.heading.as_deref(), Some("Section"));
        assert_eq!(link.block_id.as_deref(), Some("block"));
        assert_eq!(link.display.as_deref(), Some("label"));
    }

    #[test]
    fn parses_ordinary_markdown_links_with_exact_target_spans() {
        let source =
            b"See [topic](../Topic%20One.md#Intro \"title\") and ![asset](<img/My Image.png>).";
        let mut diagnostics = Vec::new();
        let document = parse_markdown(
            fixture_file("notes/A.md", source),
            source,
            &CompilerPolicy::default(),
            &mut diagnostics,
        )
        .expect("parse Markdown");
        assert_eq!(document.links.len(), 2);
        let topic = &document.links[0];
        assert_eq!(topic.syntax, LinkSyntax::Markdown);
        assert_eq!(topic.path.as_deref(), Some("../Topic One.md"));
        assert_eq!(topic.heading.as_deref(), Some("Intro"));
        assert_eq!(topic.display.as_deref(), Some("topic"));
        assert_eq!(span_bytes(source, &topic.span), b"../Topic%20One.md#Intro");
        let asset = &document.links[1];
        assert!(asset.embed);
        assert_eq!(asset.path.as_deref(), Some("img/My Image.png"));
        assert_eq!(span_bytes(source, &asset.span), b"img/My Image.png");
    }

    #[test]
    fn duplicate_frontmatter_keys_fail_closed() {
        let source = b"---\ntitle: first\ntitle: second\n---\n# Body\n";
        let mut diagnostics = Vec::new();
        let error = parse_markdown(
            fixture_file("duplicate.md", source),
            source,
            &CompilerPolicy::default(),
            &mut diagnostics,
        )
        .expect_err("duplicate YAML keys must not be silently overwritten");
        assert!(matches!(error, VaultcError::MalformedInput { .. }));
    }
}
