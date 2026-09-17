#!/usr/bin/env bash
# pgo.sh — build con Profile-Guided Optimization (PGO) para el binario grafito.
#
# Uso:
#   bash scripts/pgo.sh              # instrumenta, entrena headless, mergea y recompila
#   bash scripts/pgo.sh --interactive  # entrena con una sesión real de la GUI (lo usás vos)
#   bash scripts/pgo.sh --install      # además copia el binario a /usr/local/bin/grafito
#   bash scripts/pgo.sh --clean        # borra $PROFDIR y termina
#   PROFDIR=/ruta bash scripts/pgo.sh  # cambia el directorio de perfiles (default /tmp/grafito-pgo)
#
# Por qué: PGO usa perfiles de ejecución reales para que LLVM optimice layout y
# branches con datos, no con heurísticas. En apps gráficas suele dar 5-15% en
# CPU de frame. El "entrenamiento" ideal es una SESIÓN REAL de uso (--interactive);
# sin sesión, el default entrena con los harness headless (render + lib tests),
# que cubren samplers/render/matemática pero NO pintado egui.
#
# Honesto: el binario PGO queda en target/release/grafito; este script NO cambia
# defaults del repo ni CI. Requiere llvm-profdata del toolchain Rust
# (llvm-tools-preview) o compatible en PATH. Gate de formato: si el merge falla,
# el script corta con error (fail-closed), jamás compila con perfil a medias.
set -euo pipefail

PROFDIR="${PROFDIR:-/tmp/grafito-pgo}"
INTERACTIVE=0
INSTALL=0
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --interactive) INTERACTIVE=1; shift ;;
    --install) INSTALL=1; shift ;;
    --clean) rm -rf "$PROFDIR"; echo "limpiado $PROFDIR"; exit 0 ;;
    -h|--help) sed -n '2,20p' "$0"; exit 0 ;;
    *) echo "flag desconocida: $1"; exit 1 ;;
  esac
done

# 1) llvm-profdata: preferir el del toolchain Rust (misma versión LLVM que rustc).
HOST="$(rustc -vV | sed -n 's/^host: //p')"
SYSROOT="$(rustc --print sysroot)"
PROFDATA="$SYSROOT/lib/rustlib/$HOST/bin/llvm-profdata"
if [[ ! -x "$PROFDATA" ]]; then
  PROFDATA="$(command -v llvm-profdata || true)"
fi
if [[ -z "${PROFDATA:-}" || ! -x "$PROFDATA" ]]; then
  echo "error: falta llvm-profdata (instalá el componente: rustup component add llvm-tools-preview)" >&2
  exit 1
fi
echo "→ llvm-profdata: $PROFDATA ($("$PROFDATA" --version | sed -n '2p'))"

mkdir -p "$PROFDIR"
rm -f "$PROFDIR"/*.profraw

# 2) Build instrumentado (release) del binario.
export RUSTFLAGS="-Cprofile-generate=$PROFDIR"
echo "→ build instrumentado (-Cprofile-generate=$PROFDIR)…"
(cd "$ROOT" && cargo build --release -p grafito-app --locked)

BIN="$ROOT/target/release/grafito"
[[ -x "$BIN" ]] || { echo "error: no se generó $BIN" >&2; exit 1; }

# 3) Entrenamiento.
if [[ "$INTERACTIVE" == "1" ]]; then
  echo "→ sesión interactiva en 3s: usá Grafito (dibujá, paneá, zoom, asistente) ~1-3 min y cerralo."
  sleep 3
  "$BIN" || true
else
  echo "→ entrenamiento headless (render + lib tests con el perfil activo)…"
  (cd "$ROOT" && cargo test -p grafito-render -p grafito-app --lib --test headless_render --release --locked) || true
  # El bench RGBA cubre las 13 plantillas nativas (draw CPU puro).
  (cd "$ROOT" && cargo bench -p grafito-app --bench native_rgba --release -- --test) || true
fi

# 4) Merge (fail-closed).
SHOPT_NULLGLOB="$(shopt -p nullglob || true)"
shopt -s nullglob
PROFRAWS=("$PROFDIR"/*.profraw)
[[ "$SHOPT_NULLGLOB" == *"-u nullglob"* ]] && shopt -u nullglob
if [[ ${#PROFRAWS[@]} -eq 0 ]]; then
  echo "error: no se generó ningún .profraw en $PROFDIR (¿el entrenamiento no ejecutó nada instrumentado?)" >&2
  exit 1
fi
echo "→ merge de ${#PROFRAWS[@]} perfil(es)…"
"$PROFDATA" merge -o "$PROFDIR/merged.profdata" "${PROFRAWS[@]}"

# 5) Rebuild optimizado con el perfil.
export RUSTFLAGS="-Cprofile-use=$PROFDIR/merged.profdata -Cllvm-args=-pgo-warn-missing-function"
echo "→ rebuild final (-Cprofile-use)…"
(cd "$ROOT" && cargo build --release -p grafito-app --locked)

echo "✓ binario PGO: $BIN ($(du -h "$BIN" | cut -f1))"
if [[ "$INSTALL" == "1" ]]; then
  install -m 0755 "$BIN" /usr/local/bin/grafito
  echo "✓ instalado en /usr/local/bin/grafito"
fi
