#!/usr/bin/env python3
"""POSIX PTY workflow against a synthetic loopback provider, no credentials.

Usage: python3 tests/tui_pty_smoke.py /absolute/path/to/okc
All projects, provider config, and output live in a disposable temporary folder.
This smoke is not Windows ConPTY or real-provider conformance evidence.
"""

from __future__ import annotations

import codecs
import fcntl
import hashlib
import json
import os
from pathlib import Path
import pty
import re
import select
import struct
import subprocess
import sys
import tempfile
import termios
import threading
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer


def proposal(task: dict) -> dict:
    if "semantic_candidates" in task:
        return {
            "clusters": [{
                "cluster_id": "pty-fixture",
                "title": "PTY fixture",
                "canonical_path": "pty/fixture.md",
                "document_ids": [item["document_id"] for item in task["documents"]],
            }]
        }
    if "proposal" in task:
        return {"findings": []}
    dispositions = []
    for document in task["documents"]:
        for kind, field, identity in [
            ("block", "blocks", "block_id"),
            ("metadata", "metadata", "metadata_id"),
        ]:
            for item in document[field]:
                dispositions.append({
                    "kind": kind,
                    "document_id": document["document_id"],
                    "target_id": item[identity],
                    "content_hash": item["content_hash"],
                    "disposition": "preserved_verbatim",
                    "rationale": "",
                })
    return {"sections": [], "related_links": [], "dispositions": dispositions, "contradictions": []}


class Provider(BaseHTTPRequestHandler):
    requests = 0

    def log_message(self, *_args: object) -> None:
        pass

    def do_POST(self) -> None:
        type(self).requests += 1
        if type(self).requests > 20:
            self.send_error(429)
            return
        length = int(self.headers.get("content-length", "0"))
        if not 0 < length <= 1024 * 1024:
            self.send_error(413)
            return
        request = json.loads(self.rfile.read(length))
        if self.path == "/api/embed":
            response = {"model": "fixture-model", "embeddings": [[1, 1] for _ in request["input"]]}
        elif self.path == "/api/chat":
            task = json.loads(request["messages"][1]["content"])
            response = {"model": "fixture-model", "message": {
                "role": "assistant", "content": json.dumps(proposal(task)),
            }, "done": True, "done_reason": "stop"}
        else:
            self.send_error(404)
            return
        body = json.dumps(response).encode()
        self.send_response(200)
        self.send_header("content-type", "application/json")
        self.send_header("content-length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)


class Terminal:
    def __init__(self, binary: Path, project: Path, cwd: Path, environment: dict[str, str]):
        self.master, self.slave = pty.openpty()
        self.before = termios.tcgetattr(self.slave)
        fcntl.ioctl(self.slave, termios.TIOCSWINSZ, struct.pack("HHHH", 48, 180, 0, 0))
        self.trace = bytearray()
        self.grid = [[" "] * 180 for _ in range(48)]
        self.row = self.column = 0
        self.pending = ""
        self.decoder = codecs.getincrementaldecoder("utf-8")("replace")
        self.process = subprocess.Popen(
            [str(binary), "--project", str(project), "tui"],
            stdin=self.slave, stdout=self.slave, stderr=self.slave,
            cwd=cwd, env=environment, start_new_session=True,
        )

    def feed(self, chunk: bytes) -> str:
        self.pending += self.decoder.decode(chunk)
        while self.pending:
            if self.pending.startswith("\x1b"):
                if len(self.pending) == 1:
                    break
                if self.pending[1] == "[":
                    match = re.match(r"\x1b\[([0-?]*)([ -/]*)([@-~])", self.pending)
                    if match is None:
                        break
                    parameters, _, command = match.groups()
                    values = (
                        [int(value or 0) for value in parameters.split(";")]
                        if not parameters.startswith("?") else []
                    )
                    first = values[0] if values else 0
                    if command in "Hf":
                        self.row = max(0, (first or 1) - 1)
                        column = values[1] if len(values) > 1 else 1
                        self.column = max(0, (column or 1) - 1)
                    elif command == "G":
                        self.column = max(0, (first or 1) - 1)
                    elif command == "d":
                        self.row = max(0, (first or 1) - 1)
                    elif command == "A":
                        self.row = max(0, self.row - (first or 1))
                    elif command == "B":
                        self.row += first or 1
                    elif command == "C":
                        self.column += first or 1
                    elif command == "D":
                        self.column = max(0, self.column - (first or 1))
                    elif command == "J" and first in (2, 3):
                        self.grid = [[" "] * 180 for _ in range(48)]
                    elif command == "K" and self.row < 48:
                        if first == 2:
                            start, end = 0, 180
                        elif first == 1:
                            start, end = 0, self.column + 1
                        else:
                            start, end = self.column, 180
                        for column in range(max(0, start), min(180, end)):
                            self.grid[self.row][column] = " "
                    self.pending = self.pending[match.end():]
                    continue
                self.pending = self.pending[2:]
                continue
            character, self.pending = self.pending[0], self.pending[1:]
            if character == "\r":
                self.column = 0
            elif character == "\n":
                self.row += 1
            elif character == "\b":
                self.column = max(0, self.column - 1)
            elif character >= " ":
                if self.row < 48 and self.column < 180:
                    self.grid[self.row][self.column] = character
                self.column += 1
        return "\n".join("".join(row) for row in self.grid)

    def wait_for(self, message: str) -> None:
        deadline = time.monotonic() + 20
        visible = ""
        while time.monotonic() < deadline:
            if self.process.poll() is not None:
                raise AssertionError(f"TUI exited {self.process.returncode}: {self.trace[-4096:]!r}")
            ready, _, _ = select.select([self.master], [], [], 0.1)
            if ready:
                chunk = os.read(self.master, 65536)
                self.trace.extend(chunk)
                if len(self.trace) > 2 * 1024 * 1024:
                    raise AssertionError("PTY output exceeded smoke bound")
                # Consume cursor movement and differential redraws, not just
                # stripped ANSI bytes: unchanged letters may not be re-emitted.
                visible = self.feed(chunk)
                if re.sub(r"\s+", "", message) in re.sub(r"\s+", "", visible):
                    return
        raise AssertionError(f"Timed out waiting for {message!r}: {visible[-4096:]!r}")

    def key(self, value: bytes = b"\r") -> None:
        os.write(self.master, value)

    def finish(self) -> None:
        self.key(b"\x1b")
        deadline = time.monotonic() + 5
        # Keep draining while the child restores its screen; waiting without
        # reading can itself block the writer on a small PTY buffer.
        while self.process.poll() is None and time.monotonic() < deadline:
            ready, _, _ = select.select([self.master], [], [], 0.1)
            if ready:
                self.feed(os.read(self.master, 65536))
        assert self.process.poll() == 0, "TUI did not exit cleanly"
        assert termios.tcgetattr(self.slave) == self.before, "TUI did not restore terminal mode"

    def close(self) -> None:
        if self.process.poll() is None:
            self.process.kill()
            self.process.wait(timeout=5)
        termios.tcsetattr(self.slave, termios.TCSANOW, self.before)
        os.close(self.master)
        os.close(self.slave)


def main() -> None:
    if len(sys.argv) != 2:
        raise SystemExit(__doc__)
    binary = Path(sys.argv[1]).resolve(strict=True)
    server = ThreadingHTTPServer(("127.0.0.1", 0), Provider)
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    try:
        with tempfile.TemporaryDirectory(prefix="okc-pty-smoke-") as directory:
            root = Path(directory).resolve()
            environment = {
                **os.environ,
                "TERM": "xterm-256color",
                "OKC_PROVIDER_CONFIG": str(root / "providers.toml"),
            }

            def cli(*arguments: str) -> str:
                return subprocess.run(
                    [str(binary), *arguments], cwd=root, env=environment,
                    check=True, text=True, capture_output=True, timeout=20,
                ).stdout

            source = root / "source"
            source.mkdir()
            note = source / "Note.md"
            note.write_text("# PTY fixture\n\nSynthetic evidence retained verbatim.\n", encoding="utf-8")
            before_hash = hashlib.sha256(note.read_bytes()).hexdigest()
            before_mtime = note.stat().st_mtime_ns
            project = root / "test.okc-project"
            cli("provider", "add", "local", "--kind", "ollama", "--endpoint",
                f"http://127.0.0.1:{server.server_port}", "--model", "fixture-model")
            cli("project", "create", str(project), "--name", "PTY fixture", "--curator", "test")
            cli("--project", str(project), "project", "source", "add", "fixture", str(source))
            cli("--project", str(project), "project", "ai-route", "set", "local")
            terminal = Terminal(binary, project, root, environment)
            try:
                terminal.wait_for("Press Enter to inspect immutable sources")
                terminal.key()
                terminal.wait_for("Enter continues.")
                assert Provider.requests == 0, "preflight contacted provider"
                terminal.key()
                terminal.wait_for("A/Enter explicitly approve")
                terminal.key()
                terminal.wait_for("taxonomy approved; cluster revisions generated or resumed")
                terminal.key()
                terminal.wait_for("Approved integration plan is sealed and provider-free.")
                calls = Provider.requests
                assert calls == 4, f"expected embed/organizer/synthesis/critic, got {calls}"
                terminal.key()
                terminal.wait_for("Success is reported only after independent verification.")
                terminal.key()
                terminal.wait_for("independent verification passed")
                terminal.finish()
            finally:
                terminal.close()
            cli("verify", str(root / "IntegratedVault"))
            terminal = Terminal(binary, project, root, environment)
            try:
                terminal.wait_for("Success is reported only after independent verification.")
                terminal.finish()
            finally:
                terminal.close()
            assert Provider.requests == calls, "compile/verify/reopen unexpectedly contacted provider"
            assert hashlib.sha256(note.read_bytes()).hexdigest() == before_hash
            assert note.stat().st_mtime_ns == before_mtime
            print(json.dumps({
                "pty_workflow": "passed", "provider": "synthetic-loopback",
                "provider_calls": calls, "source_unchanged": True,
                "terminal_restored": True, "verified_restart": True,
            }))
    finally:
        server.shutdown()
        server.server_close()
        thread.join(timeout=5)


if __name__ == "__main__":
    main()
