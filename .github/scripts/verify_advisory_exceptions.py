#!/usr/bin/env python3
"""Fail-closed policy for Grafito's temporary RustSec exceptions."""

from __future__ import annotations

import argparse
import json
import os
import re
import subprocess
import sys
import tempfile
from collections import Counter, defaultdict, deque
from collections.abc import Mapping, Sequence, Set as AbstractSet
from datetime import date
from pathlib import Path


EXPECTED_ADVISORY_IGNORES = (
    "RUSTSEC-2025-0141",
    "RUSTSEC-2024-0436",
    "RUSTSEC-2026-0192",
    "RUSTSEC-2025-0165",
    "RUSTSEC-2026-0194",
    "RUSTSEC-2026-0195",
)
EXCEPTION_EXPIRES = date(2026, 9, 30)
PATCHED_QUICK_XML_VERSION = "0.41.0"
CRATES_IO_SOURCE = "registry+https://github.com/rust-lang/crates.io-index"
FORBIDDEN_WORKSPACE_DEPENDENCIES = {
    "quick-xml",
    "wayland-scanner",
    "zbus-lockstep",
    "zbus-lockstep-macros",
    "zbus_xml",
}
REQUIRED_PROC_MACROS = {
    "zbus-lockstep-macros@0.4.4",
}
POLICY_SCRIPT = ".github/scripts/verify_advisory_exceptions.py"
CHECK_INVOCATION = f"python3 {POLICY_SCRIPT} check"
AUDIT_INVOCATION = f"python3 {POLICY_SCRIPT} audit"
DOC_GRAPH_START = "<!-- reviewed-quick-xml-ancestor-edges:start -->"
DOC_GRAPH_END = "<!-- reviewed-quick-xml-ancestor-edges:end -->"

_IGNORE_RE = re.compile(
    r"(?<![\w-])--ignore(?:=|[ \t]+)(RUSTSEC-\d{4}-\d{4})\b"
)
_DIRECT_AUDIT_RE = re.compile(r"\bcargo[ \t]+audit\b")
_ADVISORY_BLOCK_RE = re.compile(
    r"^\[advisories\]\s*(.*?)(?=^\[[^]]+\]|\Z)", re.MULTILINE | re.DOTALL
)
_DENY_ID_RE = re.compile(r'\bid\s*=\s*"(RUSTSEC-\d{4}-\d{4})"')
_SEMVER_RE = re.compile(
    r"(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)"
    r"(?:-([0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*))?"
    r"(?:\+([0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*))?"
)
_MAX_U64 = (1 << 64) - 1

_PrereleaseIdentifierKey = tuple[int, int, str] | tuple[int, str]
_SemVerKey = tuple[int, int, int, int, tuple[_PrereleaseIdentifierKey, ...]]


class PolicyError(RuntimeError):
    pass


def _edge_set(text: str) -> frozenset[tuple[str, str]]:
    edges = set()
    for raw_line in text.strip().splitlines():
        parent, separator, child = raw_line.strip().partition(" -> ")
        if not separator or not parent or not child:
            raise ValueError(f"invalid expected dependency edge: {raw_line!r}")
        edges.add((parent, child))
    return frozenset(edges)


EXPECTED_ANCESTOR_EDGES = {
    "0.30.0": _edge_set(
        """
        accesskit_atspi_common@0.9.3 -> atspi-common@0.6.0
        accesskit_unix@0.12.3 -> accesskit_atspi_common@0.9.3
        accesskit_unix@0.12.3 -> atspi@0.22.0
        accesskit_winit@0.22.4 -> accesskit_unix@0.12.3
        atspi-common@0.6.0 -> zbus-lockstep-macros@0.4.4
        atspi-common@0.6.0 -> zbus-lockstep@0.4.4
        atspi-connection@0.6.0 -> atspi-common@0.6.0
        atspi-connection@0.6.0 -> atspi-proxies@0.6.0
        atspi-proxies@0.6.0 -> atspi-common@0.6.0
        atspi@0.22.0 -> atspi-common@0.6.0
        atspi@0.22.0 -> atspi-connection@0.6.0
        atspi@0.22.0 -> atspi-proxies@0.6.0
        eframe@0.29.1 -> egui_glow@0.29.1
        eframe@0.29.1 -> egui-winit@0.29.1
        egui_glow@0.29.1 -> egui-winit@0.29.1
        egui-winit@0.29.1 -> accesskit_winit@0.22.4
        workspace:grafito-app -> eframe@0.29.1
        zbus-lockstep-macros@0.4.4 -> zbus-lockstep@0.4.4
        zbus-lockstep-macros@0.4.4 -> zbus_xml@4.0.0
        zbus-lockstep@0.4.4 -> zbus_xml@4.0.0
        zbus_xml@4.0.0 -> quick-xml@0.30.0
        """
    ),
}


def extract_audit_ignore_ids(text: str) -> list[str]:
    return _IGNORE_RE.findall(text)


def assert_exact_ids(actual: Sequence[str], source: str) -> None:
    actual_list = list(actual)
    expected = list(EXPECTED_ADVISORY_IGNORES)
    duplicates = sorted(
        identifier for identifier, count in Counter(actual_list).items() if count > 1
    )
    missing = sorted(set(expected) - set(actual_list))
    extra = sorted(set(actual_list) - set(expected))
    if duplicates or missing or extra or actual_list != expected:
        raise PolicyError(
            f"{source} ignore IDs must be exactly {expected}; "
            f"got {actual_list} (duplicates={duplicates}, missing={missing}, extra={extra})"
        )


def build_audit_command(lockfile: str | Path = "Cargo.lock") -> list[str]:
    command = ["cargo", "audit", "--file", str(lockfile), "--deny", "warnings"]
    for identifier in EXPECTED_ADVISORY_IGNORES:
        command.extend(("--ignore", identifier))
    return command


def metadata_command() -> list[str]:
    return ["cargo", "metadata", "--locked", "--all-features", "--format-version", "1"]


def validate_workflow_text(text: str) -> None:
    direct_ids = extract_audit_ignore_ids(text)
    if direct_ids:
        raise PolicyError(
            "workflow files must not pass cargo-audit ignores directly; "
            f"found {direct_ids}"
        )
    if _DIRECT_AUDIT_RE.search(text):
        raise PolicyError(
            f"workflow files must invoke cargo audit only through {POLICY_SCRIPT}"
        )
    for invocation in (CHECK_INVOCATION, AUDIT_INVOCATION):
        count = text.count(invocation)
        if count != 1:
            raise PolicyError(
                f"workflow must contain exactly one {invocation!r} invocation; found {count}"
            )


def parse_deny_ids(text: str) -> list[str]:
    advisory_block = _ADVISORY_BLOCK_RE.search(text)
    if advisory_block is None:
        raise PolicyError("deny.toml has no [advisories] section")
    return _DENY_ID_RE.findall(advisory_block.group(1))


def find_forbidden_workspace_declarations(metadata: dict) -> list[str]:
    workspace_ids = set(metadata["workspace_members"])
    violations = []
    for package in metadata["packages"]:
        if package["id"] not in workspace_ids:
            continue
        for dependency in package.get("dependencies", []):
            if dependency["name"] not in FORBIDDEN_WORKSPACE_DEPENDENCIES:
                continue
            kind = dependency.get("kind") or "normal"
            target = dependency.get("target") or "*"
            violations.append(
                f"{package['name']} declares {dependency['name']} "
                f"(optional={dependency.get('optional', False)}, kind={kind}, target={target})"
            )
    return sorted(violations)


def _semver_key(version: str) -> _SemVerKey:
    if not isinstance(version, str):
        raise PolicyError(f"invalid Cargo package SemVer: {version!r}")
    match = _SEMVER_RE.fullmatch(version)
    if match is None:
        raise PolicyError(f"invalid Cargo package SemVer: {version!r}")

    try:
        core = tuple(int(match.group(index)) for index in range(1, 4))
    except ValueError as error:
        raise PolicyError(f"invalid Cargo package SemVer: {version!r}") from error
    if any(component > _MAX_U64 for component in core):
        raise PolicyError(f"Cargo package SemVer component exceeds u64: {version!r}")

    prerelease = match.group(4)
    if prerelease is None:
        return core[0], core[1], core[2], 1, ()

    identifiers = []
    for identifier in prerelease.split("."):
        if identifier.isdigit():
            if len(identifier) > 1 and identifier.startswith("0"):
                raise PolicyError(
                    f"numeric SemVer prerelease identifier has a leading zero: {version!r}"
                )
            identifiers.append((0, len(identifier), identifier))
        else:
            identifiers.append((1, identifier))
    return core[0], core[1], core[2], 0, tuple(identifiers)


_PATCHED_QUICK_XML_KEY = _semver_key(PATCHED_QUICK_XML_VERSION)


def _package_label(package: dict, workspace_ids: set[str]) -> str:
    if package["id"] in workspace_ids:
        return f"workspace:{package['name']}"
    return f"{package['name']}@{package['version']}"


def _resolved_ancestor_graph(metadata: dict, version: str) -> tuple[set[tuple[str, str]], set[str]]:
    packages = {package["id"]: package for package in metadata["packages"]}
    workspace_ids = set(metadata["workspace_members"])
    targets = [
        package["id"]
        for package in metadata["packages"]
        if package["name"] == "quick-xml" and package["version"] == version
    ]
    if len(targets) != 1:
        raise PolicyError(f"expected one quick-xml {version} package, found {len(targets)}")

    reverse = defaultdict(set)
    for node in metadata["resolve"]["nodes"]:
        for dependency in node["deps"]:
            reverse[dependency["pkg"]].add(node["id"])

    target = targets[0]
    ancestors = {target}
    edges = set()
    queue = deque([target])
    while queue:
        child_id = queue.popleft()
        for parent_id in reverse[child_id]:
            if parent_id not in packages or child_id not in packages:
                raise PolicyError("cargo metadata resolve graph references an unknown package")
            edges.add(
                (
                    _package_label(packages[parent_id], workspace_ids),
                    _package_label(packages[child_id], workspace_ids),
                )
            )
            if parent_id not in ancestors:
                ancestors.add(parent_id)
                queue.append(parent_id)
    return edges, ancestors


def assert_expected_ancestor_edges(
    metadata: dict,
    expected: Mapping[str, AbstractSet[tuple[str, str]]],
    *,
    enforce_registry: bool = False,
    required_proc_macros: AbstractSet[str] = frozenset(),
) -> None:
    vulnerable = defaultdict(list)
    for package in metadata["packages"]:
        # The audit ignores are global, so this check must catch every version
        # below the exact final patched release, including boundary prereleases.
        if (
            package["name"] == "quick-xml"
            and _semver_key(package["version"]) < _PATCHED_QUICK_XML_KEY
        ):
            vulnerable[package["version"]].append(package["id"])
    actual_versions = set(vulnerable)
    expected_versions = set(expected)
    duplicates = sorted(version for version, package_ids in vulnerable.items() if len(package_ids) != 1)
    if actual_versions != expected_versions or duplicates:
        raise PolicyError(
            "vulnerable quick-xml versions changed: "
            f"actual={sorted(actual_versions)}, expected={sorted(expected_versions)}, "
            f"duplicate_versions={duplicates}"
        )

    packages = {package["id"]: package for package in metadata["packages"]}
    workspace_ids = set(metadata["workspace_members"])
    reviewed_ancestors = set()
    for version, expected_edges in expected.items():
        actual_edges, ancestors = _resolved_ancestor_graph(metadata, version)
        if actual_edges != set(expected_edges):
            missing = sorted(set(expected_edges) - actual_edges)
            extra = sorted(actual_edges - set(expected_edges))
            raise PolicyError(
                f"quick-xml {version} ancestor graph changed: missing={missing}, extra={extra}"
            )
        reviewed_ancestors.update(ancestors)

    labels = defaultdict(list)
    for package_id in reviewed_ancestors:
        package = packages[package_id]
        labels[_package_label(package, workspace_ids)].append(package_id)
        if enforce_registry and package_id not in workspace_ids and package.get("source") != CRATES_IO_SOURCE:
            raise PolicyError(
                f"reviewed ancestor {_package_label(package, workspace_ids)} is not from crates.io: "
                f"{package.get('source')!r}"
            )
    duplicate_labels = sorted(label for label, package_ids in labels.items() if len(package_ids) > 1)
    if duplicate_labels:
        raise PolicyError(f"reviewed ancestor labels are ambiguous: {duplicate_labels}")

    for label in required_proc_macros:
        package_ids = labels.get(label, [])
        if len(package_ids) != 1:
            raise PolicyError(f"required proc macro {label} is absent from reviewed ancestors")
        targets = packages[package_ids[0]].get("targets", [])
        kinds = {kind for target in targets for kind in target.get("kind", [])}
        if "proc-macro" not in kinds:
            raise PolicyError(f"reviewed compile-time package {label} is no longer a proc macro")


def _expected_documented_graph() -> str:
    lines = ["```text"]
    for version in sorted(EXPECTED_ANCESTOR_EDGES, key=_semver_key):
        lines.append(f"quick-xml {version}:")
        lines.extend(f"{parent} -> {child}" for parent, child in sorted(EXPECTED_ANCESTOR_EDGES[version]))
    lines.append("```")
    return "\n".join(lines)


def validate_documented_graph(text: str) -> None:
    start = text.find(DOC_GRAPH_START)
    end = text.find(DOC_GRAPH_END)
    if start == -1 or end == -1 or end <= start:
        raise PolicyError("SECURITY.md is missing the reviewed quick-xml graph markers")
    actual = text[start + len(DOC_GRAPH_START) : end].strip()
    expected = _expected_documented_graph()
    if actual != expected:
        raise PolicyError("SECURITY.md reviewed quick-xml ancestor graph is stale")


def load_metadata(repo_root: Path) -> dict:
    return json.loads(
        subprocess.check_output(metadata_command(), cwd=repo_root, text=True)
    )


def validate_policy(repo_root: Path, *, today: date | None = None) -> None:
    current_date = today or date.today()
    if current_date >= EXCEPTION_EXPIRES:
        raise PolicyError(f"quick-xml advisory exception expired on {EXCEPTION_EXPIRES}")

    assert_exact_ids(
        extract_audit_ignore_ids(
            " ".join(build_audit_command(repo_root / "Cargo.lock"))
        ),
        "generated cargo-audit command",
    )
    deny_ids = parse_deny_ids((repo_root / "deny.toml").read_text(encoding="utf-8"))
    assert_exact_ids(deny_ids, "cargo deny")

    workflow_text = "\n".join(
        path.read_text(encoding="utf-8")
        for pattern in ("*.yml", "*.yaml")
        for path in sorted((repo_root / ".github" / "workflows").glob(pattern))
    )
    validate_workflow_text(workflow_text)

    metadata = load_metadata(repo_root)
    declaration_violations = find_forbidden_workspace_declarations(metadata)
    if declaration_violations:
        raise PolicyError(
            "workspace manifests declare reviewed XML dependencies: "
            + "; ".join(declaration_violations)
        )
    assert_expected_ancestor_edges(
        metadata,
        EXPECTED_ANCESTOR_EDGES,
        enforce_registry=True,
        required_proc_macros=REQUIRED_PROC_MACROS,
    )
    validate_documented_graph(
        (repo_root / ".github" / "SECURITY.md").read_text(encoding="utf-8")
    )


def run_audit(repo_root: Path) -> None:
    validate_policy(repo_root)
    with tempfile.TemporaryDirectory(
        prefix="grafito-cargo-audit-", dir=os.environ.get("RUNNER_TEMP")
    ) as isolated_home:
        environment = os.environ.copy()
        environment["CARGO_HOME"] = isolated_home
        subprocess.run(
            build_audit_command(repo_root / "Cargo.lock"),
            cwd=isolated_home,
            env=environment,
            check=True,
        )


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=("check", "audit"))
    args = parser.parse_args(argv)
    repo_root = Path(__file__).resolve().parents[2]
    try:
        if args.command == "check":
            validate_policy(repo_root)
            print("advisory exceptions, expiry, declarations, and ancestor graphs are exact")
        else:
            run_audit(repo_root)
    except (PolicyError, subprocess.CalledProcessError) as error:
        print(f"advisory exception policy failed: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
