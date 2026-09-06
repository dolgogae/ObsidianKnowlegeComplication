"""Public Python API for the Obsidian Knowledge Compiler."""

from __future__ import annotations

import json
import os
from dataclasses import dataclass, field
from types import MappingProxyType
from typing import Any, Callable, Generic, Mapping, Sequence, TypeVar, TypedDict, cast

from . import _native

__all__ = [
    "INTEROP_SCHEMA_VERSION",
    "Job",
    "OkcClient",
    "OkcError",
    "Project",
    "ProviderProfile",
    "SourceInput",
    "VerificationResult",
    "ExplanationResult",
]

INTEROP_SCHEMA_VERSION = _native.interop_schema_version()
_T = TypeVar("_T")


class VerificationResult(TypedDict):
    interop_schema_version: int
    valid: bool
    artifact_path: str
    manifest: dict[str, Any]


class ExplanationResult(TypedDict):
    interop_schema_version: int
    artifact_path: str
    record: dict[str, Any]


def _path(value: os.PathLike[str] | str) -> str:
    return os.fsdecode(os.fspath(value))


def _error_from_exception(error: BaseException) -> "OkcError":
    try:
        payload = json.loads(str(error))
        if not isinstance(payload, dict):
            raise ValueError("error payload is not an object")
        return OkcError(
            code=str(payload["code"]),
            category=str(payload["category"]),
            message=str(payload["message"]),
            retryable=bool(payload["retryable"]),
            details=cast(dict[str, Any], payload.get("details", {})),
        )
    except (KeyError, TypeError, ValueError, json.JSONDecodeError):
        return OkcError(
            code="INTERNAL",
            category="internal",
            message="native binding returned an invalid structured error",
            retryable=False,
            details={},
        )


def _native_call(call: Callable[[], _T]) -> _T:
    try:
        return call()
    except RuntimeError as error:
        raise _error_from_exception(error) from None


class OkcError(RuntimeError):
    """Stable structured error; callers never need to parse ``message``."""

    def __init__(
        self,
        *,
        code: str,
        category: str,
        message: str,
        retryable: bool,
        details: Mapping[str, Any],
    ) -> None:
        super().__init__(message)
        self.code = code
        self.category = category
        self.message = message
        self.retryable = retryable
        self.details = MappingProxyType(dict(details))


@dataclass(frozen=True, slots=True)
class ProviderProfile:
    """Immutable provider configuration containing only a secret reference."""

    name: str
    kind: str
    endpoint: str
    model: str
    api_key_env: str | None = None
    timeout_ms: int = 120_000
    max_response_bytes: int = 16 * 1024 * 1024
    max_input_bytes: int = 64 * 1024 * 1024
    max_batch_items: int = 2_048
    options: Mapping[str, Any] = field(default_factory=dict)

    def __post_init__(self) -> None:
        object.__setattr__(self, "options", MappingProxyType(dict(self.options)))

    def _json_value(self) -> dict[str, Any]:
        return {
            "name": self.name,
            "kind": self.kind,
            "endpoint": self.endpoint,
            "model": self.model,
            "api_key_env": self.api_key_env,
            "timeout_ms": self.timeout_ms,
            "max_response_bytes": self.max_response_bytes,
            "max_input_bytes": self.max_input_bytes,
            "max_batch_items": self.max_batch_items,
            "options": dict(self.options),
        }


@dataclass(frozen=True, slots=True)
class SourceInput:
    source_id: str
    path: os.PathLike[str] | str
    owner_display_name: str | None = None
    snapshot_id: str | None = None

    def _json_value(self) -> dict[str, Any]:
        return {
            "source_id": self.source_id,
            "path": _path(self.path),
            "owner_display_name": self.owner_display_name,
            "snapshot_id": self.snapshot_id,
        }


class Job(Generic[_T]):
    """A cancellable bounded Rust job with a retained terminal result."""

    def __init__(
        self,
        native: _native.NativeJob,
        mapper: Callable[[Any], _T] = cast(Callable[[Any], _T], lambda value: value),
    ) -> None:
        self._native = native
        self._mapper = mapper

    @property
    def state(self) -> str:
        return cast(str, json.loads(_native_call(self._native.state)))

    def events(self) -> list[dict[str, Any]]:
        return cast(list[dict[str, Any]], json.loads(_native_call(self._native.events_json)))

    def result(self) -> _T:
        payload = json.loads(_native_call(self._native.result_json))
        return self._mapper(payload)

    def cancel(self) -> str:
        return cast(str, json.loads(_native_call(self._native.cancel)))


class OkcClient:
    """Client with immutable provider profiles and a bounded native scheduler."""

    def __init__(
        self,
        provider_profiles: Sequence[ProviderProfile] = (),
        *,
        max_concurrent_jobs: int = 4,
    ) -> None:
        self.provider_profiles = tuple(provider_profiles)
        encoded = json.dumps(
            [profile._json_value() for profile in self.provider_profiles],
            separators=(",", ":"),
        )
        self._native = _native_call(
            lambda: _native.NativeClient(encoded, max_concurrent_jobs)
        )

    def api_info(self) -> dict[str, Any]:
        return cast(dict[str, Any], json.loads(_native_call(self._native.api_info_json)))

    def create_project(
        self,
        path: os.PathLike[str] | str,
        *,
        name: str,
        curator_id: str,
        policy_version: str = "policy-v3",
        language: str | None = None,
    ) -> Job[Project]:
        native = self._native.create_project(
            _path(path), name, curator_id, policy_version, language
        )
        return Job(native, self._project_result)

    def open_project(self, path: os.PathLike[str] | str) -> Job[Project]:
        return Job(self._native.open_project(_path(path)), self._project_result)

    def test_provider(self, name: str) -> Job[dict[str, Any]]:
        return Job(self._native.test_provider(name))

    def verify_artifact(self, path: os.PathLike[str] | str) -> Job[VerificationResult]:
        return Job(self._native.verify_artifact(_path(path)))

    def explain_artifact(
        self,
        path: os.PathLike[str] | str,
        *,
        output_path: str,
    ) -> Job[ExplanationResult]:
        return Job(self._native.explain_artifact(_path(path), output_path))

    def _project_result(self, payload: Any) -> Project:
        if not isinstance(payload, dict) or payload.get("result_type") != "project":
            raise OkcError(
                code="INTERNAL",
                category="internal",
                message="project job returned an invalid descriptor",
                retryable=False,
                details={},
            )
        native = _native_call(lambda: self._native.project_handle(str(payload["path"])))
        return Project(self, native)


class Project:
    def __init__(self, client: OkcClient, native: _native.NativeProject) -> None:
        self._client = client
        self._native = native

    @property
    def path(self) -> str:
        return self._native.path()

    def manifest(self) -> Job[dict[str, Any]]:
        return Job(self._native.manifest())

    def status(self) -> Job[dict[str, Any]]:
        return Job(self._native.status())

    def add_source(self, source: SourceInput) -> Job[dict[str, Any]]:
        return Job(
            _native_call(
                lambda: self._native.add_source(
                    json.dumps(source._json_value(), separators=(",", ":"))
                )
            )
        )

    def rebind_source(
        self,
        source_id: str,
        path: os.PathLike[str] | str,
        *,
        snapshot_id: str | None = None,
    ) -> Job[dict[str, Any]]:
        return Job(self._native.rebind_source(source_id, _path(path), snapshot_id))

    def replace_sources(self, sources: Sequence[SourceInput]) -> Job[dict[str, Any]]:
        encoded = json.dumps(
            [source._json_value() for source in sources], separators=(",", ":")
        )
        return Job(_native_call(lambda: self._native.replace_sources(encoded)))

    def set_language(self, language: str | None) -> Job[dict[str, Any]]:
        return Job(self._native.set_language(language))

    def set_ai_route(
        self, profile_name: str, *, role: str | None = None
    ) -> Job[dict[str, Any]]:
        return Job(
            _native_call(lambda: self._native.set_ai_route(role, profile_name))
        )

    def preflight(self) -> Job[dict[str, Any]]:
        return Job(self._native.preflight())

    def integrate(
        self,
        *,
        allow_remote_provider: bool,
        remote_disclosure_confirmed: bool,
    ) -> Job[dict[str, Any]]:
        return Job(
            self._native.integrate(
                allow_remote_provider, remote_disclosure_confirmed
            )
        )

    def taxonomy(self) -> Job[dict[str, Any]]:
        return Job(self._native.taxonomy())

    def approve_taxonomy(
        self,
        *,
        edited_clusters: Sequence[Mapping[str, Any]] | None = None,
        rationale: str | None = None,
    ) -> Job[dict[str, Any]]:
        encoded = (
            None
            if edited_clusters is None
            else json.dumps(list(edited_clusters), separators=(",", ":"))
        )
        return Job(
            _native_call(lambda: self._native.approve_taxonomy(encoded, rationale))
        )

    def clusters(self) -> Job[dict[str, Any]]:
        return Job(self._native.clusters())

    def approve_cluster(
        self,
        cluster_id: str,
        *,
        omission_rationales: Mapping[str, str] | None = None,
        minor_waivers: Mapping[str, str] | None = None,
    ) -> Job[dict[str, Any]]:
        return Job(
            _native_call(
                lambda: self._native.approve_cluster(
                    cluster_id,
                    json.dumps(dict(omission_rationales or {}), separators=(",", ":")),
                    json.dumps(dict(minor_waivers or {}), separators=(",", ":")),
                )
            )
        )

    def regenerate_cluster(
        self,
        cluster_id: str,
        feedback: str,
        *,
        allow_remote_provider: bool,
        remote_disclosure_confirmed: bool,
    ) -> Job[dict[str, Any]]:
        return Job(
            self._native.regenerate_cluster(
                cluster_id,
                feedback,
                allow_remote_provider,
                remote_disclosure_confirmed,
            )
        )

    def compile(self, output: os.PathLike[str] | str) -> Job[dict[str, Any]]:
        return Job(self._native.compile(_path(output)))
