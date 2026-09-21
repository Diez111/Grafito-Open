#!/usr/bin/env python3
"""symI_cnf.py: generador CNF k-coloracion con symmetry-breaking (agente I).

Ideas paper Parts 2020a s5.8: fijar un triangulo elimina la simetria S_k
(permutacion de colores) y acelera UNSAT.

Symbreak:
  - si el grafo tiene un triangulo (t0,t1,t2): unidades t0=1, t1=2, t2=3.
  - si no: v0=1 + un vecino cualquiera=2 (rompe S_k parcialmente).
Requiere k>=3 para el caso triangulo; con k<3 solo fija lo posible.

Uso libreria:
  from symI_cnf import build_cnf, to_dimacs
  nvars, clauses = build_cnf(n, edges, k, symbreak=True)
Uso CLI:
  symI_cnf.py --edges /tmp/opencode/cnpD_553.edge --k 4 --symbreak --out /tmp/opencode/symI_553_k4.cnf
  symI_cnf.py --edges F.edge --k 4 --symbreak --order degree --out O.cnf
"""

import argparse
import sys


def find_triangle(n, edges):
    """Devuelve (t0,t1,t2) 0-indexed si existe triangulo, si no None. O(sum deg^2)."""
    adj = [set() for _ in range(n)]
    for a, b in edges:
        if 0 <= a < n and 0 <= b < n and a != b:
            adj[a].add(b)
            adj[b].add(a)
    for a, b in edges:
        if a == b:
            continue
        # interseccion de vecindarios: candidato c forma triangulo a-b-c
        sa, sb = adj[a], adj[b]
        if len(sa) > len(sb):
            sa, sb = sb, sa
            a2, b2 = b, a
        else:
            a2, b2 = a, b
        for c in sa:
            if c != a2 and c != b2 and c in sb:
                return (a2, b2, c)
    return None


def _relabel_degree(n, edges):
    """Permutacion por grado descendente. Devuelve (edges2, perm) con perm[old]=new."""
    deg = [0] * n
    for a, b in edges:
        deg[a] += 1
        deg[b] += 1
    order = sorted(range(n), key=lambda v: (-deg[v], v))
    perm = [0] * n
    for new, old in enumerate(order):
        perm[old] = new
    edges2 = set()
    for a, b in edges:
        na, nb = perm[a], perm[b]
        edges2.add((min(na, nb), max(na, nb)))
    return edges2, perm


def build_cnf(n, edges, k, symbreak=False, order=None):
    """Construye CNF k-coloracion.

    n: nro vertices (0..n-1). edges: iterable de (u,v) 0-indexed.
    k: colores. symbreak: bool. order: None | 'degree' | 'none'.
    Devuelve (num_vars, clauses) con clauses=list[list[int]].
    """
    if k < 1:
        raise ValueError("k>=1")
    edges = {(min(a, b), max(a, b)) for a, b in edges if a != b}
    perm = None
    if order in ("degree", "deg"):
        edges, perm = _relabel_degree(n, edges)
    var = lambda v, c: v * k + c  # v 0-indexed, c 1..k -> id 1-based
    clauses = []
    for v in range(n):
        clauses.append([var(v, c) for c in range(1, k + 1)])
        for a in range(1, k + 1):
            for b in range(a + 1, k + 1):
                clauses.append([-var(v, a), -var(v, b)])
    for a, b in edges:
        for c in range(1, k + 1):
            clauses.append([-var(a, c), -var(b, c)])
    info = {"triangle": None, "units": []}
    if symbreak:
        tri = find_triangle(n, edges)
        units = []
        if tri is not None and k >= 3:
            t0, t1, t2 = tri
            units = [var(t0, 1), var(t1, 2), var(t2, 3)]
            info["triangle"] = tri
        elif k >= 1 and n >= 1:
            # sin triangulo: v0=1 + vecino=2 (si hay arista y k>=2)
            v0 = 0
            units = [var(v0, 1)]
            if k >= 2:
                nbr = None
                for a, b in edges:
                    if a == v0:
                        nbr = b
                        break
                    if b == v0:
                        nbr = a
                        break
                if nbr is not None:
                    units.append(var(nbr, 2))
                # si v0 aislado, buscar cualquier arista y fijar sus extremos
                if nbr is None and edges:
                    a, b = next(iter(edges))
                    units = [var(a, 1), var(b, 2)]
            info["triangle"] = None
        for u in units:
            clauses.append([u])
        info["units"] = units
    if perm is not None:
        info["perm"] = perm
    build_cnf.last_info = info
    return (n * k, clauses)


def to_dimacs(num_vars, clauses):
    return (
        f"p cnf {num_vars} {len(clauses)}\n"
        + "\n".join(" ".join(map(str, cl)) + " 0" for cl in clauses)
        + "\n"
    )


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
    ap.add_argument("--edges", required=True)
    ap.add_argument("--k", type=int, required=True)
    ap.add_argument("--symbreak", action="store_true")
    ap.add_argument("--order", default="none", choices=["none", "degree", "deg"])
    ap.add_argument("--out", required=True)
    a = ap.parse_args()
    n, edges = load_edge(a.edges)
    order = None if a.order == "none" else a.order
    nvars, clauses = build_cnf(n, edges, a.k, symbreak=a.symbreak, order=order)
    with open(a.out, "w") as f:
        f.write(to_dimacs(nvars, clauses))
    info = getattr(build_cnf, "last_info", {})
    print(
        f"n={n} m={len(edges)} k={a.k} vars={nvars} clauses={len(clauses)} "
        f"symbreak={a.symbreak} order={a.order} triangle={info.get('triangle')} "
        f"units={info.get('units')}"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
