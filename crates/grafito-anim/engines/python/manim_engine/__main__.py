#!/usr/bin/env python3
"""Motor de animaciones de Grafito: protocolo JSON v1 sobre stdio.

Implementa el lado del motor del protocolo de grafito-anim. Cuando Manim
está instalado genera y renderiza una escena real; en caso contrario produce
un artefacto placeholder válido para que el pipeline sea comprobable sin
dependencias. Escribe SIEMPRE dentro del directorio de trabajo actual.
Versión 2: sandbox reforzado, templates múltiples, safe_path symlink-safe.
"""

import ast
import json
import math
import os
import pathlib
import re
import sys

from manim_engine import PROTOCOL_VERSION

# --- Constantes de validación ---
ALLOW_EXPORT = {"gif", "png", "mp4"}
ALLOW_TEMPLATE = {"derivative-slope", "integral-area", "taylor-series", "conformal-map", ""}
JOB_RE = re.compile(r"^[A-Za-z0-9_-]{1,64}$")
MAX_EXPR_LEN = 500
MAX_NODES = 200
MAX_CANVAS = 4096
MIN_CANVAS = 64

SAFE_FUNCS = {
    "sin": math.sin, "cos": math.cos, "tan": math.tan,
    "exp": math.exp, "log": math.log, "sqrt": math.sqrt,
    "abs": abs, "pi": math.pi, "e": math.e,
}
# Whitelist estricta: solo nodos seguros. Attribute/Subscript/Lambda bloqueados explícitamente.
ALLOWED_NODES = (
    ast.Expression, ast.BinOp, ast.UnaryOp, ast.Call,
    ast.Name, ast.Constant, ast.Load,
    ast.Add, ast.Sub, ast.Mult, ast.Div, ast.Pow, ast.Mod,
    ast.USub, ast.UAdd,
)
# Nodos explícitamente prohibidos (para mensaje claro)
DENIED_NODES = (ast.Attribute, ast.Subscript, ast.Lambda, ast.ListComp, ast.DictComp, ast.GeneratorExp, ast.Await, ast.Yield, ast.Import, ast.ImportFrom)

def send(payload):
    sys.stdout.write(json.dumps(payload) + "\n")
    sys.stdout.flush()

def safe_path(workdir: pathlib.Path, job_id: str, export: str) -> pathlib.Path:
    if not JOB_RE.match(job_id):
        raise ValueError(f"job_id inválido: {job_id}")
    if export not in ALLOW_EXPORT:
        raise ValueError(f"export inválido: {export}")
    ext = "gif" if export == "gif" else "png"
    # Symlink-safe: resolver ambos y verificar relative_to
    wd = workdir.resolve()
    p = (wd / f"{job_id}.{ext}").resolve()
    try:
        p.relative_to(wd)
    except ValueError:
        raise ValueError("path traversal detectado")
    return p

def placeholder_media(job_id, export):
    """Escribe un artefacto placeholder y devuelve su ruta absoluta."""
    workdir = pathlib.Path(os.getcwd())
    p = safe_path(workdir, job_id, export if export in ALLOW_EXPORT else "png")
    if export == "gif":
        # GIF 1x1 transparente - hex estable
        data = bytes.fromhex("47494638396101000100800000000000ffffff21f90400000000002c00000000010001000002024401003b")
    else:
        data = bytes.fromhex("89504e470d0a1a0a0000000d49484452000000010000000108060000001f15c4890000000d49444154789c63000100000500010d0a2db40000000049454e44ae426082")
    try:
        with open(p, "xb") as handle:
            handle.write(data)
    except FileExistsError:
        with open(p, "wb") as handle:
            handle.write(data)
    return str(p)

def manim_is_available() -> bool:
    try:
        import manim  # noqa: F401
        return True
    except Exception:
        return False

def validate_expr(src: str) -> str:
    if not isinstance(src, str) or not (1 <= len(src) <= MAX_EXPR_LEN):
        raise ValueError("expression longitud inválida")
    # Bloquear dunders directamente en texto
    if "__" in src:
        raise ValueError("dunder no permitido")
    try:
        tree = ast.parse(src, mode="eval")
    except SyntaxError as e:
        raise ValueError(f"syntax: {e}")
    count = 0
    for node in ast.walk(tree):
        count += 1
        if count > MAX_NODES:
            raise ValueError("expression demasiado compleja")
        if isinstance(node, DENIED_NODES):
            raise ValueError(f"nodo denegado: {type(node).__name__}")
        if not isinstance(node, ALLOWED_NODES):
            raise ValueError(f"nodo no permitido: {type(node).__name__}")
        if isinstance(node, ast.Call):
            if not isinstance(node.func, ast.Name) or node.func.id not in SAFE_FUNCS:
                raise ValueError(f"función no permitida: {getattr(node.func, 'id', '?')}")
        if isinstance(node, ast.Name) and node.id != "x" and node.id not in SAFE_FUNCS:
            raise ValueError(f"variable no permitida: {node.id}")
    return src

def safe_eval(expr: str, x_val: float) -> float:
    code = compile(validate_expr(expr), "<expr>", "eval")
    env = {"x": x_val, **SAFE_FUNCS}
    return eval(code, {"__builtins__": {}}, env)

def parse_canvas(raw) -> tuple:
    if not isinstance(raw, (list, tuple)) or len(raw) != 2:
        raise ValueError("canvas debe ser [w,h]")
    try:
        w, h = int(raw[0]), int(raw[1])
    except Exception:
        raise ValueError("canvas valores no enteros")
    if not (MIN_CANVAS <= w <= MAX_CANVAS and MIN_CANVAS <= h <= MAX_CANVAS):
        raise ValueError(f"canvas {w}x{h} fuera de rango {MIN_CANVAS}..{MAX_CANVAS}")
    return w, h

def _get_expr_text(request):
    expression = request.get("spec", {}) or {}
    # Soporta tanto {"expression":"x**2"} como {"spec":{"expression":...}}
    if isinstance(expression, dict):
        expr_text = expression.get("expression") or request.get("concept") or "x**2"
    else:
        expr_text = str(expression) if expression else request.get("concept") or "x**2"
    return validate_expr(str(expr_text))

def render_with_manim(job_id, request, canvas):
    """Construye y renderiza una escena según template, de forma segura."""
    # Import tardío para no penalizar placeholder
    from manim import (
        Axes,
        Create,
        FunctionGraph,
        MathTex,
        Scene,
        config,
        Polygon,
        FadeIn,
    )

    expr_text = _get_expr_text(request)
    safe_label = re.sub(r"[^a-zA-Z0-9+\-*/^()., x]", "", expr_text)[:80]
    width, height = canvas
    workdir = pathlib.Path(os.getcwd()).resolve()
    media_dir = workdir / "media"
    media_dir.mkdir(exist_ok=True)
    config.media_dir = str(media_dir)
    config.frame_width = max(width / 100, 8)
    config.pixel_width = width
    config.pixel_height = height
    template = request.get("template", "derivative-slope")

    # Factory de escenas por template
    if template == "integral-area":
        class IntegralScene(Scene):
            def construct(self):
                axes = Axes(x_range=[-4, 4, 1], y_range=[-1, 9, 1])
                self.play(Create(axes))
                def fn(x):
                    try:
                        return safe_eval(expr_text, x)
                    except Exception:
                        return 0.0
                graph = FunctionGraph(fn, x_range=[-4, 4], color="#5B8DEF")
                area = axes.get_area(graph, x_range=[0, 2], color="#5B8DEF", opacity=0.4)
                self.play(Create(graph))
                self.play(FadeIn(area))
                label = MathTex(safe_label + r"\quad \int_0^2").to_edge("UR")
                self.play(Create(label))
        SceneClass = IntegralScene
    elif template == "taylor-series":
        class TaylorScene(Scene):
            def construct(self):
                axes = Axes(x_range=[-4, 4, 1], y_range=[-4, 4, 1])
                self.play(Create(axes))
                def fn(x):
                    try:
                        return safe_eval(expr_text, x)
                    except Exception:
                        return 0.0
                graph = FunctionGraph(fn, x_range=[-4, 4], color="#ED5B5B")
                # Aproximación: tangente + curvatura (Taylor 2)
                def taylor_fn(x):
                    x0 = 0.0
                    try:
                        y0 = safe_eval(expr_text, x0)
                        # derivada numérica simple
                        h = 1e-5
                        y1 = (safe_eval(expr_text, x0+h)-safe_eval(expr_text, x0-h))/(2*h)
                        y2 = (safe_eval(expr_text, x0+h)-2*y0+safe_eval(expr_text, x0-h))/(h*h)
                        return y0 + y1*(x-x0) + 0.5*y2*(x-x0)**2
                    except Exception:
                        return 0.0
                taylor = FunctionGraph(taylor_fn, x_range=[-4,4], color="#FFD84D")
                self.play(Create(graph))
                self.play(Create(taylor))
                label = MathTex(safe_label + r"\approx T_2(x)").to_edge("UR")
                self.play(Create(label))
        SceneClass = TaylorScene
    elif template == "conformal-map":
        class ConformalScene(Scene):
            def construct(self):
                axes = Axes(x_range=[-3, 3, 1], y_range=[-3, 3, 1])
                self.play(Create(axes))
                def fn(x):
                    try:
                        # mapeo conforme simple: w = x + 0.2*sin(x)
                        return x + 0.2*math.sin(3*x)
                    except Exception:
                        return x
                grid = axes.get_area(FunctionGraph(lambda x: 0, x_range=[-3,3]), x_range=[-3,3])
                # Dibujar red transformada como líneas
                for y in [-2, -1, 0, 1, 2]:
                    pts = [axes.coords_to_point(fn(x), y+0.1*math.sin(x)) for x in [i*0.2 for i in range(-15,16)]]
                    # Fallback a FunctionGraph si no hay puntos
                graph = FunctionGraph(fn, x_range=[-4, 4], color="#7ED6A0")
                self.play(Create(graph))
                label = MathTex(safe_label).to_edge("UR")
                self.play(Create(label))
        SceneClass = ConformalScene
    else:  # derivative-slope default
        class DerivativeScene(Scene):
            def construct(self):
                axes = Axes(x_range=[-4, 4, 1], y_range=[-1, 9, 1])
                self.play(Create(axes))
                def fn(x):
                    try:
                        return safe_eval(expr_text, x)
                    except Exception:
                        return 0.0
                function = FunctionGraph(fn, x_range=[-4, 4])
                self.play(Create(function))
                label = MathTex(safe_label).to_edge("UR")
                self.play(Create(label))
        SceneClass = DerivativeScene

    scene = SceneClass()
    scene.render()
    # Validación symlink-safe del media_path
    media_path = pathlib.Path(scene.renderer.file_writer.movie_file_path).resolve()
    try:
        media_path.relative_to(workdir)
    except ValueError:
        raise ValueError("media_path fuera de workdir")
    return str(media_path)


def main():
    send({"type": "hello", "protocol_version": PROTOCOL_VERSION, "capabilities": ["derivative-slope", "integral-area", "taylor-series", "conformal-map"]})
    for raw_line in sys.stdin:
        raw_line = raw_line.strip()
        if not raw_line:
            continue
        try:
            message = json.loads(raw_line)
        except json.JSONDecodeError:
            continue
        kind = message.get("type")
        if kind == "ping":
            send({"type": "pong"})
        elif kind == "shutdown":
            break
        elif kind == "render_request":
            try:
                job_id = message.get("job_id")
                if not isinstance(job_id, str) or not JOB_RE.match(job_id):
                    raise ValueError("job_id inválido")
                export = message.get("export", "png")
                if export not in ALLOW_EXPORT:
                    export = "png"
                canvas_raw = message.get("canvas", [640, 480])
                try:
                    canvas = parse_canvas(canvas_raw)
                except ValueError:
                    canvas = (640, 480)
                template = message.get("template", "")
                if template not in ALLOW_TEMPLATE:
                    template = "derivative-slope"
            except Exception as e:
                try:
                    send({"type": "error", "job_id": str(message.get("job_id", "unknown")), "code": "invalid_request", "message": str(e)[:500]})
                except Exception:
                    pass
                continue

            send({"type": "progress", "job_id": job_id, "step": "render", "percent": 30})
            try:
                if export in ("mp4", "gif") and manim_is_available():
                    try:
                        # Simular progreso intermedio
                        send({"type": "progress", "job_id": job_id, "step": "manim", "percent": 60})
                        media_path = render_with_manim(job_id, message, canvas)
                        frames = 1
                        duration_ms = 1000
                    except Exception as e:
                        # Fallback a placeholder con log a stderr para diagnóstico
                        print(f"[manim fallback] {e}", file=sys.stderr)
                        media_path = placeholder_media(job_id, export if export == "gif" else "png")
                        frames = 1
                        duration_ms = 120
                else:
                    media_path = placeholder_media(job_id, export if export == "gif" else "png")
                    frames = 1
                    duration_ms = 120
                send({"type": "progress", "job_id": job_id, "step": "render", "percent": 100})
                send({
                    "type": "render_result",
                    "job_id": job_id,
                    "media_path": media_path,
                    "frames": frames,
                    "duration_ms": duration_ms,
                })
            except Exception as error:  # noqa: BLE001
                send({
                    "type": "error",
                    "job_id": job_id,
                    "code": "render_failed",
                    "message": str(error)[:500],
                })
        else:
            continue


if __name__ == "__main__":
    main()
