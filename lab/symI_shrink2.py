#!/usr/bin/env python3
"""symI_shrink2.py: shrink greedy en 2 fases preservando UNSAT (agente I).

Fase gruesa: borra LOTES de 8 vertices (orden aleatorio) mientras conserve UNSAT.
Fase fina: vertice por vertice (como shrink.py).
Todo SAT-check usa el CNF propio de symI_cnf con symbreak (velocidad).
Al final valida el invariante UNSAT sobre el subgrafo conservado.

Uso libreria:
  from symI_shrink2 import shrink2
  r = shrink2(n, edges, k, seed=1, budget_s=600, timeout_each=120, batch=8)
Uso CLI:
  symI_shrink2.py --test
  symI_shrink2.py --edges /tmp/opencode/cnpD_553.edge --k 4 --seed 1 --budget 600 --out R.json
"""

import argparse
import json
import random
import subprocess
import sys
import time

sys.path.insert(0, "/tmp/opencode")
from symI_cnf import build_cnf, to_dimacs

KISSAT = "/home/diez/.local/bin/kissat"
TMP_CNF = "/tmp/opencode/symI_tmp.cnf"


def solve_status(n, edges, k, timeout_each=120):
    """UNSAT/SAT/TIMEOUT/ERROR via symI_cnf(symbreak=True) + kissat."""
    nvars, clauses = build_cnf(n, edges, k, symbreak=True)
    with open(TMP_CNF, "w") as f:
        f.write(to_dimacs(nvars, clauses))
    try:
        r = subprocess.run(
            [KISSAT, TMP_CNF], capture_output=True, text=True, timeout=timeout_each
        )
    except subprocess.TimeoutExpired:
        return "TIMEOUT"
    for line in r.stdout.splitlines():
        t = line.strip()
        if t.startswith("s "):
            low = t.lower()
            if "unsat" in low:
                return "UNSAT"
            if "sat" in low:
                return "SAT"
    return "ERROR"


def _induced(edges, alive):
    mp = {o: i for i, o in enumerate(sorted(alive))}
    te = {(mp[a], mp[b]) for a, b in edges if a in alive and b in alive}
    return te


def shrink2(n, edges, k, seed=1, budget_s=600, timeout_each=120, batch=8):
    t0 = time.time()
    deadline = t0 + budget_s
    edges = {(min(a, b), max(a, b)) for a, b in edges if a != b}
    base = solve_status(n, edges, k, timeout_each)
    if base != "UNSAT":
        return {
            "base": base,
            "kept": sorted(range(n)),
            "n": n,
            "removed": 0,
            "seconds": round(time.time() - t0, 2),
            "invariant": False,
            "note": "base no UNSAT, nada que minimizar",
        }
    rng = random.Random(seed)
    alive = set(range(n))
    tests = 0
    removed_coarse = 0
    # --- fase gruesa: lotes de `batch` ---
    while time.time() < deadline:
        order = sorted(alive)
        rng.shuffle(order)
        progress = False
        for i in range(0, len(order), batch):
            if time.time() >= deadline:
                break
            lot = set(order[i : i + batch])
            if not lot or lot >= alive and len(lot) == len(alive):
                continue  # no probar vaciado total
            trial = alive - lot
            if not trial:
                continue
            te = _induced(edges, trial)
            st = solve_status(len(trial), te, k, timeout_each)
            tests += 1
            if st == "UNSAT":
                alive = trial
                removed_coarse += len(lot)
                progress = True
            elif st == "TIMEOUT":
                break  # solver lento: ceder a fase fina / terminar
        if not progress:
            break
    # --- fase fina: vertice por vertice ---
    order = sorted(alive)
    rng.shuffle(order)
    removed_fine = 0
    for v in order:
        if time.time() >= deadline:
            break
        trial = alive - {v}
        if not trial:
            continue
        te = _induced(edges, trial)
        st = solve_status(len(trial), te, k, timeout_each)
        tests += 1
        if st == "UNSAT":
            alive = trial
            removed_fine += 1
        elif st == "TIMEOUT":
            break
    kept = sorted(alive)
    te = _induced(edges, alive)
    # --- validacion final del invariante UNSAT ---
    final = solve_status(len(kept), te, k, timeout_each)
    dt = round(time.time() - t0, 2)
    return {
        "base": "UNSAT",
        "kept": kept,
        "n": len(kept),
        "edges": len(te),
        "removed": n - len(kept),
        "removed_coarse": removed_coarse,
        "removed_fine": removed_fine,
        "tests": tests,
        "seconds": dt,
        "invariant": final == "UNSAT",
        "final_status": final,
    }


def run_test():
    import math

    pts_n = 4
    edges = {(0, 1), (0, 2), (1, 2), (0, 3)}
    r = shrink2(pts_n, edges, 2, seed=1, budget_s=60, timeout_each=30)
    assert r["base"] == "UNSAT", r
    assert r["invariant"], r
    assert 3 not in r["kept"], r
    assert r["n"] == 3, r
    r2 = shrink2(3, {(0, 1), (0, 2), (1, 2)}, 2, seed=1, budget_s=60, timeout_each=30)
    assert r2["n"] == 3 and r2["invariant"], r2
    print(
        "symI_shrink2 --test OK: pendant removido, triangulo intacto, invariante UNSAT"
    )
    return True


def load_edge(path):
    n = 0
    edges = set()
    with open(path) as f:
        for line in f:
            p = line.split()
            if not p:
                continue
            if p[0] == "p":
                n = int(p[2])
            elif p[0] == "e":
                a, b = int(p[1]) - 1, int(p[2]) - 1
                edges.add((min(a, b), max(a, b)))
    if n == 0 and edges:
        n = max(max(a, b) for a, b in edges) + 1
    return n, edges


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--test", action="store_true")
    ap.add_argument("--edges", default=None)
    ap.add_argument("--k", type=int, default=4)
    ap.add_argument("--seed", type=int, default=1)
    ap.add_argument("--budget", type=float, default=600)
    ap.add_argument("--timeout-each", type=float, default=120)
    ap.add_argument("--batch", type=int, default=8)
    ap.add_argument("--out", default=None)
    a = ap.parse_args()
    if a.test:
        run_test()
        return
    n, edges = load_edge(a.edges)
    r = shrink2(n, edges, a.k, a.seed, a.budget, a.timeout_each, a.batch)
    print(
        json.dumps(
            {
                kk: (vv if kk != "kept" or len(vv) < 50 else f"{len(vv)} vertices")
                for kk, vv in r.items()
            },
            indent=1,
        )
    )
    if a.out:
        json.dump(
            {kk: (vv if kk != "kept" else vv) for kk, vv in r.items()}, open(a.out, "w")
        )


if __name__ == "__main__":
    main()
