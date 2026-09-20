#!/usr/bin/env bash
# Loop headless de grafito-mode: no se detiene hasta testigo o STOP.
#
# Un turno de LLM siempre termina con texto; ESTE script es el loop real:
# re-invoca `opencode run` y el estado vive en archivos (ledger +
# lab/witnesses/hn/README.md), nunca en la memoria de la sesión.
#
# Uso: scripts/grafito-loop.sh [max_iters] [mensaje]
#   max_iters  0 = infinito (default), N = tope de iteraciones.
# Detención: testigo en lab/witnesses/hn/CHI6_WITNESS.json,
#            archivo .jspace/STOP, o tope de iteraciones.
set -u

ROOT="$(git rev-parse --show-toplevel 2>/dev/null || pwd)"
cd "$ROOT" || exit 1
MAX_ITERS="${1:-0}"
MSG="${2:-/grafito-hunt retomar: seguí el loop desde el ledger y lab/witnesses/hn/README.md; sin reportes finales, solo próxima acción + trabajo}"
WITNESS="lab/witnesses/hn/CHI6_WITNESS.json"
STOPFILE=".jspace/STOP"

iter=0
while true; do
    if [ -f "$STOPFILE" ]; then
        echo "STOP presente: fin del loop."
        exit 0
    fi
    if [ -f "$WITNESS" ]; then
        echo "Testigo presente ($WITNESS): fin del loop."
        exit 0
    fi
    iter=$((iter + 1))
    if [ "$MAX_ITERS" -gt 0 ] && [ "$iter" -gt "$MAX_ITERS" ]; then
        echo "Tope de $MAX_ITERS iteraciones: fin."
        exit 0
    fi
    echo "=== iter $iter $(date -Is) ==="
    if ! opencode run --agent grafito-mode "$MSG"; then
        echo "opencode run salió $?; espero 60 s y sigo"
        sleep 60
    fi
done
