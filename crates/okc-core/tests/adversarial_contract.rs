//! Fixed-seed mutation/property smoke tests. These are reproducible regression
//! evidence, not a replacement for a coverage-guided fuzz campaign.

use std::fs::{self, File};
use std::io::Write as _;

use okc_core::{CorpusBuilder, SourceSpec, parse_json_strict};
use zip::write::SimpleFileOptions;

#[test]
fn strict_json_fixed_seed_mutations_never_panic_or_disagree_on_accepted_values() {
    let seeds: &[&[u8]] = &[
        br#"{"schema_version":3,"metadata":{"a":[1,true,null,"text"]}}"#,
        br#"{"nested":{"a":1,"a":2}}"#,
        br#"[0,-1,1e300,"\uD800"]"#,
        b"{} trailing",
    ];
    for seed in seeds {
        for index in 0..seed.len() {
            for replacement in [0, b'"', b'{', b'}', b'[', b']', b',', b'\\', 0xff] {
                let mut mutated = seed.to_vec();
                mutated[index] = replacement;
                if let Ok(value) = parse_json_strict(&mutated) {
                    assert_eq!(
                        value,
                        serde_json::from_slice::<serde_json::Value>(&mutated).unwrap()
                    );
                }
            }
            let _ = parse_json_strict(&seed[..index]);
        }
    }
    for depth in 0..140 {
        let input = format!(
            "{}{{\"a\":1,\"\\u0061\":2}}{}",
            "[".repeat(depth),
            "]".repeat(depth)
        );
        assert!(parse_json_strict(input.as_bytes()).is_err());
    }
}

#[test]
fn zip_fixed_seed_header_and_truncation_mutations_are_bounded() {
    let temporary = tempfile::tempdir().unwrap();
    let seed_path = temporary.path().join("seed.zip");
    let mut archive = zip::ZipWriter::new(File::create(&seed_path).unwrap());
    archive
        .start_file("Note.md", SimpleFileOptions::default())
        .unwrap();
    archive.write_all(b"# A bounded archive fixture\n").unwrap();
    archive.finish().unwrap();
    let seed = fs::read(&seed_path).unwrap();
    let input = temporary.path().join("mutated.zip");
    for index in 0..seed.len() {
        let mut mutated = seed.clone();
        mutated[index] ^= 0xff;
        fs::write(&input, mutated).unwrap();
        let _ = CorpusBuilder::new().build([SourceSpec::archive("fixture", &input).unwrap()]);
        fs::write(&input, &seed[..index]).unwrap();
        let _ = CorpusBuilder::new().build([SourceSpec::archive("fixture", &input).unwrap()]);
    }
}

#[test]
fn corpus_creation_order_and_absolute_roots_do_not_change_sealed_bytes() {
    let mut expected = None;
    for round in 0..12 {
        let temporary = tempfile::tempdir().unwrap();
        let mut inputs = Vec::new();
        for source in 0..3 {
            let root = temporary.path().join(format!("root-{source}"));
            fs::create_dir(&root).unwrap();
            for offset in 0..5 {
                let index = (offset * 3 + round) % 5;
                fs::write(
                    root.join(format!("Note-{index}.md")),
                    format!("# Source {source} note {index}\n"),
                )
                .unwrap();
            }
            inputs.push(SourceSpec::directory(format!("source-{source}"), root).unwrap());
        }
        inputs.rotate_left(round % 3);
        if round % 2 == 0 {
            inputs.reverse();
        }
        let prepared = CorpusBuilder::new().build(inputs).unwrap();
        let bytes = okc_core::to_canonical_json(&prepared.corpus).unwrap();
        if let Some(expected) = &expected {
            assert_eq!(&bytes, expected);
        } else {
            expected = Some(bytes);
        }
    }
}
