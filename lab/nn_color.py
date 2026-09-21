#!/usr/bin/env python3
"""Coloracion probabilistica neuronal, nucleo estilo 2024 (arXiv 2404.05509).

Modelo: logits por vertice (N x k) = P(color|v). Loss = media sobre aristas
unitarias de P(mismo color) = sum_c p_ic * p_jc. SGD la minimiza.
- Grafo k-coloreable => el loss puede llegar a ~0 (heuristico, no prueba).
- No k-coloreable => el loss queda en piso >0 (analogo UNSAT, no certificado).
Valor real: BUSQUEDA rapida de coloreos/violaciones, no certificados.
Corre en CPU o T4 (--device cuda). TPU requeriria port JAX (no incluido).

Uso:
  nn_color.py --demo                        # triangulo k=2 (piso>0) y k=3 (->0)
  nn_color.py --points P.json --k 5         # aristas numericas tol 1e-9 (GPU ok)
  nn_color.py --points P.json --edges E.edge --k 4   # aristas de archivo
Imprime UNA linea JSON a stdout (Colab-friendly); detalle a stderr.
"""

import argparse
import json
import math
import sys
import time

import torch


def load_points(path):
    return torch.tensor(json.load(open(path)), dtype=torch.float64)


def load_edges_file(path):
    edges = []
    with open(path) as f:
        for line in f:
            p = line.split()
            if p and p[0] == "e":
                a, b = int(p[1]) - 1, int(p[2]) - 1
                edges.append((min(a, b), max(a, b)))
    return edges


def numeric_edges(pts, tol=1e-9):
    with torch.no_grad():
        d = torch.cdist(pts, pts)
        n = pts.shape[0]
        ii, jj = torch.triu_indices(n, n, 1)
        m = (d[ii, jj] - 1.0).abs() < tol
    return list(zip(ii[m].tolist(), jj[m].tolist()))


def train(pts, edges, k, steps, lr, seed, device, log_every, entropy0=0.0):
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
        loss = same
        if entropy0 > 0:
            beta = entropy0 * (1.0 - s / steps)
            ent = -(p * (p + 1e-12).log()).sum(dim=1).mean()
            loss = loss - beta * ent
        loss.backward()
        opt.step()
        v = same.item()
        if v < best:
            best = v
        if log_every and s % log_every == 0:
            print(f"step {s}/{steps} loss={v:.6f} best={best:.6f}", file=sys.stderr)
    with torch.no_grad():
        hard = torch.softmax(logits, dim=1).argmax(dim=1)
        viol = int((hard[ei] == hard[ej]).sum())
    return best, viol, time.time() - t0


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--demo", action="store_true")
    ap.add_argument("--points", default=None)
    ap.add_argument("--edges", default=None)
    ap.add_argument("--k", type=int, default=3)
    ap.add_argument("--steps", type=int, default=3000)
    ap.add_argument("--lr", type=float, default=0.15)
    ap.add_argument("--seed", type=int, default=7)
    ap.add_argument("--device", default="auto")
    ap.add_argument("--log_every", type=int, default=0)
    ap.add_argument("--restarts", type=int, default=1)
    ap.add_argument("--entropy", type=float, default=0.0)
    a = ap.parse_args()
    dev = a.device
    if dev == "auto":
        dev = "cuda" if torch.cuda.is_available() else "cpu"
    if a.demo:
        tri = torch.tensor(
            [[0.0, 0.0], [1.0, 0.0], [0.5, math.sqrt(3) / 2]], dtype=torch.float64
        )
        ed = [(0, 1), (0, 2), (1, 2)]
        out = []
        for k in (2, 3):
            best, viol, dt = train(tri, ed, k, a.steps, a.lr, a.seed, dev, 0)
            out.append(
                {
                    "demo": f"triangle_k{k}",
                    "loss_best": best,
                    "violated": viol,
                    "time_s": round(dt, 2),
                }
            )
        print(json.dumps({"device": dev, "results": out}))
        return
    pts = load_points(a.points)
    edges = load_edges_file(a.edges) if a.edges else numeric_edges(pts)
    best, viol, dt, rbest = None, None, 0.0, None
    for r in range(a.restarts):
        b, v, t = train(
            pts, edges, a.k, a.steps, a.lr, a.seed + r, dev, a.log_every, a.entropy
        )[:3]
        dt += t
        if best is None or v < viol or (v == viol and b < best):
            best, viol, rbest = b, v, r
    print(
        json.dumps(
            {
                "device": dev,
                "n": pts.shape[0],
                "edges": len(edges),
                "k": a.k,
                "steps": a.steps,
                "seed": a.seed,
                "restarts": a.restarts,
                "entropy": a.entropy,
                "best_restart": rbest,
                "loss_best": best,
                "violated": viol,
                "time_s": round(dt, 2),
            }
        )
    )


if __name__ == "__main__":
    main()
