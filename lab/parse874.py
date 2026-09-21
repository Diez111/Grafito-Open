#!/usr/bin/env python3
"""Parsea 874.vtx (exacto estilo Mathematica: enteros, fracciones, Sqrt, +-*/) a floats.
Verifica conteo y aristas unitarias contra 874.edge. Uso:
  parse874.py  -> escribe /tmp/opencode/874_pts.json, imprime resumen.
"""

import json
import math
import re
import sys


class P:
    def __init__(self, s):
        self.toks = re.findall(r"Sqrt|\d+|[+\-*/()\[\]]", s)
        self.pos = 0

    def peek(self):
        return self.toks[self.pos] if self.pos < len(self.toks) else None

    def next(self):
        t = self.peek()
        self.pos += 1
        return t

    def expr(self):
        v = self.term()
        while self.peek() in ("+", "-"):
            op = self.next()
            w = self.term()
            v = v + w if op == "+" else v - w
        return v

    def term(self):
        v = self.fact()
        while self.peek() in ("*", "/"):
            op = self.next()
            w = self.fact()
            v = v * w if op == "*" else v / w
        return v

    def fact(self):
        t = self.next()
        if t == "-":
            return -self.fact()
        if t == "+":
            return self.fact()
        if t == "Sqrt":
            assert self.next() == "["
            v = self.expr()
            assert self.next() == "]"
            return math.sqrt(v)
        if t == "(":
            v = self.expr()
            assert self.next() == ")"
            return v
        return float(t)


def parse_pt(line):
    line = line.strip()
    assert line.startswith("{") and line.endswith("}"), line[:60]
    inner = line[1:-1]
    # split top-level comma
    depth = 0
    for i, ch in enumerate(inner):
        if ch in "[(":
            depth += 1
        elif ch in "])":
            depth -= 1
        elif ch == "," and depth == 0:
            return (P(inner[:i]).expr(), P(inner[i + 1 :]).expr())
    raise ValueError("sin coma top-level: " + line[:80])


def main():
    lines = [l for l in open("/tmp/opencode/874.vtx") if l.strip()]
    pts = [parse_pt(l) for l in lines]
    assert len(pts) == 874, len(pts)
    assert all(math.isfinite(x) and math.isfinite(y) for x, y in pts)
    # aristas del .edge (1-based)
    edges = set()
    for l in open("/tmp/opencode/874.edge"):
        if l.startswith("e "):
            _, a, b = l.split()
            a, b = int(a) - 1, int(b) - 1
            edges.add((min(a, b), max(a, b)))
    assert len(edges) == 4461, len(edges)
    # verificacion numerica: contar pares a distancia 1 (tol 1e-9)
    n = len(pts)
    unit = 0
    bad = 0
    for i in range(n):
        xi, yi = pts[i]
        for j in range(i + 1, n):
            if abs(math.hypot(xi - pts[j][0], yi - pts[j][1]) - 1.0) < 1e-9:
                unit += 1
                if (i, j) not in edges:
                    bad += 1
    print(
        f"puntos={n} aristas_edge={len(edges)} aristas_numericas={unit} "
        f"numericas_no_en_edge={bad}"
    )
    json.dump([[x, y] for x, y in pts], open("/tmp/opencode/874_pts.json", "w"))
    print("ok /tmp/opencode/874_pts.json")


if __name__ == "__main__":
    main()
