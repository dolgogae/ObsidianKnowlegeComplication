from __future__ import annotations

import hashlib
import json
import threading
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from typing import Any

import pytest

import okc


_V3_FIXTURE_ARTIFACT_SHA256 = (
    "452ca0671e806a93b4f36f218cf9e62da899f6404c74705c2cf0ca14e413c7e5"
)
_BINDING_FIXTURES = Path(__file__).resolve().parents[2] / "fixtures"


def _artifact_digest(root: Path) -> str:
    lines = []
    for item in sorted(path for path in root.rglob("*") if path.is_file()):
        relative = "./" + item.relative_to(root).as_posix()
        lines.append(f"{hashlib.sha256(item.read_bytes()).hexdigest()}  {relative}\n")
    return hashlib.sha256("".join(lines).encode()).hexdigest()


class _FixtureProvider(BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"

    def log_message(self, format: str, *args: object) -> None:
        del format, args

    def do_POST(self) -> None:  # noqa: N802 - BaseHTTPRequestHandler contract
        length = int(self.headers.get("content-length", "0"))
        request = json.loads(self.rfile.read(length))
        if self.path == "/api/embed":
            inputs = request["input"]
            response = {
                "model": "fixture-model",
                "embeddings": [[1, index + 1] for index in range(len(inputs))],
                "prompt_eval_count": len(inputs),
            }
        elif self.path == "/api/chat":
            task_input = json.loads(request["messages"][1]["content"])
            output = _structured_output(task_input)
            response = {
                "model": "fixture-model",
                "message": {
                    "role": "assistant",
                    "content": json.dumps(output, separators=(",", ":")),
                },
                "done": True,
                "done_reason": "stop",
                "prompt_eval_count": 1,
                "eval_count": 1,
            }
        else:
            self.send_error(404)
            return
        encoded = json.dumps(response, separators=(",", ":")).encode()
        self.send_response(200)
        self.send_header("content-type", "application/json")
        self.send_header("content-length", str(len(encoded)))
        self.send_header("connection", "close")
        self.end_headers()
        self.wfile.write(encoded)
        self.close_connection = True


class _InvalidProvider(BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"

    def log_message(self, format: str, *args: object) -> None:
        del format, args

    def do_POST(self) -> None:  # noqa: N802 - BaseHTTPRequestHandler contract
        length = int(self.headers.get("content-length", "0"))
        self.rfile.read(length)
        encoded = b"{}"
        self.send_response(200)
        self.send_header("content-type", "application/json")
        self.send_header("content-length", str(len(encoded)))
        self.send_header("connection", "close")
        self.end_headers()
        self.wfile.write(encoded)
        self.close_connection = True


def _structured_output(task_input: dict[str, Any]) -> dict[str, Any]:
    if "semantic_candidates" in task_input:
        document_ids = [item["document_id"] for item in task_input["documents"]]
        return {
            "clusters": [
                {
                    "cluster_id": "sdk-fixture",
                    "title": "SDK fixture",
                    "canonical_path": "sdk/fixture.md",
                    "document_ids": document_ids,
                }
            ]
        }
    if "proposal" in task_input:
        return {"findings": []}

    dispositions: list[dict[str, Any]] = []
    for document in task_input["documents"]:
        for block in document["blocks"]:
            dispositions.append(
                {
                    "kind": "block",
                    "document_id": document["document_id"],
                    "target_id": block["block_id"],
                    "content_hash": block["content_hash"],
                    "disposition": "preserved_verbatim",
                    "rationale": "",
                }
            )
        for metadata in document["metadata"]:
            dispositions.append(
                {
                    "kind": "metadata",
                    "document_id": document["document_id"],
                    "target_id": metadata["metadata_id"],
                    "content_hash": metadata["content_hash"],
                    "disposition": "preserved_verbatim",
                    "rationale": "",
                }
            )
    return {
        "sections": [],
        "related_links": [],
        "dispositions": dispositions,
        "contradictions": [],
    }


def test_api_info_and_relative_path_error_are_structured() -> None:
    client = okc.OkcClient()
    assert client.api_info()["interop_schema_version"] == 1

    job = client.open_project("relative.okc-project")
    with pytest.raises(okc.OkcError) as raised:
        job.result()
    assert raised.value.code == "PATH_NOT_ABSOLUTE"
    assert raised.value.category == "path"


def test_create_open_and_manifest_round_trip(tmp_path: Path) -> None:
    client = okc.OkcClient()
    root = tmp_path / "python.okc-project"
    project = client.create_project(
        root, name="Python", curator_id="curator", language="ko-KR"
    ).result()

    assert Path(project.path).samefile(root)
    manifest = project.manifest().result()["payload"]
    assert manifest["name"] == "Python"
    assert manifest["language"] == "ko-KR"
    assert Path(client.open_project(root).result().path).samefile(root)


def test_provider_profile_is_immutable_and_command_is_rejected() -> None:
    profile = okc.ProviderProfile(
        name="local", kind="ollama", endpoint="http://127.0.0.1:11434", model="test"
    )
    with pytest.raises((AttributeError, TypeError)):
        profile.options["secret"] = "value"  # type: ignore[index]

    command = okc.ProviderProfile(
        name="command", kind="command", endpoint="ignored", model="ignored"
    )
    with pytest.raises(okc.OkcError) as raised:
        okc.OkcClient([command])
    assert raised.value.code == "PROVIDER_UNSUPPORTED"


def test_missing_environment_secret_is_structured(monkeypatch: pytest.MonkeyPatch) -> None:
    variable = "OKC_PYTHON_TEST_SECRET_2A44F5_DO_NOT_SET"
    monkeypatch.delenv(variable, raising=False)
    profile = okc.ProviderProfile(
        name="missing-secret",
        kind="open_ai_compatible",
        endpoint="http://127.0.0.1:9",
        model="fixture",
        api_key_env=variable,
    )
    with pytest.raises(okc.OkcError) as raised:
        okc.OkcClient([profile]).test_provider("missing-secret").result()
    assert raised.value.code == "PROVIDER_ENV_SECRET_MISSING"
    assert raised.value.category == "provider"


def test_invalid_provider_response_is_structured() -> None:
    server = ThreadingHTTPServer(("127.0.0.1", 0), _InvalidProvider)
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    try:
        profile = okc.ProviderProfile(
            name="invalid-response",
            kind="ollama",
            endpoint=f"http://127.0.0.1:{server.server_port}",
            model="fixture",
        )
        with pytest.raises(okc.OkcError) as raised:
            okc.OkcClient([profile]).test_provider("invalid-response").result()
        assert raised.value.code == "PROVIDER_RESPONSE_INVALID"
        assert raised.value.category == "provider"
    finally:
        server.shutdown()
        server.server_close()
        thread.join(timeout=5)


def test_remote_cache_miss_requires_per_call_consent(tmp_path: Path) -> None:
    source = tmp_path / "remote-source"
    source.mkdir()
    (source / "Note.md").write_text("# Remote consent\n", encoding="utf-8")
    profile = okc.ProviderProfile(
        name="remote",
        kind="open_ai",
        endpoint="https://provider.invalid",
        model="fixture",
        timeout_ms=10,
    )
    client = okc.OkcClient([profile])
    project = client.create_project(
        tmp_path / "remote.okc-project",
        name="Remote consent",
        curator_id="sdk-test",
    ).result()
    project.add_source(okc.SourceInput("remote", source)).result()
    project.set_ai_route("remote").result()

    with pytest.raises(okc.OkcError) as raised:
        project.integrate(
            allow_remote_provider=False,
            remote_disclosure_confirmed=False,
        ).result()
    assert raised.value.code == "REMOTE_CONSENT_REQUIRED"
    assert raised.value.category == "consent"


@pytest.mark.parametrize(("directory", "family"), [("v1-basic", "v1"), ("v2-basic", "v2")])
def test_legacy_artifacts_are_auto_detected_verified_and_explained(
    directory: str, family: str
) -> None:
    artifact = (_BINDING_FIXTURES / directory).resolve()
    client = okc.OkcClient()
    verification = client.verify_artifact(artifact).result()
    assert verification["interop_schema_version"] == 1
    assert verification["family"] == family
    assert verification["valid"] is True
    explanation = client.explain_artifact(
        artifact, output_path="knowledge/Topic.md"
    ).result()
    assert explanation["interop_schema_version"] == 1
    assert explanation["family"] == family


def test_invalid_language_does_not_leave_partial_project(tmp_path: Path) -> None:
    client = okc.OkcClient()
    root = tmp_path / "invalid.okc-project"
    with pytest.raises(okc.OkcError):
        client.create_project(
            root, name="Invalid", curator_id="curator", language="bad tag"
        ).result()
    assert not root.exists()


def test_complete_v3_approval_compile_verify_and_explain(tmp_path: Path) -> None:
    server = ThreadingHTTPServer(("127.0.0.1", 0), _FixtureProvider)
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    try:
        source = tmp_path / "source"
        source.mkdir()
        source_note = source / "Fixture.md"
        source_note.write_text(
            "# SDK fixture\n\nEvidence retained verbatim.\n", encoding="utf-8"
        )
        source_bytes = source_note.read_bytes()
        endpoint = f"http://127.0.0.1:{server.server_port}"
        profile = okc.ProviderProfile(
            name="fixture",
            kind="ollama",
            endpoint=endpoint,
            model="fixture-model",
        )
        client = okc.OkcClient([profile])
        project = client.create_project(
            tmp_path / "python-v3.okc-project",
            name="SDK parity",
            curator_id="sdk-test",
            language="en",
        ).result()
        project.add_source(okc.SourceInput("fixture", source)).result()
        project.set_ai_route("fixture").result()

        first = project.integrate(
            allow_remote_provider=False, remote_disclosure_confirmed=False
        ).result()
        assert first["interop_schema_version"] == 1
        assert first["checkpoint"] == "needs_taxonomy"
        taxonomy = project.taxonomy().result()
        assert taxonomy["interop_schema_version"] == 1
        cluster_id = taxonomy["taxonomy"]["clusters"][0]["cluster_id"]
        project.approve_taxonomy(rationale="fixture taxonomy reviewed").result()

        second = project.integrate(
            allow_remote_provider=False, remote_disclosure_confirmed=False
        ).result()
        assert second["checkpoint"] == "needs_clusters"
        assert len(project.clusters().result()["payload"]) == 1
        project.approve_cluster(cluster_id).result()

        final = project.integrate(
            allow_remote_provider=False, remote_disclosure_confirmed=False
        ).result()
        assert final["checkpoint"] == "ready_to_compile"
        output = tmp_path / "python-output"
        compiled = project.compile(output).result()
        assert compiled["path"] == str(output)
        assert _artifact_digest(output) == _V3_FIXTURE_ARTIFACT_SHA256
        verification = client.verify_artifact(output).result()
        assert verification["family"] == "v3"
        assert verification["valid"] is True
        explanation = client.explain_artifact(
            output, output_path="knowledge/sdk/fixture.md"
        ).result()
        assert explanation["family"] == "v3"
        with pytest.raises(okc.OkcError) as exists:
            project.compile(output).result()
        assert exists.value.code == "OUTPUT_EXISTS"
        with pytest.raises(okc.OkcError) as overlap:
            project.compile(source).result()
        assert overlap.value.code == "OUTPUT_OVERLAP"
        assert source_note.read_bytes() == source_bytes
    finally:
        server.shutdown()
        server.server_close()
        thread.join(timeout=5)
