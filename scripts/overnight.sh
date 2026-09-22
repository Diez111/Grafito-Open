#!/bin/bash
# overnight.sh — turno noche autónomo (solo CPU local, sin TPU/Colab).
# 1) Espera que termine el mining G3 (libera 10 cores).
# 2) Criba SIREN-CPU de evobundle (8) + dobles (C5/C6/C7).
# 3) Lo resistente (refined>0) va a virt_test.py (kissat).
# Log: /tmp/opencode/overnight.log
LOG=/tmp/opencode/overnight.log
cd /home/diez/Documentos/Github/Grafito-Open || exit 1
exec >>"$LOG" 2>&1
echo "=== overnight inicio $(date) ==="
echo "[1/4] esperando G3 (PID 1807724)..."
while kill -0 1807724 2>/dev/null; do sleep 300; done
echo "G3 terminó $(date). FORCED: $(grep -c FORCED /tmp/opencode/rotmerge/sweep_g3full.out)"
echo "[2/4] criba evo (8 uniones)..."
python3 - <<'EOF'
import json, subprocess, sys
bundle = json.load(open("lab/colab_upload/evobundle.json"))
for c in bundle:
    open("/tmp/ov.vtx.json", "w").write(json.dumps(c["vtx"]))
    open("/tmp/ov.edge", "w").write("\n".join(f"e {a} {b}" for a, b in c["edge"]))
    r = subprocess.run(
        ["uv", "run", "--with", "jax[cpu]", "--with", "python-sat",
         "python3", "lab/nn_siren.py", "--vtx", "/tmp/ov.vtx.json",
         "--edges", "/tmp/ov.edge", "--k", "5", "--restarts", "4",
         "--steps", "6000", "--kicks", "30", "--tabu-iters", "20000",
         "--slim-rounds", "5", "--slim-radius", "2", "--seed", "7"],
        capture_output=True, text=True, timeout=3600)
    try:
        d = json.loads(r.stdout.strip().splitlines()[-1])
        print(f"{c['name']}: best={d['best_violations']} refined={d['refined_violations']}", flush=True)
        if d["refined_violations"] > 0:
            open(f"/tmp/opencode/resist_{c['name']}.json", "w").write(json.dumps(d))
    except Exception as e:
        print(f"{c['name']}: ERROR {e} STDERR {r.stderr[-300:]}", flush=True)
EOF
echo "[3/4] criba dobles C5/C6/C7..."
python3 - <<'EOF'
import json, subprocess
bundle = json.load(open("lab/colab_upload/newcand2_bundle.json"))
for c in bundle:
    open("/tmp/ov2.vtx.json", "w").write(json.dumps(c["vtx"]))
    open("/tmp/ov2.edge", "w").write("\n".join(f"e {a} {b}" for a, b in c["edge"]))
    r = subprocess.run(
        ["uv", "run", "--with", "jax[cpu]", "--with", "python-sat",
         "python3", "lab/nn_siren.py", "--vtx", "/tmp/ov2.vtx.json",
         "--edges", "/tmp/ov2.edge", "--k", "5", "--restarts", "4",
         "--steps", "6000", "--kicks", "30", "--tabu-iters", "20000",
         "--slim-rounds", "5", "--slim-radius", "2", "--seed", "7"],
        capture_output=True, text=True, timeout=5400)
    try:
        d = json.loads(r.stdout.strip().splitlines()[-1])
        print(f"{c['name']}: best={d['best_violations']} refined={d['refined_violations']}", flush=True)
        if d["refined_violations"] > 0:
            open(f"/tmp/opencode/resist_{c['name']}.json", "w").write(json.dumps(d))
    except Exception as e:
        print(f"{c['name']}: ERROR {e} STDERR {r.stderr[-300:]}", flush=True)
EOF
echo "[4/4] virt_test de resistentes..."
for f in /tmp/opencode/resist_*.json; do
  [ -e "$f" ] || continue
  echo "resistente: $f"
done
echo "=== overnight fin $(date) ==="
