#!/usr/bin/env python3
"""shrink.py: minimizacion greedy preservando UNSAT (2026-09-20).
Dado grafo + k con k-CNF UNSAT, intenta sacar vertices (orden aleatorio)
conservando UNSAT. Acotado por presupuesto (max_tests / max_seconds).
Uso:
  shrink.py --test     # mecanica: triangulo+pendant k=2 -> debe volver al triangulo
  shrink.py --points P.json --edges E.edge --k 4 --seed 1 --budget 300 --out R.json
"""

import argparse
import json
import random
import sys
import time

sys.path.insert(0, "/tmp/opencode")
from hn_hunt import kcolor_cnf, solve_cnf, load_edge, load_points_json


def shrink(pts, edges, k, seed=1, max_tests=200, max_seconds=600, timeout_each=120):
    t0 = time.time()
    rng = random.Random(seed)
    # verificar base UNSAT
    st, _, _ = solve_cnf(kcolor_cnf(len(pts), edges, k), timeout_each)
    if st != "UNSAT":
        return {
            "kept": sorted(range(len(pts))),
            "base": st,
            "note": "base no UNSAT, nada que minimizar",
        }
    alive = set(range(len(pts)))
    order = list(alive)
    rng.shuffle(order)
    tests = 0
    for v in order:
        if tests >= max_tests or time.time() - t0 > max_seconds:
            break
        trial = alive - {v}
        # reindexar
        mp = {o: i for i, o in enumerate(sorted(trial))}
        te = {(mp[a], mp[b]) for a, b in edges if a in trial and b in trial}
        st, _, _ = solve_cnf(kcolor_cnf(len(trial), te, k), timeout_each)
        tests += 1
        if st == "UNSAT":
            alive = trial
    kept = sorted(alive)
    mp = {o: i for i, o in enumerate(kept)}
    te = {(mp[a], mp[b]) for a, b in edges if a in alive and b in alive}
    pts2 = [pts[i] for i in kept]
    return {
        "kept": kept,
        "n": len(kept),
        "edges": len(te),
        "tests": tests,
        "seconds": round(time.time() - t0, 1),
        "base": "UNSAT",
        "points": pts2,
        "edge_list": sorted(te),
    }


def run_test():
    # Triangulo + pendant en (-1,0): k=2 UNSAT por el triangulo.
    # El greedy DEBE sacar el pendant (3) y quedarse con [0,1,2].
    import math

    pts = [[0, 0], [1, 0], [0.5, math.sqrt(3) / 2], [-1, 0]]
    edges = {(0, 1), (0, 2), (1, 2), (0, 3)}
    r = shrink(pts, edges, 2, seed=1, max_tests=10, max_seconds=60)
    assert r["base"] == "UNSAT", r
    assert 3 not in r["kept"], r
    assert r["n"] == 3, r
    # Triangulo solo: nada que sacar.
    r2 = shrink(pts[:3], {(0, 1), (0, 2), (1, 2)}, 2, seed=1)
    assert r2["n"] == 3, r2
    print("shrink --test OK: pendant removido, triangulo intacto, invariante UNSAT")
    return True


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--test", action="store_true")
    ap.add_argument("--points", default=None)
    ap.add_argument("--edges", default=None)
    ap.add_argument("--k", type=int, default=4)
    ap.add_argument("--seed", type=int, default=1)
    ap.add_argument("--budget", type=int, default=200)
    ap.add_argument("--seconds", type=float, default=600)
    ap.add_argument("--out", default=None)
    a = ap.parse_args()
    if a.test:
        run_test()
        return
    pts = load_points_json(a.points)
    edges = load_edge(a.edges)
    r = shrink(pts, edges, a.k, a.seed, a.budget, a.seconds)
    keep = {k: v for k, v in r.items() if k != "points"}
    out = json.dumps(keep)
    if a.out:
        json.dump(r, open(a.out, "w"))
    print(out)


if __name__ == "__main__":
    main()
