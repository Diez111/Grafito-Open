#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(CDPATH='' cd -- "$(dirname -- "$0")" && pwd)"
PROJECT_ROOT="$(CDPATH='' cd -- "$SCRIPT_DIR/../.." && pwd)"

# Sourcing must expose the import policy without starting a cross-build.
# shellcheck disable=SC1091
source "$PROJECT_ROOT/packaging/build-exe.sh"

for dll in \
    KERNEL32.dll \
    api-ms-win-core-synch-l1-2-0.dll \
    bcryptprimitives.dll \
    uiautomationcore.dll; do
    is_windows_system_dll "$dll" || {
        echo "expected Windows system DLL to be accepted: $dll" >&2
        exit 1
    }
    accept_unresolved_system_import "$dll" >/dev/null
done

unknown_dll="grafito-unresolved-fixture.dll"
if is_windows_system_dll "$unknown_dll"; then
    echo "unknown DLL was classified as a Windows system DLL" >&2
    exit 1
fi
if error="$(accept_unresolved_system_import "$unknown_dll" 2>&1)"; then
    echo "unknown unresolved DLL was accepted" >&2
    exit 1
fi
[[ "$error" == *"unresolved non-system PE import: $unknown_dll"* ]]

assert_static_crt_workflow() {
    local workflow="$1"

    awk '
        /^  build:/ { in_build = 1; next }
        in_build && /^  [[:alnum:]_-]+:/ { exit }
        in_build && /CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_RUSTFLAGS: -C target-feature=\+crt-static/ {
            configured = 1
        }
        END { exit !configured }
    ' "$workflow" || {
        echo "MSVC static CRT is not configured in the build job: $workflow" >&2
        exit 1
    }

    grep -Fq 'packaging/verify-msvc-runtime.ps1' "$workflow" || {
        echo "workflow does not run the MSVC runtime verifier: $workflow" >&2
        exit 1
    }
}

assert_static_crt_workflow "$PROJECT_ROOT/.github/workflows/release.yml"
assert_static_crt_workflow "$PROJECT_ROOT/.github/workflows/build-windows.yml"

grep -Eqi 'VCRUNTIME|ucrtbase|dynamic CRT' "$PROJECT_ROOT/packaging/verify-msvc-runtime.ps1" || {
    echo "MSVC runtime verifier does not reject dynamic CRT imports" >&2
    exit 1
}

ci_workflow="$PROJECT_ROOT/.github/workflows/ci.yml"
postrm_mentions="$(grep -c 'packaging/debian/postrm' "$ci_workflow" || true)"
(( postrm_mentions >= 2 )) || {
    echo "CI does not parse and ShellCheck the postrm hook" >&2
    exit 1
}
grep -Fq '\./postrm$' "$ci_workflow" || {
    echo "CI does not assert the packaged postrm hook" >&2
    exit 1
}
grep -Fq 'bash packaging/tests/packaging-fixtures.sh' "$ci_workflow" || {
    echo "CI does not execute packaging fixtures" >&2
    exit 1
}
grep -Fq 'cargo metadata --locked --format-version 1' "$ci_workflow" || {
    echo "CI does not parse the full lockfile with the declared MSRV" >&2
    exit 1
}
grep -Fq 'cargo check -p grafito-app --target x86_64-pc-windows-gnu --all-features --locked' "$ci_workflow" || {
    echo "CI does not check the Windows GNU app with the declared MSRV" >&2
    exit 1
}

grep -Fq 'rust-version = "1.81"' "$PROJECT_ROOT/Cargo.toml" || {
    echo "workspace rust-version is not the verified 1.81 minimum" >&2
    exit 1
}
for msrv_file in \
    "$PROJECT_ROOT/.github/workflows/ci.yml" \
    "$PROJECT_ROOT/.github/CONTRIBUTING.md" \
    "$PROJECT_ROOT/AGENTS.md" \
    "$PROJECT_ROOT/README.md" \
    "$PROJECT_ROOT/README.en.md" \
    "$PROJECT_ROOT/packaging/README.md"; do
    if grep -Fq '1.78' "$msrv_file" || ! grep -Fq '1.81' "$msrv_file"; then
        echo "stale or missing Rust 1.81 MSRV documentation: $msrv_file" >&2
        exit 1
    fi
done

echo "Packaging fixtures passed."
