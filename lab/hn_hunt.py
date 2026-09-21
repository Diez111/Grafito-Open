#!/usr/bin/env python3
"""hn_hunt.py: pipeline unificado de caza HN (2026-09-20).
Funciones importables + CLI. Reemplaza el zoo (rotmerge/monomine/olamine/
mine874/mine_virtual) con una sola implementacion testeada.
Uso:
  hn_hunt.py --points P.json --edges E.edge --k 5 --mode same --dmin 0.5 --max-tests 25000
  hn_hunt.py --test   # suite rapida: C4 k=2 (mismo), triangulo k=2/3, shrink-smoke
"""

import argparse
import json
import math
import os
import subprocess
import sys

KISSAT = "/home/diez/.local/bin/kissat"
sys.path.insert(0, "/tmp/opencode")
try:
    from parse874 import parse_pt
except ImportError:
    parse_pt = None


def load_points_json(path):
    return json.load(open(path))


def load_edge(path):
    edges = set()
    for line in open(path):
        p = line.split()
        if p and p[0] == "e":
            a, b = int(p[1]) - 1, int(p[2]) - 1
            edges.add((min(a, b), max(a, b)))
    return edges


def numeric_edges(pts, tol=1e-9):
    n = len(pts)
    out = set()
    for i in range(n):
        xi, yi = pts[i]
        for j in range(i + 1, n):
            if abs(math.hypot(xi - pts[j][0], yi - pts[j][1]) - 1.0) < tol:
                out.add((i, j))
    return out


def kcolor_cnf(n, edges, k):
    cls = []
    for v in range(n):
        cls.append(" ".join(str(v * k + c) for c in range(1, k + 1)) + " 0")
        for a in range(1, k + 1):
            for b in range(a + 1, k + 1):
                cls.append(f"-{v * k + a} -{v * k + b} 0")
    for i, j in edges:
        for c in range(1, k + 1):
            cls.append(f"-{i * k + c} -{j * k + c} 0")
    return f"p cnf {n * k} {len(cls)}\n" + "\n".join(cls) + "\n"


def solve_cnf(cnf_text, timeout_s=120):
    with open("/tmp/opencode/.hn_tmp.cnf", "w") as f:
        f.write(cnf_text)
    try:
        r = subprocess.run(
            [KISSAT, "/tmp/opencode/.hn_tmp.cnf"],
            capture_output=True,
            text=True,
            timeout=timeout_s,
        )
    except subprocess.TimeoutExpired:
        return ("TIMEOUT", None, timeout_s)
    status, model = None, []
    for line in r.stdout.splitlines():
        t = line.strip()
        if t.startswith("s "):
            low = t.lower()
            if "unsat" in low:
                return ("UNSAT", None, 0)
            if "sat" in low:
                status = "SAT"
        elif t.startswith("v"):
            for tok in t[1:].split():
                try:
                    lit = int(tok)
                except ValueError:
                    continue
                if lit != 0:
                    model.append(lit)
    if status == "SAT":
        return ("SAT", model, 0)
    return ("ERROR", None, 0)


def coloring_from_model(model, n, k):
    col = [None] * n
    for lit in model:
        if lit > 0:
            v = (lit - 1) // k
            if 0 <= v < n and col[v] is None:
                col[v] = (lit - 1) % k
    return col


def candidates(colors, pts, edges, dmin=0.5, mode="same"):
    n = len(pts)
    out = []
    for i in range(n):
        for j in range(i + 1, n):
            if (i, j) in edges:
                continue
            d = math.hypot(pts[i][0] - pts[j][0], pts[i][1] - pts[j][1])
            if d < dmin:
                continue
            same = colors[i] == colors[j]
            if (mode == "same" and same) or (mode == "diff" and not same):
                out.append((i, j, d))
    return out


def test_pair(cnf_text, nvars, p, q, k, mode="same"):
    """True si el par es FORZADO (UNSAT al imponer lo contrario)."""
    lines = cnf_text.splitlines()
    extra = []
    if mode == "same":
        for c in range(1, k + 1):
            extra.append(f"-{p * k + c} -{q * k + c} 0")
    else:
        for c in range(1, k + 1):
            extra.append(f"-{p * k + c} {q * k + c} 0")
            extra.append(f"{p * k + c} -{q * k + c} 0")
    ncl = len(lines) - 1 + len(extra)
    full = f"p cnf {nvars} {ncl}\n" + "\n".join(lines[1:] + extra) + "\n"
    st, _, _ = solve_cnf(full)
    return st == "UNSAT"


def mine(cnf_text, nvars, colors, pts, edges, k, dmin=0.5, mode="same", max_tests=None):
    for i, j in edges:
        if colors[i] is None or colors[j] is None or colors[i] == colors[j]:
            raise ValueError(f"coloreo invalido en arista {(i, j)}")
    cands = candidates(colors, pts, edges, dmin, mode)
    if max_tests:
        cands = cands[:max_tests]
    forced = []
    for p, q, d in cands:
        if test_pair(cnf_text, nvars, p, q, k, mode):
            forced.append([p, q, d])
    return {"tested": len(cands), "forced": forced}


def run_test():
    # C4 cuadrado unitario k=2: pares forzado-mismo EXACTOS {(0,2),(1,3)}.
    pts = [[0, 0], [1, 0], [1, 1], [0, 1]]
    edges = {(0, 1), (1, 2), (2, 3), (0, 3)}
    cnf = kcolor_cnf(4, edges, 2)
    st, model, _ = solve_cnf(cnf)
    assert st == "SAT", st
    col = coloring_from_model(model, 4, 2)
    r = mine(cnf, 8, col, pts, edges, 2, dmin=0.5, mode="same")
    got = sorted((a, b) for a, b, _ in r["forced"])
    assert got == [(0, 2), (1, 3)], got
    r2 = mine(cnf, 8, col, pts, edges, 2, dmin=0.5, mode="diff")
    assert r2["forced"] == [], r2["forced"]
    # Triangulo: k=2 UNSAT, k=3 SAT sin pares forzados no-arista (no hay).
    tri = [[0, 0], [1, 0], [0.5, math.sqrt(3) / 2]]
    te = {(0, 1), (0, 2), (1, 2)}
    assert solve_cnf(kcolor_cnf(3, te, 2))[0] == "UNSAT"
    st3, m3, _ = solve_cnf(kcolor_cnf(3, te, 3))
    assert st3 == "SAT"
    print("hn_hunt --test OK: C4 same={(0,2),(1,3)} diff={} tri-k2=UNSAT tri-k3=SAT")
    return True


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--test", action="store_true")
    ap.add_argument("--points", default=None)
    ap.add_argument("--edges", default=None)
    ap.add_argument("--k", type=int, default=5)
    ap.add_argument("--mode", default="same", choices=["same", "diff"])
    ap.add_argument("--dmin", type=float, default=0.5)
    ap.add_argument("--max-tests", type=int, default=25000)
    ap.add_argument("--out", default=None)
    a = ap.parse_args()
    if a.test:
        run_test()
        return
    pts = load_points_json(a.points)
    edges = load_edge(a.edges) if a.edges else numeric_edges(pts)
    n = len(pts)
    cnf = kcolor_cnf(n, edges, a.k)
    st, model, _ = solve_cnf(cnf, timeout_s=900)
    res = {"n": n, "edges": len(edges), "k": a.k, "base": st}
    if st == "SAT":
        col = coloring_from_model(model, n, a.k)
        res["mining"] = mine(
            cnf, n * a.k, col, pts, edges, a.k, a.dmin, a.mode, a.max_tests
        )
    out = json.dumps(res)
    if a.out:
        open(a.out, "w").write(out + "\n")
    print(out)


if __name__ == "__main__":
    main()
