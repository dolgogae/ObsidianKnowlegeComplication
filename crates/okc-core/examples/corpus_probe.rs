//! Bounded, provider-free ingestion probe; not the semantic QG-006 benchmark.
//! Run the built executable under the host's peak-RSS measurement tool.

use std::fs;
use std::time::Instant;

use okc_core::{CorpusBuilder, SourceSpec};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let arguments: Vec<_> = std::env::args().skip(1).collect();
    if arguments.len() != 3 {
        return Err(
            "usage: corpus_probe SOURCES NOTES BYTES_PER_NOTE (temporary fixture only)".into(),
        );
    }
    let sources: usize = arguments[0].parse()?;
    let notes: usize = arguments[1].parse()?;
    let bytes_per_note: usize = arguments[2].parse()?;
    let accepted_bytes = notes
        .checked_mul(bytes_per_note)
        .ok_or("byte count overflow")?;
    if !(1..=10).contains(&sources)
        || !(sources..=100_000).contains(&notes)
        || !(128..=16 * 1024 * 1024).contains(&bytes_per_note)
        || u64::try_from(accepted_bytes)? > 20 * 1024 * 1024 * 1024
    {
        return Err("probe arguments exceed the current source safety limits".into());
    }
    let temporary = tempfile::Builder::new()
        .prefix("okc-corpus-probe-")
        .tempdir()?;
    let start = Instant::now();
    let mut inputs = Vec::new();
    for index in 0..sources {
        let root = temporary.path().join(format!("vault-{index:02}"));
        fs::create_dir(&root)?;
        inputs.push(SourceSpec::directory(format!("source-{index:02}"), root)?);
    }
    for index in 0..notes {
        let mut text = format!("# Note {index:06}\n\nDeterministic probe evidence {index:06}.\n");
        text.extend(std::iter::repeat_n('x', bytes_per_note - text.len() - 1));
        text.push('\n');
        fs::write(
            inputs[index % sources]
                .path()
                .join(format!("Note-{index:06}.md")),
            text,
        )?;
    }
    let fixture_ms = start.elapsed().as_millis();
    let start = Instant::now();
    let prepared = CorpusBuilder::new().build(inputs)?;
    let build_ms = start.elapsed().as_millis();
    if prepared.source_count != sources || prepared.corpus.documents.len() != notes {
        return Err("probe accepted count mismatch".into());
    }
    println!(
        "{}",
        serde_json::to_string(&serde_json::json!({
            "probe_version": 1,
            "profile": "repeated-ascii-unique-title-v1",
            "source_count": sources,
            "note_count": notes,
            "bytes_per_note": bytes_per_note,
            "accepted_bytes": accepted_bytes,
            "corpus_hash": prepared.corpus.corpus_hash.hex(),
            "block_count": prepared.block_texts.len(),
            "fixture_ms": fixture_ms,
            "build_ms": build_ms,
            "provider_calls": 0,
            "semantic_candidates": null,
            "qg_006_pass": false
        }))?
    );
    Ok(())
}
