#!/usr/bin/env python3
"""sowL_gen.py — siembra L2 Hadwiger-Nelson, estilo Parts/Heule.
H exacto (fracciones + sqrt3), rotacion compleja arbitraria,
suma Minkowski con dedup tol 1e-9, union con dedup,
grafo unitario completo tol 1e-6, CNF k4/k5. Solo /tmp/opencode.
"""

import math

SQRT3_2 = math.sqrt(3.0) / 2.0
ETA = 0.5 * math.acos(5.0 / 6.0)  # exp((i/2) arccos(5/6)) ~= 16.778655 deg
RHO = math.acos(7.0 / 8.0)  # exp(i arccos(7/8)) ~= 28.955024 deg
TOL_DEDUP = 1e-9
TOL_UNIT = 1e-6


def H_set():
    return [
        (0.0, 0.0),
        (1.0, 0.0),
        (-1.0, 0.0),
        (0.5, SQRT3_2),
        (-0.5, SQRT3_2),
        (0.5, -SQRT3_2),
        (-0.5, -SQRT3_2),
    ]


def rot_pts(pts, ang):
    c, s = math.cos(ang), math.sin(ang)
    return [(x * c - y * s, x * s + y * c) for (x, y) in pts]


def union_all(sets, tol=TOL_DEDUP):
    out = []
    for S in sets:
        for x, y in S:
            if not any(abs(ox - x) < tol and abs(oy - y) < tol for (ox, oy) in out):
                out.append((x, y))
    return out


def minkowski(A, B, tol=TOL_DEDUP):
    out = []
    for ax, ay in A:
        for bx, by in B:
            x, y = ax + bx, ay + by
            if not any(abs(ox - x) < tol and abs(oy - y) < tol for (ox, oy) in out):
                out.append((x, y))
    return out


def unit_edges(pts, tol=TOL_UNIT):
    E = []
    n = len(pts)
    for i in range(n):
        xi, yi = pts[i]
        for j in range(i + 1, n):
            if abs(math.hypot(pts[j][0] - xi, pts[j][1] - yi) - 1.0) < tol:
                E.append((i, j))
    return E


def write_cnf(pts, E, k, path):
    n = len(pts)

    def var(i, c):
        return i * k + c + 1

    clauses = []
    for i in range(n):
        clauses.append([var(i, c) for c in range(k)])
        for c1 in range(k):
            for c2 in range(c1 + 1, k):
                clauses.append([-var(i, c1), -var(i, c2)])
    for u, v in E:
        for c in range(k):
            clauses.append([-var(u, c), -var(v, c)])
    with open(path, "w") as f:
        f.write(f"p cnf {n * k} {len(clauses)}\n")
        for cl in clauses:
            f.write(" ".join(map(str, cl)) + " 0\n")
    return len(clauses)


def write_vtx(pts, path):
    with open(path, "w") as f:
        for x, y in pts:
            f.write(f"{x:.15f} {y:.15f}\n")


def Hk(k):
    """H^k = union de k copias de H rotadas por multiplos de eta (H^0=H)."""
    H = H_set()
    return union_all([rot_pts(H, i * ETA) for i in range(max(k, 1))])


if __name__ == "__main__":
    print(f"eta={ETA:.15f} rad ({math.degrees(ETA):.6f} deg)")
    print(f"rho={RHO:.15f} rad ({math.degrees(RHO):.6f} deg)")
    H = H_set()
    V31 = union_all([rot_pts(H, i * ETA) for i in range(5)])
    E31 = unit_edges(V31)
    print(f"V31=union_{{i=0..4}} eta^i H: n={len(V31)} aristas={len(E31)}")
    write_vtx(V31, "/tmp/opencode/sowL_V31.vtx")
    write_cnf(V31, E31, 4, "/tmp/opencode/sowL_V31_k4.cnf")
    write_cnf(V31, E31, 5, "/tmp/opencode/sowL_V31_k5.cnf")

    cands = {}
    # (b) variantes L union rho S con rho
    cands["L=V31_S=H_rho"] = union_all([V31, rot_pts(H, RHO)])
    cands["L=V31_S=H1_rho"] = union_all([V31, rot_pts(Hk(2), RHO)])
    cands["L=V31_S=H2parc_rho"] = union_all([V31, rot_pts(Hk(3), RHO)])
    cands["L=H1_S=H_rho"] = union_all([Hk(2), rot_pts(H, RHO)])
    cands["L=H_S=H_rho"] = union_all([H, rot_pts(H, RHO)])
    # (b extra) L grande via Minkowski + S chico
    M = minkowski(V31, [(0.0, 0.0), (1.0, 0.0)])
    cands["L=V31x01_S=H_rho"] = union_all([M, rot_pts(H, RHO)])
    # (c) variantes con eta
    cands["L=V31_S=H_eta5"] = union_all([V31, rot_pts(H, 5 * ETA)])
    cands["L=V31_S=H1_eta5"] = union_all([V31, rot_pts(Hk(2), 5 * ETA)])
    cands["L=H1_S=H1_eta"] = union_all([Hk(2), rot_pts(Hk(2), ETA)])
    for name, pts in cands.items():
        E = unit_edges(pts)
        print(f"{name}: n={len(pts)} aristas={len(E)}")
        write_vtx(pts, f"/tmp/opencode/sowL_{name}.vtx")
        write_cnf(pts, E, 4, f"/tmp/opencode/sowL_{name}_k4.cnf")
