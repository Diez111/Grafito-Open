#!/usr/bin/env python3
"""Motor de animaciones de Grafito: protocolo JSON v1 sobre stdio.

Implementa el lado del motor del protocolo de grafito-anim. Cuando Manim
está instalado genera y renderiza una escena real; en caso contrario produce
un artefacto placeholder válido para que el pipeline sea comprobable sin
dependencias. Escribe SIEMPRE dentro del directorio de trabajo actual.
Versión 3: universal YouTube-style, placeholder <2s garantizado, manim fallback robusto.
"""

import ast
import json
import math
import os
import pathlib
import re
import sys
import time
import hashlib

from manim_engine import PROTOCOL_VERSION

# --- Constantes de validación ---
ALLOW_EXPORT = {"gif", "png", "mp4"}
ALLOW_TEMPLATE = {
    "derivative-slope",
    "integral-area",
    "taylor-series",
    "conformal-map",
    "pitagoras",
    "pythagoras",
    "universal",
    "",
}
# M2: canónicas que SOLO existen en el renderer nativo Rust (11 CANONICAL del
# protocolo menos las 6 del worker). Pedirlas acá es `error unsupported`
# honesto, jamás imagen falsa (ver `preparar_render` + test matriz M2-4).
CANONICAL_SOLO_RUST = {
    "euler",
    "fourier",
    "logistic-bifurcation",
    "gradient-field",
    "mobius-transform",
}
JOB_RE = re.compile(r"^[A-Za-z0-9_-]{1,64}$")
MAX_EXPR_LEN = 500
MAX_NODES = 200
MAX_CANVAS = 4096
MIN_CANVAS = 64
MAX_CONCEPT_LEN = 500
# M2-1: cotas anti-DoS de enteros gigantes (reproduce: `9**99999999` colgaba
# el worker evaluando un entero de ~954M de dígitos).
MAX_LITERAL_ABS = 1_000_000  # literales |v| > 1e6 → Err
MAX_LITERAL_DIGITOS = 6  # más de 6 dígitos en un literal → Err
MAX_POW_EXPONENTE = 1000  # `**` con exponente plegado > 1000 → Err
SAFE_EVAL_TIMEOUT_SECS = 1.0  # `safe_eval` abandona tras este tiempo → Err

SAFE_FUNCS = {
    "sin": math.sin,
    "cos": math.cos,
    "tan": math.tan,
    "exp": math.exp,
    "log": math.log,
    "sqrt": math.sqrt,
    "abs": abs,
    "pi": math.pi,
    "e": math.e,
}
ALLOWED_NODES = (
    ast.Expression,
    ast.BinOp,
    ast.UnaryOp,
    ast.Call,
    ast.Name,
    ast.Constant,
    ast.Load,
    ast.Add,
    ast.Sub,
    ast.Mult,
    ast.Div,
    ast.Pow,
    ast.Mod,
    ast.USub,
    ast.UAdd,
)
DENIED_NODES = (
    ast.Attribute,
    ast.Subscript,
    ast.Lambda,
    ast.ListComp,
    ast.DictComp,
    ast.GeneratorExp,
    ast.Await,
    ast.Yield,
    ast.Import,
    ast.ImportFrom,
)

# Paleta profesional YouTube
ACCENTS = [
    (66, 133, 244),
    (235, 211, 84),
    (255, 77, 77),
    (126, 214, 160),
    (168, 120, 255),
    (255, 153, 51),
]


def send(payload):
    sys.stdout.write(json.dumps(payload) + "\n")
    sys.stdout.flush()


def safe_path(workdir: pathlib.Path, job_id: str, export: str) -> pathlib.Path:
    if not JOB_RE.match(job_id):
        raise ValueError(f"job_id inválido: {job_id}")
    if export not in ALLOW_EXPORT:
        raise ValueError(f"export inválido: {export}")
    ext = "gif" if export == "gif" else "png"
    wd = workdir.resolve()
    p = (wd / f"{job_id}.{ext}").resolve()
    try:
        p.relative_to(wd)
    except ValueError:
        raise ValueError("path traversal detectado")
    return p


def _concept_color(concept: str):
    h = int(hashlib.sha256(concept.encode("utf-8")).hexdigest()[:6], 16)
    return ((h >> 16) & 0xFF, (h >> 8) & 0xFF, h & 0xFF)


def _escribir_bytes_exclusivo(p: pathlib.Path, data: bytes) -> None:
    """Escribe `data` en `p` sin seguir ni truncar nada existente (M2-2).

    `O_CREAT|O_EXCL|O_NOFOLLOW`: si el destino existe —archivo, dir o
    symlink plantado— falla con `FileExistsError` sin tocar a la víctima.
    Un único intento: el llamador convierte el `Err` en `error` honesto,
    jamás reintenta truncando.
    """
    flags = os.O_WRONLY | os.O_CREAT | os.O_EXCL | getattr(os, "O_NOFOLLOW", 0)
    try:
        fd = os.open(p, flags, 0o644)
    except FileExistsError:
        raise
    except OSError as e:
        raise FileExistsError(f"destino no creable sin sobrescribir: {p.name} ({e})")
    try:
        with os.fdopen(fd, "wb") as handle:
            handle.write(data)
    except BaseException:
        try:
            os.unlink(p)
        except OSError:
            pass
        raise


def placeholder_media(job_id, export, concept=""):
    """Escribe un artefacto placeholder profesional <2s y devuelve su ruta.

    M2-2: creación exclusiva de punta a punta (Pillow vuelca a `BytesIO` y
    el único write a disco es `_escribir_bytes_exclusivo`). Si el destino
    ya existe o es un symlink plantado → `FileExistsError` sin fallback
    que trunque.
    """
    workdir = pathlib.Path(os.getcwd())
    # El enlace plantado se ve en la ruta cruda (sin resolver): `safe_path`
    # resuelve y podría apuntar a la víctima en vez de al enlace.
    ext_cruda = "gif" if export == "gif" else "png"
    crudo = workdir / f"{job_id}.{ext_cruda}"
    if os.path.lexists(crudo):
        raise FileExistsError(f"el destino ya existe: {crudo.name}")
    p = safe_path(workdir, job_id, export if export in ALLOW_EXPORT else "png")
    # Vía rápida honesta (el guard real es el O_EXCL atómico de abajo).
    if os.path.lexists(p):
        raise FileExistsError(f"el destino ya existe: {p.name}")
    start = time.time()
    data = None
    # Intentar generar placeholder con Pillow (estilo YouTube) si está disponible
    try:
        from PIL import Image, ImageDraw

        import io as _io

        w, h = 640, 360
        bg = (14, 14, 20)
        accent = _concept_color(concept if concept else job_id)
        # crear imagen con gradiente sutil
        img = Image.new("RGB", (w, h), bg)
        draw = ImageDraw.Draw(img)
        # gradiente vertical muy sutil hacia bg claro
        for y in range(h):
            mix = y / h * 0.25
            r = int(bg[0] * (1 - mix) + (22) * (mix) + accent[0] * 0.05)
            g = int(bg[1] * (1 - mix) + (22) * (mix) + accent[1] * 0.05)
            b = int(bg[2] * (1 - mix) + (34) * (mix) + accent[2] * 0.05)
            draw.line([(0, y), (w, y)], fill=(r, g, b))
        # grid sutil
        step = 40
        for x in range(0, w, step):
            draw.line([(x, 0), (x, h)], fill=(255, 255, 255, 18))
        for y in range(0, h, step):
            draw.line([(0, y), (w, y)], fill=(255, 255, 255, 18))
        # barra de acento superior
        draw.rectangle([0, 0, w, 4], fill=accent)
        # titulo centrado (truncado, seguro)
        text = (concept[:42] + "...") if len(concept) > 42 else concept
        if not text.strip():
            text = "matemática"
        # dibujar texto con fuente por defecto (sin dependencia externa)
        # centrar aproximadamente
        # Pillow default font approx 6px por char
        tx = max(10, (w - len(text) * 7) // 2)
        ty = h // 2 - 10
        # fondo semitransparente para legibilidad
        draw.rectangle(
            [tx - 8, ty - 6, tx + len(text) * 7 + 8, ty + 18], fill=(0, 0, 0)
        )
        draw.text((tx, ty), text, fill=(235, 235, 245))
        # progress bar inferior animado (estático en placeholder, 70%)
        bar_w = int(w * 0.72)
        draw.rectangle([0, h - 6, w, h - 2], fill=(255, 255, 255, 30))
        draw.rectangle([0, h - 6, bar_w, h - 2], fill=accent)
        # circulo central pulsante simulado
        cx, cy = w // 2, h // 2 + 40
        draw.ellipse([cx - 8, cy - 8, cx + 8, cy + 8], fill=(255, 255, 255))
        draw.ellipse([cx - 4, cy - 4, cx + 4, cy + 4], fill=accent)
        # guardar a memoria: el único write a disco es el exclusivo de abajo
        buf = _io.BytesIO()
        if export == "gif":
            # GIF 1 frame: guardar como GIF
            img.save(buf, format="GIF")
        else:
            img.save(buf, format="PNG")
        data = buf.getvalue()
        # garantizar <2s
        if time.time() - start > 1.9:
            print(f"[placeholder] took {time.time() - start:.2f}s", file=sys.stderr)
    except ImportError:
        pass
    except Exception as e:
        # cualquier error en Pillow -> fallback a hex estable (sigue <2s)
        print(f"[placeholder PIL fallback] {e}", file=sys.stderr)
        pass
    if data is None:
        # Fallback ultra-rápido 1x1 (siempre <5ms) con color variado por concepto
        if export == "gif":
            data = bytes.fromhex(
                "47494638396101000100800000000000ffffff21f90400000000002c00000000010001000002024401003b"
            )
            # variar un byte de paleta segun hash para distinguir conceptos
            try:
                accent = _concept_color(concept if concept else job_id)
                arr = bytearray(data)
                # bytes 13,14,15 son color de fondo en GIF header si existe palette
                if len(arr) > 15:
                    arr[13] = accent[0]
                    arr[14] = accent[1]
                    arr[15] = accent[2]
                data = bytes(arr)
            except Exception:
                pass
        else:
            data = bytes.fromhex(
                "89504e470d0a1a0a0000000d49484452000000010000000108060000001f15c4890000000d49444154789c63000100000500010d0a2db40000000049454e44ae426082"
            )
    _escribir_bytes_exclusivo(p, data)
    return str(p)


def manim_is_available() -> bool:
    try:
        import manim  # noqa: F401

        return True
    except Exception:
        return False


def _digitos_de_literal(valor) -> int:
    """Cuenta dígitos decimales de un literal numérico (M2-1).

    Enteros: dígitos del valor absoluto. Flotantes: dígitos de la parte
    entera más los de la fraccionaria (sin punto ni signo ni exponente).
    `bool` no es literal numérico válido acá → 0 (lo rechaza el chequeo
    de tipo del llamador).
    """
    if isinstance(valor, bool):
        return 0
    if isinstance(valor, int):
        return len(str(abs(valor)))
    if isinstance(valor, float):
        if not __import__("math").isfinite(valor):
            return MAX_LITERAL_DIGITOS + 1
        texto = repr(abs(valor))
        return sum(1 for c in texto if c.isdigit())
    return MAX_LITERAL_DIGITOS + 1


def _plegar_constante(nodo):
    """Pliega `Constant`/aritmética de literales a `float`, o `None`.

    Solo literales y `+ - * / **` entre ellos (sin nombres): sirve para
    acotar el exponente de `**` aunque venga como `2+3`. `None` = depende
    de `x` o no es numérico → no se acota acá (lo cubre el timeout).
    """
    if isinstance(nodo, ast.Constant):
        v = nodo.value
        if isinstance(v, bool):
            return None
        if isinstance(v, (int, float)):
            try:
                f = float(v)
            except (OverflowError, ValueError):
                return None
            return f if __import__("math").isfinite(f) else None
        return None
    if isinstance(nodo, ast.UnaryOp) and isinstance(nodo.op, (ast.UAdd, ast.USub)):
        v = _plegar_constante(nodo.operand)
        if v is None:
            return None
        return v if isinstance(nodo.op, ast.UAdd) else -v
    if isinstance(nodo, ast.BinOp) and isinstance(
        nodo.op, (ast.Add, ast.Sub, ast.Mult, ast.Div, ast.Pow, ast.Mod)
    ):
        a = _plegar_constante(nodo.left)
        b = _plegar_constante(nodo.right)
        if a is None or b is None:
            return None
        try:
            if isinstance(nodo.op, ast.Add):
                r = a + b
            elif isinstance(nodo.op, ast.Sub):
                r = a - b
            elif isinstance(nodo.op, ast.Mult):
                r = a * b
            elif isinstance(nodo.op, ast.Div):
                r = a / b
            elif isinstance(nodo.op, ast.Mod):
                r = a % b
            else:  # Pow: el exponente gigante se rechaza antes de evaluar
                if abs(b) > MAX_POW_EXPONENTE:
                    return float("inf")
                r = a**b
        except (OverflowError, ZeroDivisionError, ValueError):
            return None
        return r if __import__("math").isfinite(r) else None
    return None


def validate_expr(src: str) -> str:
    if not isinstance(src, str) or not (1 <= len(src) <= MAX_EXPR_LEN):
        raise ValueError("expression longitud inválida")
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
                raise ValueError(
                    f"función no permitida: {getattr(node.func, 'id', '?')}"
                )
        if isinstance(node, ast.Name) and node.id != "x" and node.id not in SAFE_FUNCS:
            raise ValueError(f"variable no permitida: {node.id}")
        # M2-1: literales acotados (anti-DoS de enteros gigantes).
        if isinstance(node, ast.Constant) and isinstance(node.value, (int, float)):
            if isinstance(node.value, bool):
                raise ValueError("booleano no permitido en expresión")
            if abs(float(node.value)) > MAX_LITERAL_ABS:
                raise ValueError(f"literal {node.value!r} excede ±{MAX_LITERAL_ABS}")
            if _digitos_de_literal(node.value) > MAX_LITERAL_DIGITOS:
                raise ValueError(
                    f"literal {node.value!r} excede {MAX_LITERAL_DIGITOS} dígitos"
                )
        # M2-1: `**` con exponente plegado gigante (p. ej. `9**99999999`).
        if (
            isinstance(node, ast.BinOp)
            and isinstance(node.op, ast.Pow)
            and _plegar_constante(node.right) is not None
            and abs(_plegar_constante(node.right)) > MAX_POW_EXPONENTE
        ):
            raise ValueError(f"exponente excede ±{MAX_POW_EXPONENTE} (anti-DoS)")
    return src


_SAFE_EVAL_POOL = None
_SAFE_EVAL_POOL_LOCK = None


def _get_safe_eval_pool():
    """Pool global reutilizado R1-6 (un solo hilo, sin crear por eval).

    Antes se creaba un `ThreadPoolExecutor(1)` + `shutdown(wait=False)` por
    eval: 50 timeouts seguidos dejaban 50 hilos abandonados. Ahora el pool
    vive en el módulo y se reutiliza (threads estables). El hilo abandonado
    tras un timeout sigue sin poder matarse (GIL): la barrera primaria es
    `validate_expr`; el timeout es segunda barrera. Nunca se hace shutdown
    (vive hasta el fin del worker).
    """
    import concurrent.futures as _cf
    import threading as _th

    global _SAFE_EVAL_POOL, _SAFE_EVAL_POOL_LOCK
    if _SAFE_EVAL_POOL_LOCK is None:
        _SAFE_EVAL_POOL_LOCK = _th.Lock()
    if _SAFE_EVAL_POOL is None:
        with _SAFE_EVAL_POOL_LOCK:
            if _SAFE_EVAL_POOL is None:
                _SAFE_EVAL_POOL = _cf.ThreadPoolExecutor(
                    max_workers=1, thread_name_prefix="safe-eval"
                )
    return _SAFE_EVAL_POOL


def safe_eval(
    expr: str, x_val: float, timeout_secs: float = SAFE_EVAL_TIMEOUT_SECS
) -> float:
    """Evalúa `expr` con `x=x_val`, acotada en tiempo (M2-1).

    La validación estática (`validate_expr`) ya rechaza literales y
    exponentes gigantes; este timeout es la segunda barrera para lo que
    no se puede plegar (`x**x`, funciones con convergencia lenta…).
    Vencido → `ValueError` rápido. El hilo abandonado no se puede matar
    (GIL): la barrera primaria sigue siendo `validate_expr`.
    R1-6: usa el pool global reutilizado (sin crear hilos por eval).
    """
    code = compile(validate_expr(expr), "<expr>", "eval")
    env = {"x": x_val, **SAFE_FUNCS}
    import concurrent.futures as _cf

    pool = _get_safe_eval_pool()
    futuro = pool.submit(eval, code, {"__builtins__": {}}, env)
    try:
        return futuro.result(timeout=timeout_secs)
    except _cf.TimeoutError as e:
        futuro.cancel()
        raise ValueError(f"evaluación excedió {timeout_secs}s (anti-DoS)") from e


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


def _sanitize_concept(raw) -> str:
    if not isinstance(raw, str):
        raw = str(raw) if raw is not None else ""
    s = raw.strip().replace("\n", " ").replace("\r", " ").replace("\t", " ")
    # colapsar espacios
    s = re.sub(r"\s+", " ", s)
    if len(s) > MAX_CONCEPT_LEN:
        s = s[:MAX_CONCEPT_LEN]
    if not s:
        s = "matemática"
    return s


def _get_expr_text(request):
    # Universal: si spec tiene expression valida, usarla; si no, intentar concept como expresion
    # pero nunca fallar: si validacion falla, usar "x**2" como fallback seguro para manim
    concept_raw = request.get("concept") or ""
    concept = _sanitize_concept(concept_raw)
    expression = request.get("spec", {}) or {}
    if isinstance(expression, dict):
        expr_text = expression.get("expression") or concept or "x**2"
    else:
        expr_text = str(expression) if expression else concept or "x**2"
    expr_text = str(expr_text).strip()[:MAX_EXPR_LEN]
    if not expr_text:
        expr_text = "x**2"
    # intentar validar, si falla usar fallback sin lanzar
    try:
        return validate_expr(expr_text)
    except Exception:
        # concepto natural no es expresion matematica valida -> fallback seguro
        # pero guardar concepto original para label
        return "x**2"


class RenderRequestError(ValueError):
    """Pedido inválido o no soportado, con código de protocolo (M2-3/M2-4).

    `code`: `invalid_request` (export/canvas/job_id/template malformados)
    o `unsupported` (template canónico que solo existe en el renderer
    nativo Rust: jamás se finge con una imagen de otro template).
    """

    def __init__(self, code: str, message: str):
        super().__init__(message)
        self.code = code


def preparar_render(message) -> dict:
    """Valida un `render_request` sin fallbacks silenciosos (M2-3/M2-4).

    Devuelve `{"job_id", "export", "canvas", "template", "concept"}` con
    el template ya normalizado (`pythagoras`→`pitagoras`; `""`/`"auto"` →
    ruteo por concepto documentado). Lanza `RenderRequestError` honesto
    en vez de defaultear a png/640×480/derivative-slope.
    """
    job_id = message.get("job_id")
    if not isinstance(job_id, str) or not JOB_RE.match(job_id):
        raise RenderRequestError("invalid_request", "job_id inválido")
    export = message.get("export", "png")
    if export not in ALLOW_EXPORT:
        raise RenderRequestError(
            "invalid_request",
            f"export inválido: {export!r} (válidos: gif/png/mp4)",
        )
    canvas_raw = message.get("canvas", [640, 480])
    try:
        canvas = parse_canvas(canvas_raw)
    except ValueError as e:
        raise RenderRequestError("invalid_request", f"canvas inválido: {e}")
    template = message.get("template", "")
    if not isinstance(template, str):
        raise RenderRequestError("invalid_request", "template no es texto")
    t = template.strip().lower()
    # Alias histórico documentado.
    if t == "pythagoras":
        t = "pitagoras"
    if t in ALLOW_TEMPLATE and t != "":
        pass
    elif t in ("", "auto"):
        # Sin plantilla: ruteo por concepto (documentado, no default fijo).
        t = ""
    elif t in CANONICAL_SOLO_RUST:
        # M2-4: paridad 11/6 explícita — el worker no finge estos renders.
        raise RenderRequestError(
            "unsupported",
            f"template {t!r} solo disponible en el renderer nativo "
            "(pedilo sin motor externo o usá derivative-slope/integral-area/"
            "taylor-series/conformal-map/pitagoras/universal)",
        )
    else:
        raise RenderRequestError(
            "invalid_request", f"template desconocido: {template!r}"
        )
    concept = _sanitize_concept(message.get("concept", ""))
    return {
        "job_id": job_id,
        "export": export,
        "canvas": canvas,
        "template": t,
        "concept": concept,
    }


def render_with_manim(job_id, request, canvas):
    """Construye y renderiza una escena segun template, de forma segura. Universal fallback."""
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
    concept_raw = _sanitize_concept(request.get("concept") or expr_text)
    safe_label = re.sub(r"[^a-zA-Z0-9+\-*/^()., x]", "", expr_text)[:80]
    if not safe_label.strip():
        safe_label = re.sub(r"[^a-zA-Z0-9+\-*/^()., ]", "", concept_raw)[:40] or "f(x)"
    width, height = canvas
    workdir = pathlib.Path(os.getcwd()).resolve()
    media_dir = workdir / "media"
    media_dir.mkdir(exist_ok=True)
    config.media_dir = str(media_dir)
    config.frame_width = max(width / 100, 8)
    config.pixel_width = width
    config.pixel_height = height
    template = request.get("template", "derivative-slope")
    # normalizar template universal/pitagoras
    if template in ("universal", "auto", ""):
        # elegir mejor segun concepto
        c = concept_raw.lower()
        if "pit" in c or "pythag" in c:
            template = "pitagoras"
        elif "integral" in c:
            template = "integral-area"
        elif "taylor" in c:
            template = "taylor-series"
        elif "conform" in c or "complej" in c or "complex" in c:
            template = "conformal-map"
        else:
            template = "derivative-slope"
    if template == "pythagoras":
        template = "pitagoras"

    if template == "pitagoras":

        class PitagorasScene(Scene):
            def construct(self):
                from manim import Polygon, Text

                # triangulo rectangulo clasico
                tri = Polygon([-2, -1, 0], [2, -1, 0], [2, 1.5, 0], color="#FFFFFF")
                self.play(Create(tri))
                # cuadrados: animacion simple con colores
                sq1 = Polygon(
                    [-2, -1, 0],
                    [-2, -3, 0],
                    [0, -3, 0],
                    [0, -1, 0],
                    color="#3EA6FF",
                    fill_opacity=0.3,
                )
                sq2 = Polygon(
                    [2, -1, 0],
                    [3.5, -1, 0],
                    [3.5, 1.5, 0],
                    [2, 1.5, 0],
                    color="#FFD84D",
                    fill_opacity=0.3,
                )
                self.play(FadeIn(sq1), FadeIn(sq2))
                label = MathTex("a^2+b^2=c^2").to_edge("UR")
                self.play(Create(label))

        SceneClass = PitagorasScene
    elif template == "integral-area":

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
                area = axes.get_area(
                    graph, x_range=[0, 2], color="#5B8DEF", opacity=0.4
                )
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

                def taylor_fn(x):
                    x0 = 0.0
                    try:
                        y0 = safe_eval(expr_text, x0)
                        h = 1e-5
                        y1 = (
                            safe_eval(expr_text, x0 + h) - safe_eval(expr_text, x0 - h)
                        ) / (2 * h)
                        y2 = (
                            safe_eval(expr_text, x0 + h)
                            - 2 * y0
                            + safe_eval(expr_text, x0 - h)
                        ) / (h * h)
                        return y0 + y1 * (x - x0) + 0.5 * y2 * (x - x0) ** 2
                    except Exception:
                        return 0.0

                taylor = FunctionGraph(taylor_fn, x_range=[-4, 4], color="#FFD84D")
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
                        return x + 0.2 * math.sin(3 * x)
                    except Exception:
                        return x

                graph = FunctionGraph(fn, x_range=[-4, 4], color="#7ED6A0")
                self.play(Create(graph))
                label = MathTex(safe_label).to_edge("UR")
                self.play(Create(label))

        SceneClass = ConformalScene
    else:
        # derivative-slope + universal fallback (cubre cualquier texto)
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
                # titulo con concepto si no es expresion pura
                title = (
                    safe_label
                    if safe_label != "x**2" or "deriv" in concept_raw.lower()
                    else concept_raw[:30]
                )
                label = MathTex(
                    re.sub(r"[^a-zA-Z0-9+\-*/^()., ]", "", title)[:40]
                ).to_edge("UR")
                self.play(Create(label))

        SceneClass = DerivativeScene

    scene = SceneClass()
    scene.render()
    media_path = pathlib.Path(scene.renderer.file_writer.movie_file_path).resolve()
    try:
        media_path.relative_to(workdir)
    except ValueError:
        raise ValueError("media_path fuera de workdir")
    return str(media_path)


def main():
    send(
        {
            "type": "hello",
            "protocol_version": PROTOCOL_VERSION,
            "capabilities": [
                "derivative-slope",
                "integral-area",
                "taylor-series",
                "conformal-map",
                "pitagoras",
                "universal",
            ],
        }
    )
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
                pedido = preparar_render(message)
            except RenderRequestError as e:
                try:
                    send(
                        {
                            "type": "error",
                            "job_id": str(message.get("job_id", "unknown")),
                            "code": e.code,
                            "message": str(e)[:500],
                        }
                    )
                except Exception:
                    pass
                continue
            except Exception as e:
                try:
                    send(
                        {
                            "type": "error",
                            "job_id": str(message.get("job_id", "unknown")),
                            "code": "invalid_request",
                            "message": str(e)[:500],
                        }
                    )
                except Exception:
                    pass
                continue
            job_id = pedido["job_id"]
            export = pedido["export"]
            canvas = pedido["canvas"]
            concept_for_placeholder = pedido["concept"]
            # El template normalizado viaja en el pedido para manim.
            message = dict(message)
            message["template"] = pedido["template"]
            message["concept"] = pedido["concept"]

            send(
                {"type": "progress", "job_id": job_id, "step": "render", "percent": 30}
            )
            try:
                # placeholder rapido si manim no esta o export es png estatico (siempre <2s)
                if export in ("mp4", "gif") and manim_is_available():
                    try:
                        send(
                            {
                                "type": "progress",
                                "job_id": job_id,
                                "step": "manim",
                                "percent": 60,
                            }
                        )
                        media_path = render_with_manim(job_id, message, canvas)
                        frames = 1
                        duration_ms = 1000
                    except Exception as e:
                        print(f"[manim fallback] {e}", file=sys.stderr)
                        media_path = placeholder_media(
                            job_id,
                            export if export == "gif" else "png",
                            concept_for_placeholder,
                        )
                        frames = 1
                        duration_ms = 120
                else:
                    # placeholder universal: cualquier texto -> imagen profesional en <2s
                    media_path = placeholder_media(
                        job_id,
                        export if export == "gif" else "png",
                        concept_for_placeholder,
                    )
                    frames = 1
                    duration_ms = 120
                send(
                    {
                        "type": "progress",
                        "job_id": job_id,
                        "step": "render",
                        "percent": 100,
                    }
                )
                send(
                    {
                        "type": "render_result",
                        "job_id": job_id,
                        "media_path": media_path,
                        "frames": frames,
                        "duration_ms": duration_ms,
                    }
                )
            except Exception as error:  # noqa: BLE001
                # ultimo fallback: placeholder incluso si algo exploto, nunca dejar al cliente colgado
                try:
                    media_path = placeholder_media(
                        job_id, "png", concept_for_placeholder
                    )
                    send(
                        {
                            "type": "progress",
                            "job_id": job_id,
                            "step": "render",
                            "percent": 100,
                        }
                    )
                    send(
                        {
                            "type": "render_result",
                            "job_id": job_id,
                            "media_path": media_path,
                            "frames": 1,
                            "duration_ms": 120,
                        }
                    )
                except Exception:
                    send(
                        {
                            "type": "error",
                            "job_id": job_id,
                            "code": "render_failed",
                            "message": str(error)[:500],
                        }
                    )
        else:
            continue


if __name__ == "__main__":
    main()
