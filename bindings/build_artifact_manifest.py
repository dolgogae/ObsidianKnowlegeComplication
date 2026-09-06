#!/usr/bin/env python3
"""Validate SDK package metadata and write deterministic SHA-256 checksums."""

from __future__ import annotations

import hashlib
import json
import sys
import tarfile
import zipfile
from pathlib import Path


def main() -> int:
    if len(sys.argv) != 2:
        raise SystemExit("usage: build_artifact_manifest.py ARTIFACT_DIRECTORY")
    root = Path(sys.argv[1])
    if not root.is_dir():
        raise SystemExit(f"artifact directory does not exist: {root}")

    wheels = sorted(root.glob("okc_compiler-*.whl"))
    for wheel in wheels:
        with zipfile.ZipFile(wheel) as archive:
            sboms = [
                name
                for name in archive.namelist()
                if ".dist-info/sboms/" in name and name.endswith(".json")
            ]
            for name in sboms:
                validate_cyclonedx(archive.read(name), f"{wheel.name}:{name}")
        if not sboms:
            raise SystemExit(f"Python wheel has no embedded SBOM: {wheel.name}")

    source_distributions = sorted(root.glob("okc_compiler-*.tar.gz"))
    for source_distribution in source_distributions:
        with tarfile.open(source_distribution, mode="r:gz") as archive:
            if not any(name.endswith("/Cargo.lock") for name in archive.getnames()):
                raise SystemExit(
                    f"Python source distribution has no Cargo.lock: "
                    f"{source_distribution.name}"
                )

    for sbom in sorted(root.glob("*.cyclonedx.json")):
        validate_cyclonedx(sbom.read_bytes(), sbom.name)

    checksum_path = root / "SHA256SUMS"
    artifacts = sorted(
        path
        for path in root.iterdir()
        if path.is_file() and path.name != checksum_path.name
    )
    if not artifacts:
        raise SystemExit(f"artifact directory is empty: {root}")
    lines = [
        f"{hashlib.sha256(path.read_bytes()).hexdigest()}  {path.name}\n"
        for path in artifacts
    ]
    with checksum_path.open("w", encoding="utf-8", newline="\n") as stream:
        stream.write("".join(lines))
    return 0


def validate_cyclonedx(content: bytes, label: str) -> None:
    try:
        value = json.loads(content)
    except (UnicodeDecodeError, json.JSONDecodeError) as error:
        raise SystemExit(f"invalid CycloneDX JSON in {label}: {error}") from error
    if not isinstance(value, dict) or value.get("bomFormat") != "CycloneDX":
        raise SystemExit(f"invalid CycloneDX document in {label}")


if __name__ == "__main__":
    raise SystemExit(main())
