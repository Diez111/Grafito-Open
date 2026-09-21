#!/usr/bin/env python3
"""gadF_kit.py — Agente F: kit de construcción Parts 2020a (arXiv 2010.12665v2).

Piezas (Secs. Definitions / Notation / Type J del paper):
  - rombo unidad parametrizado (lado 1, ángulo theta; default 60° -> diagonal
    corta 1, diagonal larga sqrt(3) = mono-pair a nivel k=3).
  - rotación de copias sobre vértice compartido (rot_about).
  - composición spindle: 2 copias + rotación que pone terminales a distancia 1.
  - constructor Moser spindle (7 vértices, 11 aristas, chi=4).
  - ensamblador ciclo-J (N gadgets en ciclo con aristas unidad entre terminales).

Constantes del paper (Notation): eta = exp(i/2·arccos(5/6)),
rho = exp(i·arccos(7/8)); mono-pair 8/3 escalable como n·(8/3)^(m/2).
Aquí se trabaja en floats con tolerancia (patrón /tmp/opencode/parse874.py:
verificación numérica de pares a distancia 1); eta/rho quedan expuestas para
el escalado de niveles superiores.

Grafo = lista de puntos + aristas = TODOS los pares a distancia 1 (tol).
Uso:
  python3 gadF_kit.py moser   # test obligatorio: Moser k3 UNSAT + k4 SAT
  python3 gadF_kit.py cycleJ  # ciclo-J mínimo (2 Mosers abiertos en spindle)
  python3 gadF_kit.py all     # ambos + JSON final por stdout
Solver: /home/diez/.local/bin/kissat (symlink a /usr/local/bin/kissat).
"""

import cmath
import json
import math
import subprocess
import sys
import time

KISSAT = "/home/diez/.local/bin/kissat"
TOL_DEDUP = 1e-9
TOL_UNIT = 1e-6

# --- Multiplicadores de rotación del paper (Notation) ---
ETA = cmath.exp(1j * math.acos(5.0 / 6.0) / 2.0)  # eta = exp(i/2·arccos(5/6))
RHO = cmath.exp(1j * math.acos(7.0 / 8.0))  # rho = exp(i·arccos(7/8))
MONO_BASE = 8.0 / 3.0  # mono-pair de Harder; niveles: n*(8/3)^(m/2)


def mono_scaled(n, m):
    """Longitud de mono-pair escalable del paper: n*(8/3)^(m/2)."""
    return n * (MONO_BASE ** (m / 2.0))


# --- Geometría ---
def rot_about(pt, pivot, ang):
    """Rota pt alrededor de pivot por ángulo ang (radianes)."""
    c, s = math.cos(ang), math.sin(ang)
    dx, dy = pt[0] - pivot[0], pt[1] - pivot[1]
    return (pivot[0] + c * dx - s * dy, pivot[1] + s * dx + c * dy)


def unit_rhombus(theta_deg=60.0):
    """Rombo unidad: O=(0,0), U=(1,0), V=(cos,sin), W=(1+cos,sin).

    Devuelve (pts_dict, sides, mono_pair) donde mono_pair=(O,W).
    Con theta=60°: diagonal corta |U-V|=1, mono-pair |O-W|=sqrt(3),
    forzado al mismo color en todo 3-coloreo propio (dos triángulos
    que comparten la arista corta U-V).
    """
    t = math.radians(theta_deg)
    c, s = math.cos(t), math.sin(t)
    O = (0.0, 0.0)
    U = (1.0, 0.0)
    V = (c, s)
    W = (1.0 + c, s)
    pts = {"O": O, "U": U, "V": V, "W": W}
    sides = [("O", "U"), ("O", "V"), ("U", "W"), ("V", "W"), ("U", "V")]
    return pts, sides, ("O", "W")


def merge_points(named_list):
    """Dedup por tolerancia. Devuelve (coords, index_of)."""
    coords = []
    idx = []
    for _, p in named_list:
        found = None
        for i, q in enumerate(coords):
            if math.hypot(p[0] - q[0], p[1] - q[1]) < TOL_DEDUP:
                found = i
                break
        if found is None:
            coords.append(p)
            idx.append(len(coords) - 1)
        else:
            idx.append(found)
    return coords, idx


def unit_edges(coords, tol=TOL_UNIT):
    """Todas las aristas unidad entre coords (grafo unit-distance honesto)."""
    edges = []
    n = len(coords)
    for i in range(n):
        for j in range(i + 1, n):
            d = math.hypot(coords[i][0] - coords[j][0], coords[i][1] - coords[j][1])
            if abs(d - 1.0) < tol:
                edges.append((i, j))
    return edges


def build_moser(closed=True, theta_deg=60.0):
    """Moser spindle vía kit: 2 rombos que comparten O; el 2do rotado por
    phi tal que los terminales W1,W2 queden a distancia 1.

    Devuelve dict(n, coords, edges, terminals=(t0,t1)).
    Cerrado: 7 vértices, 11 aristas. Abierto (sin arista W1-W2): 7v/10a,
    k3-SAT con terminales forzados al mismo color (mono-pair k=3).
    """
    pts1, _, mono1 = unit_rhombus(theta_deg)
    W1 = pts1["W"]
    r = math.hypot(*W1)  # sqrt(3) con theta=60
    phi = 2.0 * math.asin(1.0 / (2.0 * r))  # 2·r·sin(phi/2) = 1
    O = pts1["O"]
    pts2 = {k: rot_about(p, O, phi) for k, p in unit_rhombus(theta_deg)[0].items()}
    named = [
        ("O", O),
        ("U1", pts1["U"]),
        ("V1", pts1["V"]),
        ("W1", W1),
        ("U2", pts2["U"]),
        ("V2", pts2["V"]),
        ("W2", pts2["W"]),
    ]
    coords, _ = merge_points(named)
    # O es el índice 0 por construcción (primer punto, sin duplicados previos)
    edges = unit_edges(coords)
    if not closed:
        t0 = t1 = None

        # localiza terminales W1, W2 por cercanía a sus coords teóricas
        def find(p):
            best, bd = None, 1e18
            for i, q in enumerate(coords):
                d = math.hypot(p[0] - q[0], p[1] - q[1])
                if d < bd:
                    best, bd = i, d
            assert bd < TOL_DEDUP, (p, bd)
            return best

        t0, t1 = find(W1), find(pts2["W"])
        edges = [
            e
            for e in edges
            if not ((e[0] == t0 and e[1] == t1) or (e[0] == t1 and e[1] == t0))
        ]
    else:

        def find(p):
            best, bd = None, 1e18
            for i, q in enumerate(coords):
                d = math.hypot(p[0] - q[0], p[1] - q[1])
                if d < bd:
                    best, bd = i, d
            assert bd < TOL_DEDUP, (p, bd)
            return best

        t0, t1 = find(W1), find(pts2["W"])
    return {
        "n": len(coords),
        "coords": coords,
        "edges": edges,
        "terminals": (t0, t1),
        "phi": phi,
    }


def build_cycleJ(n_gadgets=2, closed_gadgets=False):
    """Ensamblador ciclo-J: N gadgets Moser en ciclo.

    Con N=2 y gadgets ABIERTOS: identifica Q(A)=P'(B) (vértice compartido)
    y rota B sobre ese vértice hasta que su otro terminal cierre a
    distancia 1 del terminal libre de A. Ciclo: mono1 + arista + mono2
    + arista de cierre (spindle de mono-pairs k=3, cf. Definitions).
    Devuelve dict(n, coords, edges, halves=[A, B]).
    """
    assert n_gadgets == 2, "kit actual: ciclo-J mínimo N=2"
    A = build_moser(closed=closed_gadgets)
    Bloc = build_moser(closed=closed_gadgets)
    PA, QA = A["terminals"]
    pA = A["coords"][PA]
    qA = A["coords"][QA]
    PB, QB = Bloc["terminals"]
    pB = Bloc["coords"][PB]
    qB = Bloc["coords"][QB]
    # 1) trasladar B: P' -> Q(A)
    t = (qA[0] - pB[0], qA[1] - pB[1])
    Bmov = [(x + t[0], y + t[1]) for x, y in Bloc["coords"]]
    d = (qB[0] - pB[0], qB[1] - pB[1])  # |d| = 1
    # 2) rotar B sobre qA: R(psi)·d debe apuntar a un corte de
    #    círculo(qA,1) con círculo(pA,1). |pA-qA| = 1 -> cortes existen.
    mx = (pA[0] + qA[0]) / 2.0
    my = (pA[1] + qA[1]) / 2.0
    h = math.sqrt(max(0.0, 1.0 - 0.25 * ((pA[0] - qA[0]) ** 2 + (pA[1] - qA[1]) ** 2)))
    vx, vy = qA[0] - pA[0], qA[1] - pA[1]
    L = math.hypot(vx, vy)
    px, py = -vy / L, vx / L
    S1 = (mx + h * px, my + h * py)
    S2 = (mx - h * px, my - h * py)
    base_ang = math.atan2(d[1], d[0])
    for S in (S1, S2):
        u = (S[0] - qA[0], S[1] - qA[1])
        psi = math.atan2(u[1], u[0]) - base_ang
        Brot = [rot_about(p, qA, psi) for p in Bmov]
        named = [("a%d" % i, p) for i, p in enumerate(A["coords"])]
        named += [("b%d" % i, p) for i, p in enumerate(Brot)]
        coords, _ = merge_points(named)
        if len(coords) == A["n"] + Bloc["n"] - 1:  # solo Q compartido
            edges = unit_edges(coords)
            return {
                "n": len(coords),
                "coords": coords,
                "edges": edges,
                "halves": (A, Bloc),
                "psi": psi,
            }
    raise RuntimeError("ciclo-J: colisión de vértices en ambos cortes")


# --- CNF k-coloración + kissat ---
def to_cnf(n, edges, k, extra=None):
    """CNF estándar: >=1 color por vértice, <=1 color, aristas bicolor."""
    clauses = []
    for v in range(n):
        clauses.append([(v * k + c + 1) for c in range(k)])
        for c1 in range(k):
            for c2 in range(c1 + 1, k):
                clauses.append([-(v * k + c1 + 1), -(v * k + c2 + 1)])
    for u, v in edges:
        for c in range(k):
            clauses.append([-(u * k + c + 1), -(v * k + c + 1)])
    if extra:
        clauses.extend(extra)
    nv = n * k
    lines = ["p cnf %d %d" % (nv, len(clauses))]
    for cl in clauses:
        lines.append(" ".join(map(str, cl)) + " 0")
    return "\n".join(lines) + "\n"


def kissat_run(cnf_text, tag):
    """Corre kissat sobre CNF en memoria. Devuelve (SAT|UNSAT, segundos)."""
    path = "/tmp/opencode/gadF_%s.cnf" % tag
    with open(path, "w") as f:
        f.write(cnf_text)
    t0 = time.perf_counter()
    p = subprocess.run(
        [KISSAT, "-q", path], capture_output=True, text=True, timeout=300
    )
    dt = time.perf_counter() - t0
    out = (p.stdout or "") + (p.stderr or "")
    if "UNSATISFIABLE" in out or p.returncode == 20:
        return "UNSAT", dt
    if "SATISFIABLE" in out or p.returncode == 10:
        return "SAT", dt
    raise RuntimeError("kissat salida inesperada rc=%d: %s" % (p.returncode, out[:500]))


def check(n, edges, k, tag, extra=None):
    return kissat_run(to_cnf(n, edges, k, extra), tag)


def test_moser():
    """TEST OBLIGATORIO: Moser spindle reconstruido con el kit:
    k3 UNSAT + k4 SAT. Además forcing: abierto k3 SAT pero con
    terminales distintos -> UNSAT (prueba de mono-pair k=3)."""
    M = build_moser(closed=True)
    assert M["n"] == 7, M["n"]
    assert len(M["edges"]) == 11, len(M["edges"])
    t0, t1 = M["terminals"]
    d = math.hypot(
        M["coords"][t0][0] - M["coords"][t1][0], M["coords"][t0][1] - M["coords"][t1][1]
    )
    assert abs(d - 1.0) < TOL_UNIT, d
    r3, dt3 = check(M["n"], M["edges"], 3, "moser_k3")
    r4, dt4 = check(M["n"], M["edges"], 4, "moser_k4")
    # forcing del mono-pair: Moser abierto solo, k3 SAT...
    O = build_moser(closed=False)
    assert O["n"] == 7 and len(O["edges"]) == 10, (O["n"], len(O["edges"]))
    o0, o1 = O["terminals"]
    ro, dto = check(O["n"], O["edges"], 3, "moser_open_k3")
    # ...pero terminales forzados a distinto color -> UNSAT
    diff = []
    for c in range(3):
        diff.append([-(o0 * 3 + c + 1), -(o1 * 3 + c + 1)])
    rf, dtf = check(O["n"], O["edges"], 3, "moser_open_k3_diff", extra=diff)
    return {
        "n": M["n"],
        "aristas": len(M["edges"]),
        "k3": r3,
        "t_k3": round(dt3, 3),
        "k4": r4,
        "t_k4": round(dt4, 3),
        "forcing_abierto_k3": ro,
        "forcing_dif_color": rf,
        "t_forcing": round(dtf, 3),
    }


def test_cycleJ():
    """Ciclo-J mínimo: 2 Mosers abiertos en spindle (13v). k3/k4 reales."""
    J = build_cycleJ(2, closed_gadgets=False)
    assert J["n"] == 13, J["n"]
    A, B = J["halves"]
    # mitades sanas: cada Moser abierto k3-SAT por separado
    ra, _ = check(A["n"], A["edges"], 3, "cycleJ_halfA_k3")
    rb, _ = check(B["n"], B["edges"], 3, "cycleJ_halfB_k3")
    r3, dt3 = check(J["n"], J["edges"], 3, "cycleJ_k3")
    r4, dt4 = check(J["n"], J["edges"], 4, "cycleJ_k4")
    return {
        "n": J["n"],
        "aristas": len(J["edges"]),
        "mitadA_k3": ra,
        "mitadB_k3": rb,
        "k3": r3,
        "t_k3": round(dt3, 3),
        "k4": r4,
        "t_k4": round(dt4, 3),
    }


def main():
    mode = sys.argv[1] if len(sys.argv) > 1 else "all"
    if mode == "moser":
        print(json.dumps(test_moser(), indent=1))
    elif mode == "cycleJ":
        print(json.dumps(test_cycleJ(), indent=1))
    elif mode == "all":
        m = test_moser()
        j = test_cycleJ()
        ok = m["k3"] == "UNSAT" and m["k4"] == "SAT"
        print(json.dumps({"moser": m, "cicloJ": j, "kit_ok": ok}, indent=1))
    else:
        sys.exit("modo: moser | cycleJ | all")


if __name__ == "__main__":
    main()
