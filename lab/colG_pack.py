#!/usr/bin/env python3
"""colG_pack.py — programa neuronal Hadwiger-Nelson, agente G (neural-escala).

Modelo: logits por vertice (N x k) = P(color|v). Loss = media sobre aristas
de P(mismo color) = sum_c p_ic * p_jc. Adam la minimiza.
- Grafo k-coloreable => loss puede llegar a ~0 (heuristico, NO prueba).
- No k-coloreable => loss queda en piso >0 (analogo UNSAT, NO certificado).
Valor real: BUSQUEDA rapida de coloreos/violaciones, no certificados.

Autocontenido: solo torch + stdlib (argparse, json, math, sys, time).
Sin numpy, sin scipy, sin dependencias externas.

Modos:
  --demo : 3 demos de validacion (triangulo k2/k3 + cuadrado C4 k2).
  --points P.json [--edges E.edge] --k K : modo escala (restarts -> JSONL).

Ejemplos:
  python colG_pack.py --demo
  python colG_pack.py --points 509_parts.vtx.json --k 5 --restarts 32 \\
      --steps 20000 --entropy 0.05 --entropy-schedule cosine --out colG_509k5.jsonl
"""

import argparse
import json
import math
import sys
import time

import torch

TOL_NUMERIC_EDGE = 1e-9


# ---------------------------------------------------------------- I/O
def load_points(path):
    with open(path) as f:
        data = json.load(f)
    return torch.tensor(data, dtype=torch.float64)


def load_edges_file(path):
    """Formato .edge: lineas 'e a b' (1-based, como DIMACS) o 'a b' (1-based).
    Devuelve lista de (i, j) 0-based, sin duplicados ni autoloops."""
    edges = []
    seen = set()
    with open(path) as f:
        for line in f:
            p = line.split()
            if not p or p[0].startswith(("c", "p", "#")):
                continue
            if p[0] == "e" and len(p) >= 3:
                a, b = int(p[1]) - 1, int(p[2]) - 1
            elif len(p) >= 2:
                try:
                    a, b = int(p[0]) - 1, int(p[1]) - 1
                except ValueError:
                    continue
            else:
                continue
            if a == b:
                continue
            e = (min(a, b), max(a, b))
            if e not in seen:
                seen.add(e)
                edges.append(e)
    return edges


def numeric_edges(pts, tol=TOL_NUMERIC_EDGE):
    """Aristas unitarias por distancia euclidea |d-1|<tol. GPU-ok (cdist)."""
    with torch.no_grad():
        d = torch.cdist(pts, pts)
        n = pts.shape[0]
        ii, jj = torch.triu_indices(n, n, 1)
        m = (d[ii, jj] - 1.0).abs() < tol
    return list(zip(ii[m].tolist(), jj[m].tolist()))


# ------------------------------------------------------- validacion
def validate_coloring(colors, edges):
    """Verificacion arista por arista. Devuelve (violated, viol_list)."""
    viol_list = [(int(a), int(b)) for (a, b) in edges if colors[a] == colors[b]]
    return len(viol_list), viol_list


# ------------------------------------------------------- entrenamiento
def entropy_beta(entropy0, step, steps, schedule):
    if entropy0 <= 0:
        return 0.0
    if schedule == "cosine":
        # Decaimiento coseno: entropy0 -> 0 en [1, steps]
        return entropy0 * 0.5 * (1.0 + math.cos(math.pi * step / steps))
    if schedule == "none":
        return entropy0
    raise ValueError(f"entropy-schedule desconocido: {schedule}")


def train(
    pts, edges, k, steps, lr, seed, device, log_every, entropy0=0.0, schedule="cosine"
):
    """Una corrida. Devuelve dict con loss_best (piso de P(mismo color)),
    violated (conteo arista por arista), colors, time_s."""
    g = torch.Generator(device="cpu").manual_seed(seed)
    n = pts.shape[0]
    logits = torch.randn(
        n, k, dtype=torch.float64, generator=g, requires_grad=True, device=device
    )
    ei = torch.tensor([e[0] for e in edges], device=device)
    ej = torch.tensor([e[1] for e in edges], device=device)
    opt = torch.optim.Adam([logits], lr=lr)
    best = float("inf")
    t0 = time.time()
    for s in range(1, steps + 1):
        opt.zero_grad()
        p = torch.softmax(logits, dim=1)
        same = (p[ei] * p[ej]).sum(dim=1).mean()
        beta = entropy_beta(entropy0, s, steps, schedule)
        if beta > 0:
            ent = -(p * (p + 1e-12).log()).sum(dim=1).mean()
            loss = same - beta * ent
        else:
            loss = same
        loss.backward()
        opt.step()
        v = same.item()
        if v < best:
            best = v
        if log_every and s % log_every == 0:
            print(
                f"step {s}/{steps} loss={v:.6f} best={best:.6f} beta={beta:.4f}",
                file=sys.stderr,
            )
    with torch.no_grad():
        hard = torch.softmax(logits, dim=1).argmax(dim=1).cpu().tolist()
        violated, viol_list = validate_coloring(hard, edges)
    return {
        "loss_best": best,
        "violated": violated,
        "colors": hard,
        "viol_edges": viol_list,
        "time_s": time.time() - t0,
    }


# ------------------------------------------------------- demos
def demo_graphs():
    s3 = math.sqrt(3) / 2
    tri = torch.tensor([[0.0, 0.0], [1.0, 0.0], [0.5, s3]], dtype=torch.float64)
    tri_edges = [(0, 1), (0, 2), (1, 2)]
    sq = torch.tensor(
        [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]], dtype=torch.float64
    )
    sq_edges = [(0, 1), (1, 2), (2, 3), (3, 0)]
    return [
        {
            "name": "triangle_k2",
            "pts": tri,
            "edges": tri_edges,
            "k": 2,
            "exp_viol": 1,
            "loss_lo": 0.25,
            "loss_hi": 0.42,
        },
        {
            "name": "triangle_k3",
            "pts": tri,
            "edges": tri_edges,
            "k": 3,
            "exp_viol": 0,
            "loss_lo": 0.0,
            "loss_hi": 0.02,
        },
        {
            "name": "squareC4_k2",
            "pts": sq,
            "edges": sq_edges,
            "k": 2,
            "exp_viol": 0,
            "loss_lo": 0.0,
            "loss_hi": 0.02,
        },
    ]


def run_demos(steps, lr, seed, device, log_every):
    results = []
    ok_all = True
    for d in demo_graphs():
        r = train(d["pts"], d["edges"], d["k"], steps, lr, seed, device, log_every)
        ok = (
            r["violated"] == d["exp_viol"]
            and d["loss_lo"] <= r["loss_best"] <= d["loss_hi"]
        )
        ok_all = ok_all and ok
        results.append(
            {
                "demo": d["name"],
                "k": d["k"],
                "loss_best": round(r["loss_best"], 6),
                "violated": r["violated"],
                "expected_violated": d["exp_viol"],
                "pass": ok,
                "time_s": round(r["time_s"], 2),
            }
        )
        print(
            f"[demo] {d['name']}: loss_best={r['loss_best']:.6f} "
            f"violated={r['violated']} (esperado {d['exp_viol']}) "
            f"-> {'PASS' if ok else 'FAIL'}",
            file=sys.stderr,
        )
    return results, ok_all


# ------------------------------------------------------- main
def main():
    ap = argparse.ArgumentParser(description="colG: coloracion neuronal HN")
    ap.add_argument(
        "--demo", action="store_true", help="3 demos de validacion (tri k2/k3 + C4 k2)"
    )
    ap.add_argument("--points", default=None, help="JSON [[x,y],...]")
    ap.add_argument(
        "--edges", default=None, help=".edge (1-based); si falta, numeric tol 1e-9"
    )
    ap.add_argument("--k", type=int, default=3)
    ap.add_argument("--steps", type=int, default=3000)
    ap.add_argument("--lr", type=float, default=0.15)
    ap.add_argument("--seed", type=int, default=7)
    ap.add_argument("--device", default="auto", help="auto|cpu|cuda")
    ap.add_argument("--log_every", type=int, default=0)
    ap.add_argument("--restarts", type=int, default=1)
    ap.add_argument(
        "--entropy",
        type=float,
        default=0.0,
        help="beta inicial de entropia (0 = sin entropia)",
    )
    ap.add_argument(
        "--entropy-schedule",
        default="cosine",
        choices=["none", "cosine"],
        help="decaimiento de beta: cosine (defecto) o none (constante)",
    )
    ap.add_argument(
        "--out",
        default=None,
        help="JSONL: una linea por restart {restart,seed,loss_best,violated,time_s}",
    )
    ap.add_argument(
        "--colors-out",
        default=None,
        help="JSON con el mejor coloreo {colors, violated, viol_edges}",
    )
    a = ap.parse_args()

    dev = a.device
    if dev == "auto":
        dev = "cuda" if torch.cuda.is_available() else "cpu"

    if a.demo:
        results, ok = run_demos(a.steps, a.lr, a.seed, dev, a.log_every)
        print(
            json.dumps(
                {
                    "device": dev,
                    "steps": a.steps,
                    "lr": a.lr,
                    "seed": a.seed,
                    "results": results,
                }
            )
        )
        sys.exit(0 if ok else 1)

    if not a.points:
        ap.error("--demo o --points es obligatorio")

    pts = load_points(a.points).to(dev)
    edges = load_edges_file(a.edges) if a.edges else numeric_edges(pts)
    if not edges:
        print(json.dumps({"error": "grafo sin aristas"}))
        sys.exit(2)

    t_all = time.time()
    rows = []
    best = None
    for r in range(a.restarts):
        tr = train(
            pts,
            edges,
            a.k,
            a.steps,
            a.lr,
            a.seed + r,
            dev,
            a.log_every,
            a.entropy,
            a.entropy_schedule,
        )
        row = {
            "restart": r,
            "seed": a.seed + r,
            "loss_best": tr["loss_best"],
            "violated": tr["violated"],
            "time_s": round(tr["time_s"], 2),
        }
        rows.append(row)
        if best is None or (tr["violated"], tr["loss_best"]) < (
            best["violated"],
            best["loss_best"],
        ):
            best = {"restart": r, **tr}
        print(
            f"[restart {r}] loss_best={tr['loss_best']:.6f} "
            f"violated={tr['violated']} time={tr['time_s']:.1f}s",
            file=sys.stderr,
        )

    if a.out:
        with open(a.out, "w") as f:
            for row in rows:
                f.write(json.dumps(row) + "\n")

    # Re-validacion arista por arista del mejor coloreo (doble puerta)
    re_viol, re_list = validate_coloring(best["colors"], edges)
    assert re_viol == best["violated"], "inconsistencia en validacion final"

    if a.colors_out:
        with open(a.colors_out, "w") as f:
            json.dump(
                {"colors": best["colors"], "violated": re_viol, "viol_edges": re_list},
                f,
            )

    print(
        json.dumps(
            {
                "device": dev,
                "n": pts.shape[0],
                "edges": len(edges),
                "k": a.k,
                "steps": a.steps,
                "lr": a.lr,
                "seed": a.seed,
                "restarts": a.restarts,
                "entropy": a.entropy,
                "entropy_schedule": a.entropy_schedule,
                "best_restart": best["restart"],
                "loss_best": best["loss_best"],
                "violated": re_viol,
                "out": a.out,
                "time_s": round(time.time() - t_all, 2),
            }
        )
    )


if __name__ == "__main__":
    main()
