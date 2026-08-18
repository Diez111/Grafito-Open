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

# --- Grafito logo must ship in the Debian package ---
build_deb="$PROJECT_ROOT/packaging/build-deb.sh"

grep -Fq 'hicolor/scalable/apps' "$build_deb" || {
    echo "build-deb.sh does not stage the scalable icon directory" >&2
    exit 1
}
grep -Fq 'grafito-icon.svg' "$build_deb" || {
    echo "build-deb.sh does not install the scalable Grafito logo" >&2
    exit 1
}
grep -Fq 'ERROR: missing icon asset' "$build_deb" || {
    echo "build-deb.sh does not abort when an icon asset is missing" >&2
    exit 1
}

# Every declared PNG size must exist as an asset and match the deb staging.
for icon_size in 16 32 48 64 128 256 512; do
    [[ -f "$PROJECT_ROOT/assets/grafito-icon-${icon_size}x${icon_size}.png" ]] || {
        echo "missing icon asset grafito-icon-${icon_size}x${icon_size}.png" >&2
        exit 1
    }
    grep -Fq "grafito-icon-${icon_size}x${icon_size}.png" "$build_deb" || {
        echo "build-deb.sh does not stage the ${icon_size}x${icon_size} icon" >&2
        exit 1
    }
done
[[ -f "$PROJECT_ROOT/assets/grafito-icon.svg" ]] || {
    echo "missing scalable asset grafito-icon.svg" >&2
    exit 1
}

desktop_icon="$(sed -n 's/^Icon=//p' "$PROJECT_ROOT/packaging/debian/grafito.desktop" | head -1)"
[[ -n "$desktop_icon" && "$desktop_icon" == "grafito" ]] || {
    echo "desktop entry must reference the grafito icon name" >&2
    exit 1
}
grep -Fq 'Icon=' "$PROJECT_ROOT/packaging/debian/grafito.desktop"

# Default assistant plugins must ship in the package (e.g. j-space).
grep -Fq 'usr/share/grafito/plugins' "$build_deb" || {
    echo "build-deb.sh does not stage default assistant plugins" >&2
    exit 1
}
[[ -f "$PROJECT_ROOT/plugins/j-space/grafito-plugin.toml" ]] || {
    echo "the default j-space plugin is missing" >&2
    exit 1
}

echo "Packaging fixtures passed."
