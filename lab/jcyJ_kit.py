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
    elif mode == "j1":
        print(json.dumps(test_j1_program(), indent=1))
    else:
        sys.exit("modo: moser | cycleJ | all | j1")


# (trigger al final del archivo)

# Agente J1, 2026-09-21. Solo /tmp/opencode, prefijo jcyJ_.
#
# Paper (verificado contra /tmp/opencode/jcyJ_paper.txt):
#  - Mono-pair: dos vértices con el mismo color en todo k-coloreo propio
#    (default k=4; aquí nivel k=3 por escala del gadget, honesto).
#  - Spindle: ciclo de dos mono-pairs + una arista unidad (línea 106).
#  - Eta/rho: eta = exp((i/2)arccos(5/6)), rho = exp(i arccos(7/8))
#    (línea 146). Rotaciones por múltiplos de eta: G{a} (Notation).
#  - Type J: 2+ base graphs con mono-pair (k=4) conectados en ciclo
#    mono-pairs + 1 arista unidad -> 5-cromático (líneas 363-367).
#  - Teorema de escalado (líneas 368-374): mono-pair de longitud r/sqrt(3)
#    desde dos mono-pairs de longitud r rotando uno sobre vértice común
#    por eta; todo queda en Q[sqrt(3), sqrt(11)].
#    Geometría exacta usada aquí: rotación por 2·arg(eta) = arccos(5/6)
#    (familia eta^a del paper) sobre un extremo común da cuerda
#    2·r·sin(arg eta) = r/sqrt(3), pues sin(arg eta) = 1/(2 sqrt(3)).
#    (cos(2a)=5/6 -> sin^2(a)=(1-5/6)/2=1/12.)
#  - Identidad 8/3 vía eta (derivación propia): |1+eta^2+eta^4| = 8/3,
#    pues |z|^2 = 3+4cos t+2cos 2t con cos t=5/6, cos 2t=7/18 da 64/9.
#  - El 8/3-mono-pair del paper vive en (+-4/3,0) (línea 769) y sus
#    grafos mínimos reales tienen 367+ vértices (G367, línea 1878):
#    un forcing a nivel k=4 está fuera del alcance de 60 min -> aquí
#    nivel k=3 honesto (el paper también usa k=3 en piezas base:
#    rombos/ruedas hexagonales).
# Gadgets nuevos:
#  - build_double_rhombus_83: dos rombos unidad (mono-pair diagonal
#    sqrt(3) a nivel 3) compartiendo O, rotados por phi_83 con
#    2 sqrt(3) sin(phi/2) = 8/3 -> terminales W1,W2 a 8/3, ambos
#    forzados a color(O) en todo 3-coloreo. 7v/10a.
#  - scale_mono_pair_eta: dos copias de un mono-pair-r compartiendo un
#    extremo, una rotada por +-arccos(5/6) -> nuevos terminales a
#    r/sqrt(3), ambos forzados a color(común). Abierto (sin arista
#    terminal-terminal).
#  - scale_mono_pair_rho: idem por arccos(7/8) -> cuerda r/2
#    (2r sin(b/2) = r/2 pues sin^2(b/2) = (1-7/8)/2 = 1/16).
# =====================================================================

ALPHA_ETA = math.acos(5.0 / 6.0) / 2.0  # arg(eta) ~= 0.2928 rad
PSI_ETA2 = math.acos(5.0 / 6.0)  # 2 arg(eta) ~= 33.56°
BETA_RHO = math.acos(7.0 / 8.0)  # arg(rho) ~= 28.96°
PHI_83 = 2.0 * math.asin(4.0 / (3.0 * math.sqrt(3.0)))  # ~= 100.66°
R83 = 8.0 / 3.0


def rot_eta_about(pt, pivot):
    """Rota pt sobre pivot multiplicando por eta (paper Notation)."""
    z = complex(pt[0] - pivot[0], pt[1] - pivot[1]) * ETA
    return (pivot[0] + z.real, pivot[1] + z.imag)


def rot_rho_about(pt, pivot):
    """Rota pt sobre pivot multiplicando por rho (paper Notation)."""
    z = complex(pt[0] - pivot[0], pt[1] - pivot[1]) * RHO
    return (pivot[0] + z.real, pivot[1] + z.imag)


def jkissat_run(cnf_text, tag):
    """kissat con prefijo jcyJ_ (disciplina de la misión)."""
    path = "/tmp/opencode/jcyJ_%s.cnf" % tag
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
    raise RuntimeError("kissat rc=%d: %s" % (p.returncode, out[:500]))


def jcheck(n, edges, k, tag, extra=None):
    return jkissat_run(to_cnf(n, edges, k, extra), tag)


def contrary_clause(u, v, k):
    """Cláusula contraria: u y v con distinto color (u != v)."""
    return [[-(u * k + c + 1), -(v * k + c + 1)] for c in range(k)]


def mono_dist(g, terminals=None):
    t = terminals or g["terminals"]
    a, b = g["coords"][t[0]], g["coords"][t[1]]
    return math.hypot(a[0] - b[0], a[1] - b[1])


def build_double_rhombus_83():
    """Doble rombo: mono-pair de longitud 8/3 a nivel k=3.

    Rombo unidad 60°: diagonal O-W = sqrt(3) forzada monocroma en todo
    3-coloreo (dos triángulos comparten la arista corta U-V). Dos copias
    comparten O; copia 2 rotada por PHI_83 -> |W1-W2| = 8/3 exacto
    (a error float). W1, W2 ambos = color(O): mono-pair 8/3.
    """
    pts1, _, _ = unit_rhombus(60.0)
    O = pts1["O"]
    pts2 = {k: rot_about(p, O, PHI_83) for k, p in pts1.items()}
    named = [
        ("O", O),
        ("U1", pts1["U"]),
        ("V1", pts1["V"]),
        ("W1", pts1["W"]),
        ("U2", pts2["U"]),
        ("V2", pts2["V"]),
        ("W2", pts2["W"]),
    ]
    coords, _ = merge_points(named)

    def find(p):
        best, bd = None, 1e18
        for i, q in enumerate(coords):
            d = math.hypot(p[0] - q[0], p[1] - q[1])
            if d < bd:
                best, bd = i, d
        assert bd < TOL_DEDUP, (p, bd)
        return best

    t0, t1 = find(pts1["W"]), find(pts2["W"])
    edges = unit_edges(coords)
    return {
        "n": len(coords),
        "coords": coords,
        "edges": edges,
        "terminals": (t0, t1),
        "r": R83,
        "level": 3,
        "phi": PHI_83,
    }


def _scaled_from_pair(g, ang, share, out):
    """Núcleo del escalado: 2 copias comparten vértice `share`; la copia B
    rota por `ang` sobre él; nuevos terminales = `out` de cada copia."""
    A = g["coords"]
    T = A[share]
    Bmov = [rot_about(p, T, ang) for p in A]
    named = [("a%d" % i, p) for i, p in enumerate(A)]
    named += [("b%d" % i, p) for i, p in enumerate(Bmov)]
    coords, _ = merge_points(named)
    if len(coords) != 2 * len(A) - 1:
        return None  # colisión de vértices con este signo

    def find(p):
        best, bd = None, 1e18
        for i, q in enumerate(coords):
            d = math.hypot(p[0] - q[0], p[1] - q[1])
            if d < bd:
                best, bd = i, d
        assert bd < TOL_DEDUP, (p, bd)
        return best

    qA = find(A[out])
    # localiza Q_B rotando el punto teórico igual que la copia B
    qB = find(rot_about(A[out], T, ang))
    edges = unit_edges(coords)
    # abierto: sin arista terminal-terminal (como build_moser(open))
    edges = [
        e
        for e in edges
        if not ((e[0] == qA and e[1] == qB) or (e[0] == qB and e[1] == qA))
    ]
    return {"n": len(coords), "coords": coords, "edges": edges, "terminals": (qA, qB)}


def scale_mono_pair_eta(g, share=None, out=None):
    """Escalado eta del paper: r -> r/sqrt(3).

    Dos copias del mono-pair comparten el extremo `share`; una se rota
    por +-2 arg(eta) = +-arccos(5/6) (familia eta^a). Cuerda entre los
    extremos libres: 2 r sin(arg eta) = r/sqrt(3). Ambos libres forzados
    a color(común): mono-pair escalado (mismo nivel k).
    Prueba +psi y luego -psi (primer merge limpio 2n-1 gana).
    """
    t0, t1 = g["terminals"]
    share = t0 if share is None else share
    out = t1 if out is None else out
    for ang in (PSI_ETA2, -PSI_ETA2):
        r = _scaled_from_pair(g, ang, share, out)
        if r is not None:
            r["r"] = mono_dist(g) / math.sqrt(3.0)
            r["level"] = g.get("level", 3)
            r["psi"] = ang
            return r
    raise RuntimeError("scale_eta: colisión en ambos signos")


def scale_mono_pair_rho(g, share=None, out=None):
    """Escalado rho: r -> r/2 (cuerda 2r sin(beta/2) = r/2)."""
    t0, t1 = g["terminals"]
    share = t0 if share is None else share
    out = t1 if out is None else out
    for ang in (BETA_RHO, -BETA_RHO):
        r = _scaled_from_pair(g, ang, share, out)
        if r is not None:
            r["r"] = mono_dist(g) / 2.0
            r["level"] = g.get("level", 3)
            r["psi"] = ang
            return r
    raise RuntimeError("scale_rho: colisión en ambos signos")


def build_cycleJ_generic(make_open, r_term):
    """Ciclo-J genérico: 2 gadgets abiertos (terminales a distancia r_term)
    en ciclo spindle: Q(A)=P(B) compartido, Q(B) rotado hasta el corte de
    círculo(P_A,1) con círculo(compartido, r_term); arista unidad de cierre
    P_A-Q(B). Con r_term=1 reproduce build_cycleJ del kit."""
    A = make_open()
    Bloc = make_open()
    PA, QA = A["terminals"]
    pA = A["coords"][PA]
    qA = A["coords"][QA]
    PB, QB = Bloc["terminals"]
    dvec = (
        Bloc["coords"][QB][0] - Bloc["coords"][PB][0],
        Bloc["coords"][QB][1] - Bloc["coords"][PB][1],
    )
    assert abs(math.hypot(*dvec) - r_term) < 1e-6, (math.hypot(*dvec), r_term)
    t = (qA[0] - Bloc["coords"][PB][0], qA[1] - Bloc["coords"][PB][1])
    Bmov = [(x + t[0], y + t[1]) for x, y in Bloc["coords"]]
    # corte círculo(pA,1) / círculo(qA, r_term); centros a distancia r_term
    d = r_term
    ux, uy = (pA[0] - qA[0]) / d, (pA[1] - qA[1]) / d
    a = (r_term**2 - 1.0 + d**2) / (2.0 * d)
    h = math.sqrt(max(0.0, r_term**2 - a**2))
    M = (qA[0] + a * ux, qA[1] + a * uy)
    px, py = -uy, ux
    base_ang = math.atan2(dvec[1], dvec[0])
    for S in ((M[0] + h * px, M[1] + h * py), (M[0] - h * px, M[1] - h * py)):
        u = (S[0] - qA[0], S[1] - qA[1])
        psi = math.atan2(u[1], u[0]) - base_ang
        Brot = [rot_about(p, qA, psi) for p in Bmov]
        named = [("a%d" % i, p) for i, p in enumerate(A["coords"])]
        named += [("b%d" % i, p) for i, p in enumerate(Brot)]
        coords, _ = merge_points(named)
        if len(coords) == A["n"] + Bloc["n"] - 1:
            edges = unit_edges(coords)
            return {
                "n": len(coords),
                "coords": coords,
                "edges": edges,
                "halves": (A, Bloc),
                "psi": psi,
                "S": S,
            }
    raise RuntimeError("ciclo-J genérico: colisión en ambos cortes")


def rhombus_gadget():
    """Rombo unidad como gadget mono-pair: terminales (O, W), r = sqrt(3),
    nivel 3. Compartir O en el escalado reproduce el Moser abierto."""
    pts, _, _ = unit_rhombus(60.0)
    order = ["O", "U", "V", "W"]
    coords = [pts[k] for k in order]
    return {
        "n": 4,
        "coords": coords,
        "edges": unit_edges(coords),
        "terminals": (0, 3),
        "r": math.sqrt(3.0),
        "level": 3,
    }


def test_j1_program():
    """Misión J1: forcing del 8/3 y su escalado eta + ciclo-J mínimo."""
    out = {}
    # T1: rombo solo (pieza base): O=W en todo 3-coloreo
    pts, _, _ = unit_rhombus(60.0)
    rc = list(pts.values())
    ec = unit_edges(rc)
    idx = {k: i for i, k in enumerate(pts.keys())}
    r_base, _ = jcheck(4, ec, 3, "j1_rhombus_k3")
    r_f, dtf = jcheck(
        4, ec, 3, "j1_rhombus_k3_diff", extra=contrary_clause(idx["O"], idx["W"], 3)
    )
    out["T1_rombo"] = {
        "n": 4,
        "aristas": len(ec),
        "k3": r_base,
        "dif_color": r_f,
        "t": round(dtf, 3),
    }
    # T2: doble rombo 8/3
    G = build_double_rhombus_83()
    d83 = mono_dist(G)
    assert G["n"] == 7, G["n"]
    assert len(G["edges"]) == 10, len(G["edges"])
    assert abs(d83 - R83) < 1e-9, d83
    g0, g1 = G["terminals"]
    rb, dtb = jcheck(G["n"], G["edges"], 3, "j1_83_k3")
    rf, dtf = jcheck(
        G["n"], G["edges"], 3, "j1_83_k3_diff", extra=contrary_clause(g0, g1, 3)
    )
    out["T2_mono83"] = {
        "n": G["n"],
        "aristas": len(G["edges"]),
        "r": d83,
        "k3": rb,
        "dif_color": rf,
        "t": round(dtf, 3),
    }
    # T3: escalado eta del rombo sqrt(3) -> 1 (compartiendo O).
    # El ángulo del kit (Moser: 2 asin(1/2sqrt(3))) ES arccos(5/6): el
    # Moser abierto es el rombo escalado por eta. Prueba del constructor.
    R = rhombus_gadget()
    S = scale_mono_pair_eta(R)
    dS = mono_dist(S)
    assert S["n"] == 7, S["n"]
    assert abs(dS - 1.0) < 1e-9, dS
    assert len(S["edges"]) == 10, len(S["edges"])
    s0, s1 = S["terminals"]
    sb, _ = jcheck(S["n"], S["edges"], 3, "j1_eta_k3")
    sf, sft = jcheck(
        S["n"], S["edges"], 3, "j1_eta_k3_diff", extra=contrary_clause(s0, s1, 3)
    )
    out["T3_escalado_eta"] = {
        "base": "rombo",
        "n": S["n"],
        "aristas": len(S["edges"]),
        "r": dS,
        "r_teorico": 1.0,
        "psi": S["psi"],
        "k3": sb,
        "dif_color": sf,
        "t": round(sft, 3),
    }
    # T3b: escalado rho del 8/3 -> 4/3 (compartiendo terminal W1).
    # Obstrucción documentada (no geométrica sino de forcing): con el
    # escalado eta sobre terminal, los centros O_A,O_B quedan a cuerda
    # 2 sqrt(3) sin(arg eta) = 1 EXACTA -> arista O_A-O_B que exige
    # O_A != O_B mientras T fuerza color(O_A) = color(O_B): base UNSAT
    # vacua (medido: 13v/21a k3 UNSAT). Con rho: cuerda O = sqrt(3)/2.
    S2 = scale_mono_pair_rho(G)
    dS2 = mono_dist(S2)
    assert S2["n"] == 13, S2["n"]
    assert abs(dS2 - R83 / 2.0) < 1e-9, (dS2, R83 / 2.0)
    q0, q1 = S2["terminals"]
    hb, _ = jcheck(S2["n"], S2["edges"], 3, "j1_rho_k3")
    hf, hft = jcheck(
        S2["n"], S2["edges"], 3, "j1_rho_k3_diff", extra=contrary_clause(q0, q1, 3)
    )
    out["T3b_escalado_rho"] = {
        "base": "doble-rombo-83",
        "n": S2["n"],
        "aristas": len(S2["edges"]),
        "r": dS2,
        "r_teorico": R83 / 2.0,
        "psi": S2["psi"],
        "k3": hb,
        "dif_color": hf,
        "t": round(hft, 3),
    }
    out["forcing_ok"] = (
        r_f == "UNSAT"
        and rf == "UNSAT"
        and sf == "UNSAT"
        and hf == "UNSAT"
        and r_base == "SAT"
        and rb == "SAT"
        and sb == "SAT"
        and hb == "SAT"
    )

    # T4: ciclo-J mínimo con el gadget escalado (2 mitades + cierre 1)
    def make_open():
        return scale_mono_pair_rho(build_double_rhombus_83())

    try:
        J = build_cycleJ_generic(make_open, R83 / 2.0)
        A, B = J["halves"]
        ra, _ = jcheck(A["n"], A["edges"], 3, "j1_cyc_halfA_k3")
        rb2, _ = jcheck(B["n"], B["edges"], 3, "j1_cyc_halfB_k3")
        r3, t3 = jcheck(J["n"], J["edges"], 3, "j1_cyc_k3")
        r4, t4 = jcheck(J["n"], J["edges"], 4, "j1_cyc_k4")
        r5, t5 = jcheck(J["n"], J["edges"], 5, "j1_cyc_k5")
        out["T4_cicloJ"] = {
            "n": J["n"],
            "aristas": len(J["edges"]),
            "mitadA_k3": ra,
            "mitadB_k3": rb2,
            "k3": r3,
            "t_k3": round(t3, 3),
            "k4": r4,
            "t_k4": round(t4, 3),
            "k5": r5,
            "t_k5": round(t5, 3),
        }
    except RuntimeError as e:
        out["T4_cicloJ"] = {"error": str(e)}
    return out


if __name__ == "__main__":
    main()
