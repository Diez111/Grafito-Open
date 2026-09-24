#!/bin/bash
# parmine.sh — test de pares en PARALELO (xargs -P8): base+nomono fija + P≠Q.
# Uso: parmine.sh <base.cnf> <pairs> <k> <out>   (pares: "i j" 0-based)
# Replica el patrón test_pair_env.sh que resolvió el G3 (420k pares).
KISSAT=/home/diez/.local/bin/kissat
BASE=$1; PAIRS=$2; K=$3; OUT=$4
test_pair() {
  P=$1; Q=$2
  f=$(mktemp /tmp/opencode/.pm_XXXXXX.cnf)
  {
    head -n1 "$BASE" | awk -v k="$K" '{print $1, $2, $3, $4+k}'
    tail -n +2 "$BASE"
    for c in $(seq 1 "$K"); do
      echo "-$((P * K + c)) -$((Q * K + c)) 0"
    done
  } > "$f"
  # ojo: el header p cnf queda con conteo viejo; kissat lo tolera si sobran
  if $KISSAT "$f" 2>/dev/null | grep -q "^s UNSATISFIABLE"; then
    echo "FORCED $P $Q"
  fi
  rm -f "$f"
}
export -f test_pair
export KISSAT BASE K
: > "$OUT"
cut -d' ' -f1,2 "$PAIRS" | xargs -P8 -n2 bash -c 'test_pair "$1" "$2"' _ >> "$OUT"
echo "parmine fin: $(grep -c FORCED "$OUT") FORCED" | tee -a "$OUT"
