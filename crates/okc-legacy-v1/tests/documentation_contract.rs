use std::fs;
use std::path::{Path, PathBuf};

fn repository_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("vaultc crate must be nested under the repository root")
        .to_path_buf()
}

fn collect_markdown_files(directory: &Path, files: &mut Vec<PathBuf>) {
    let mut entries = fs::read_dir(directory)
        .unwrap_or_else(|error| {
            panic!("read Markdown directory `{}`: {error}", directory.display())
        })
        .map(|entry| entry.expect("read Markdown directory entry").path())
        .collect::<Vec<_>>();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            collect_markdown_files(&path, files);
        } else if path.extension().is_some_and(|extension| extension == "md") {
            files.push(path);
        }
    }
}

fn decode_percent_path(value: &str) -> Result<String, String> {
    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            let high = bytes
                .get(index + 1)
                .and_then(|byte| (*byte as char).to_digit(16))
                .ok_or_else(|| format!("invalid percent escape in `{value}`"))?;
            let low = bytes
                .get(index + 2)
                .and_then(|byte| (*byte as char).to_digit(16))
                .ok_or_else(|| format!("invalid percent escape in `{value}`"))?;
            let octet = u8::try_from((high << 4) | low)
                .map_err(|_| format!("percent escape overflow in `{value}`"))?;
            decoded.push(octet);
            index += 3;
        } else {
            decoded.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(decoded).map_err(|error| format!("non-UTF-8 link `{value}`: {error}"))
}

fn has_uri_scheme(target: &str) -> bool {
    let mut characters = target.chars();
    if !characters
        .next()
        .is_some_and(|character| character.is_ascii_alphabetic())
    {
        return false;
    }
    for character in characters {
        match character {
            ':' => return true,
            '/' | '#' => return false,
            value if value.is_ascii_alphanumeric() || matches!(value, '+' | '-' | '.') => {}
            _ => return false,
        }
    }
    false
}

fn target_path(raw: &str) -> Option<&str> {
    let trimmed = raw.trim();
    if let Some(without_open) = trimmed.strip_prefix('<') {
        return without_open.split_once('>').map(|(target, _)| target);
    }
    trimmed.split_ascii_whitespace().next()
}

#[test]
fn repository_relative_markdown_links_resolve() {
    let root = repository_root();
    let mut files = fs::read_dir(&root)
        .expect("read repository root")
        .map(|entry| entry.expect("read root entry").path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "md"))
        .collect::<Vec<_>>();
    collect_markdown_files(&root.join("docs"), &mut files);
    files.sort();

    let mut failures = Vec::new();
    for file in files {
        let content = fs::read_to_string(&file)
            .unwrap_or_else(|error| panic!("read Markdown `{}`: {error}", file.display()));
        let mut fenced = false;
        for (line_index, line) in content.lines().enumerate() {
            let trimmed = line.trim_start();
            if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
                fenced = !fenced;
                continue;
            }
            if fenced {
                continue;
            }

            let mut remainder = line;
            while let Some(open) = remainder.find("](") {
                let after_open = &remainder[open + 2..];
                let Some(close) = after_open.find(')') else {
                    break;
                };
                let raw = &after_open[..close];
                remainder = &after_open[close + 1..];
                let Some(target) = target_path(raw) else {
                    continue;
                };
                if target.is_empty() || target.starts_with('#') || has_uri_scheme(target) {
                    continue;
                }
                let path_without_fragment = target.split_once('#').map_or(target, |(path, _)| path);
                match decode_percent_path(path_without_fragment) {
                    Ok(decoded) => {
                        let resolved = file
                            .parent()
                            .expect("Markdown file has a parent")
                            .join(decoded);
                        if !resolved.exists() {
                            failures.push(format!(
                                "{}:{}: `{target}`",
                                file.strip_prefix(&root).unwrap_or(&file).display(),
                                line_index + 1
                            ));
                        }
                    }
                    Err(error) => failures.push(format!(
                        "{}:{}: {error}",
                        file.strip_prefix(&root).unwrap_or(&file).display(),
                        line_index + 1
                    )),
                }
            }
        }
    }

    assert!(
        failures.is_empty(),
        "repository-relative Markdown links must resolve:\n{}",
        failures.join("\n")
    );
}
