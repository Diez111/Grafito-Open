#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(CDPATH='' cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(CDPATH='' cd -- "$SCRIPT_DIR/.." && pwd)"
TARGET="x86_64-pc-windows-gnu"
MINGW_PREFIX="${MINGW_PREFIX:-x86_64-w64-mingw32}"
MINGW_CC="${MINGW_CC:-${MINGW_PREFIX}-gcc}"
MINGW_AR="${MINGW_AR:-${MINGW_PREFIX}-ar}"
MINGW_WINDRES="${MINGW_WINDRES:-${MINGW_PREFIX}-windres}"
OBJDUMP="${OBJDUMP:-${MINGW_PREFIX}-objdump}"
OUTPUT_DIR="$ROOT_DIR/target/$TARGET/release"
EXE="$OUTPUT_DIR/grafito.exe"
IMPORT_REPORT="$OUTPUT_DIR/grafito-pe-imports.txt"

is_windows_system_dll() {
    case "${1,,}" in
        api-ms-win-*.dll|ext-ms-win-*.dll)
            return 0
            ;;
        advapi32.dll|bcrypt.dll|bcryptprimitives.dll|cfgmgr32.dll|comctl32.dll|comdlg32.dll|crypt32.dll)
            return 0
            ;;
        d2d1.dll|d3d11.dll|d3d12.dll|d3dcompiler_47.dll|dbghelp.dll|dnsapi.dll|dwmapi.dll|dwrite.dll|dxgi.dll)
            return 0
            ;;
        gdi32.dll|hid.dll|imm32.dll|iphlpapi.dll|kernel32.dll|kernelbase.dll|mpr.dll|msvcp_win.dll|msvcrt.dll)
            return 0
            ;;
        ncrypt.dll|ntdll.dll|ole32.dll|oleaut32.dll|opengl32.dll|powrprof.dll|propsys.dll|rpcrt4.dll|secur32.dll)
            return 0
            ;;
        setupapi.dll|shell32.dll|shlwapi.dll|ucrtbase.dll|uiautomationcore.dll|user32.dll|userenv.dll|usp10.dll)
            return 0
            ;;
        uxtheme.dll|version.dll|vulkan-1.dll|winhttp.dll|wininet.dll|winmm.dll|winspool.drv|wintrust.dll|wldap32.dll)
            return 0
            ;;
        ws2_32.dll|wtsapi32.dll)
            return 0
            ;;
        *)
            return 1
            ;;
    esac
}

accept_unresolved_system_import() {
    local dll="$1"

    if is_windows_system_dll "$dll"; then
        return 0
    fi

    echo "ERROR: unresolved non-system PE import: $dll" >&2
    echo "       Install its runtime DLL or make the dependency link statically." >&2
    return 1
}

resolve_mingw_runtime_dll() {
    local dll="$1"
    local lookup
    local candidate

    for lookup in "$dll" "${dll,,}"; do
        candidate="$("$MINGW_CC" -print-file-name="$lookup")"
        if [[ "$candidate" != "$lookup" && -f "$candidate" ]]; then
            printf '%s\n' "$candidate"
            return 0
        fi
    done
    return 1
}

inspect_and_bundle_imports() {
    local -a queue=("$EXE")
    local -a bundled=()
    local -A seen_imports=()
    local index=0
    local pe
    local pe_headers
    local section_headers
    local line
    local dll
    local dll_key
    local runtime_path
    local destination

    pe_headers="$("$OBJDUMP" -p "$EXE")"
    [[ "$pe_headers" == *"(Windows GUI)"* ]] || {
        echo "ERROR: $EXE is not a Windows GUI-subsystem executable." >&2
        return 1
    }
    section_headers="$("$OBJDUMP" -h "$EXE")"
    [[ "$section_headers" == *".rsrc"* ]] || {
        echo "ERROR: $EXE has no embedded Windows resource section." >&2
        return 1
    }

    : > "$IMPORT_REPORT"
    while (( index < ${#queue[@]} )); do
        pe="${queue[$index]}"
        index=$((index + 1))
        pe_headers="$("$OBJDUMP" -p "$pe")"

        while IFS= read -r line; do
            if [[ "$line" =~ DLL[[:space:]]+Name:[[:space:]]*(.+)$ ]]; then
                dll="${BASH_REMATCH[1]}"
                dll="${dll%$'\r'}"
                dll="${dll%"${dll##*[![:space:]]}"}"
                dll_key="${dll,,}"
                printf '%s -> %s\n' "$(basename -- "$pe")" "$dll" >> "$IMPORT_REPORT"

                [[ -z "${seen_imports[$dll_key]:-}" ]] || continue
                seen_imports[$dll_key]=1

                if runtime_path="$(resolve_mingw_runtime_dll "$dll")"; then
                    destination="$OUTPUT_DIR/$(basename -- "$runtime_path")"
                    cp -f -- "$runtime_path" "$destination"
                    queue+=("$destination")
                    bundled+=("$(basename -- "$destination")")
                elif ! accept_unresolved_system_import "$dll"; then
                    return 1
                fi
            fi
        done <<< "$pe_headers"
    done

    (( ${#seen_imports[@]} > 0 )) || {
        echo "ERROR: no PE imports were found in $EXE." >&2
        return 1
    }

    echo "PE import report: $IMPORT_REPORT"
    if (( ${#bundled[@]} > 0 )); then
        echo "Bundled MinGW runtime DLLs: ${bundled[*]}"
    else
        echo "No MinGW runtime DLLs are dynamically imported."
    fi
}

run_wine_smoke() {
    local wine_cmd="${WINE:-}"
    local temporary_prefix=""
    local -a wine_environment=("env" "WINEDEBUG=-all" "WINEDLLOVERRIDES=mscoree,mshtml=")

    if [[ -z "$wine_cmd" ]]; then
        if command -v wine >/dev/null 2>&1; then
            wine_cmd="wine"
        elif command -v wine64 >/dev/null 2>&1; then
            wine_cmd="wine64"
        else
            echo "Wine not found; skipping the optional Windows loader smoke test."
            return 0
        fi
    fi

    command -v "$wine_cmd" >/dev/null 2>&1 || {
        echo "ERROR: configured Wine command not found: $wine_cmd" >&2
        return 1
    }

    if [[ -z "${WINEPREFIX:-}" ]]; then
        temporary_prefix="$(mktemp -d "${TMPDIR:-/tmp}/grafito-wine.XXXXXX")"
        wine_environment+=(WINEARCH=win64 "WINEPREFIX=$temporary_prefix")
    fi

    cleanup_wine_prefix() {
        if [[ -n "$temporary_prefix" ]]; then
            if command -v wineserver >/dev/null 2>&1; then
                WINEPREFIX="$temporary_prefix" wineserver -k >/dev/null 2>&1 || true
                WINEPREFIX="$temporary_prefix" wineserver -w >/dev/null 2>&1 || true
            fi
            rm -rf -- "$temporary_prefix"
        fi
    }
    trap cleanup_wine_prefix EXIT

    echo "Running Wine --help smoke test..."
    if command -v timeout >/dev/null 2>&1; then
        timeout 120s "${wine_environment[@]}" "$wine_cmd" "$EXE" --help
    else
        "${wine_environment[@]}" "$wine_cmd" "$EXE" --help
    fi
    cleanup_wine_prefix
    trap - EXIT
    echo "Wine --help smoke test passed."
}

main() {
    echo "Building Grafito for Windows (.exe)..."
    echo ""

    for tool in "$MINGW_CC" "$MINGW_AR" "$MINGW_WINDRES" "$OBJDUMP"; do
        command -v "$tool" >/dev/null 2>&1 || {
            echo "ERROR: required MinGW tool is not installed: $tool" >&2
            echo "Install mingw-w64 and run this script again." >&2
            exit 1
        }
    done

    # Add Windows target if not already added
    echo "Adding Windows target..."
    rustup target add "$TARGET" 2>/dev/null || true

    # Build for Windows
    echo "Building Windows executable..."
    CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER="$MINGW_CC" \
        CARGO_TARGET_X86_64_PC_WINDOWS_GNU_AR="$MINGW_AR" \
        WINDRES="$MINGW_WINDRES" \
        cargo build --manifest-path "$ROOT_DIR/Cargo.toml" --release --target "$TARGET" -p grafito-app --locked

    # Check if build succeeded
    if [[ -f "$EXE" ]]; then
        inspect_and_bundle_imports
        run_wine_smoke
        echo ""
        echo "Windows executable built successfully!"
        echo "Output: $EXE"
        echo ""
        echo "File size:"
        ls -lh "$EXE"
    else
        echo ""
        echo "ERROR: Build failed. Check the output above for errors."
        exit 1
    fi
}

if [[ "${BASH_SOURCE[0]}" == "$0" ]]; then
    main "$@"
fi
