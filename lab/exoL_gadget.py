#!/usr/bin/env python3
"""exoL_gadget.py — L3 Exoo-Ismailescu: G40 (mono 8/3 CONDICIONAL) + forcing SAT.

Paper: arXiv:1805.00157 (Exoo-Ismailescu, 12p). Coords [a,b,c,d] exactas del paper:
  [a,b,c,d] := (a*sqrt3/36 + b*sqrt11/36, c/36 + d*sqrt33/36)
G40: 40 pts (paper p.3), 82 aristas unidad, 59 pares sqrt(11/3); par rojo 8/3:
  P=[0,0,0,0]=(0,0), Q=[0,0,96,0]=(0,8/3).
Claim papel (p.3-4): en todo 4-coloreo que EVITA mono sqrt(11/3), P y Q mismo color.
NO es mono-pair incondicional a nivel 4. Tests:
  T1 base k4 -> SAT (G40 es 4-coloreable)
  T2 base + P!=Q -> SAT esperado (= NO hay forcing incondicional -> abort)
  T3 base + no-mono-11/3 + P!=Q -> UNSAT esperado (= claim condicional del paper)
Solver: /home/diez/.local/bin/kissat.
"""

import math, subprocess, time, json, sys

KISSAT = "/home/diez/.local/bin/kissat"
S3, S11, S33 = math.sqrt(3.0), math.sqrt(11.0), math.sqrt(33.0)
TOL = 1e-6
D113 = math.sqrt(11.0 / 3.0)
D83 = 8.0 / 3.0


def dec(t):
    a, b, c, d = t
    return (a * S3 / 36.0 + b * S11 / 36.0, c / 36.0 + d * S33 / 36.0)


G40_RAW = [
    (0, 0, 0, 0),
    (0, 0, 96, 0),
    (-33, -3, 33, -3),
    (-33, 3, 33, -9),
    (-33, 3, 33, 3),
    (-33, 3, 63, -3),
    (-33, 9, 63, 3),
    (-18, 0, 48, -6),
    (-18, 0, 48, 6),
    (-18, 6, 48, 0),
    (-15, -9, 15, 3),
    (-15, -3, 15, -3),
    (-15, -3, 45, 3),
    (-15, 3, -15, -3),
    (-15, 3, 15, -9),
    (-15, 3, 15, 3),
    (-15, 3, 81, -3),
    (-15, 3, 111, 3),
    (-15, 9, 15, -3),
    (-15, 9, 81, 3),
    (0, -12, 0, 0),
    (0, -6, 30, 0),
    (0, -6, 66, 0),
    (0, 0, 30, -6),
    (0, 0, 30, 6),
    (0, 0, 66, -6),
    (0, 0, 66, 6),
    (0, 6, 0, -6),
    (0, 6, 0, 6),
    (0, 6, 30, 0),
    (0, 6, 66, 0),
    (0, 6, 96, 6),
    (0, 12, 30, 6),
    (0, 12, 66, 6),
    (15, 3, 15, -3),
    (15, 3, 81, 3),
    (18, 0, 48, -6),
    (18, 6, 48, 0),
    (33, 3, 33, -3),
    (33, 3, 63, 3),
]


def build():
    pts = [dec(t) for t in G40_RAW]
    assert len(pts) == 40
    ue, me = [], []
    for i in range(40):
        for j in range(i + 1, 40):
            d = math.hypot(pts[i][0] - pts[j][0], pts[i][1] - pts[j][1])
            if abs(d - 1.0) < TOL:
                ue.append((i, j))
            if abs(d - D113) < TOL:
                me.append((i, j))
    dPQ = math.hypot(pts[0][0] - pts[1][0], pts[0][1] - pts[1][1])
    return pts, ue, me, dPQ


def to_cnf(n, edges, k, extra=None):
    cl = []
    for v in range(n):
        cl.append([(v * k + c + 1) for c in range(k)])
        for c1 in range(k):
            for c2 in range(c1 + 1, k):
                cl.append([-(v * k + c1 + 1), -(v * k + c2 + 1)])
    for u, v in edges:
        for c in range(k):
            cl.append([-(u * k + c + 1), -(v * k + c + 1)])
    if extra:
        cl.extend(extra)
    return "p cnf %d %d\n" % (n * k, len(cl)) + "".join(
        " ".join(map(str, c)) + " 0\n" for c in cl
    )


def run(cnf, tag):
    p = "/tmp/opencode/exoL_%s.cnf" % tag
    open(p, "w").write(cnf)
    t0 = time.perf_counter()
    r = subprocess.run([KISSAT, "-q", p], capture_output=True, text=True, timeout=300)
    dt = time.perf_counter() - t0
    out = (r.stdout or "") + (r.stderr or "")
    if "UNSATISFIABLE" in out or r.returncode == 20:
        return "UNSAT", dt, p
    if "SATISFIABLE" in out or r.returncode == 10:
        return "SAT", dt, p
    raise RuntimeError("kissat rc=%d %s" % (r.returncode, out[:300]))


def diff_clause(u, v, k):
    # u y v distinto color: para cada color c, no ambos c
    return [[-(u * k + c + 1), -(v * k + c + 1)] for c in range(k)]


def main():
    pts, ue, me, dPQ = build()
    P, Q, k = 0, 1, 4
    r1, t1, f1 = run(to_cnf(40, ue, k), "G40_k4")
    r2, t2, f2 = run(to_cnf(40, ue, k, diff_clause(P, Q, k)), "G40_k4_diff83")
    nomono = []
    for u, v in me:
        nomono.extend(diff_clause(u, v, k))
    r3, t3, f3 = run(
        to_cnf(40, ue, k, nomono + diff_clause(P, Q, k)), "G40_k4_nomono113_diff83"
    )
    # control: base + no-mono-11/3 solo (debe ser SAT: existe coloreo sin mono 11/3)
    r4, t4, f4 = run(to_cnf(40, ue, k, nomono), "G40_k4_nomono113")
    print(
        json.dumps(
            {
                "n": 40,
                "unit_edges": len(ue),
                "pairs_11_3": len(me),
                "d_PQ": dPQ,
                "P": P,
                "Q": Q,
                "T1_base_k4": [r1, round(t1, 3), f1],
                "T2_diff83": [r2, round(t2, 3), f2],
                "T3_nomono113_diff83": [r3, round(t3, 3), f3],
                "T4_nomono113": [r4, round(t4, 3), f4],
            },
            indent=1,
        )
    )


if __name__ == "__main__":
    main()
