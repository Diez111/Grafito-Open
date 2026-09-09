"""Tests M2 robustez del worker manim (red-first del frente M2+M3).

Solo stdlib. Runner desde `crates/grafito-anim/engines/python`::

    python3 -m unittest manim_engine.test_m2_robustez -v

Cubre: M2-1 (literales/`**`/timeout), M2-2 (O_EXCL|O_NOFOLLOW + symlink),
M2-3 (sin fallbacks silenciosos) y M2-4 (matriz 11/6 `unsupported`).
"""

import os
import tempfile
import time
import unittest

from manim_engine.__main__ import (
    ALLOW_TEMPLATE,
    CANONICAL_SOLO_RUST,
    MAX_LITERAL_DIGITOS,
    MAX_POW_EXPONENTE,
    RenderRequestError,
    placeholder_media,
    preparar_render,
    safe_eval,
    validate_expr,
)

# Espejo documentado de `grafito-anim/src/protocol.rs::CANONICAL_TEMPLATES`
# (11). Si el protocolo suma una plantilla, este test lo grita: actualizar
# acá + `CANONICAL_SOLO_RUST` en `__main__.py` o implementar el render.
CANONICAL_11 = [
    "derivative-slope",
    "integral-area",
    "taylor-series",
    "conformal-map",
    "pitagoras",
    "euler",
    "fourier",
    "logistic-bifurcation",
    "gradient-field",
    "mobius-transform",
    "universal",
]

# Las 6 que el worker sí renderiza (ver `ALLOW_TEMPLATE` en `__main__.py`).
WORKER_6 = [
    "derivative-slope",
    "integral-area",
    "taylor-series",
    "conformal-map",
    "pitagoras",
    "universal",
]


def pedido(template="derivative-slope", **kwargs):
    base = {
        "job_id": "job-1",
        "template": template,
        "concept": "derivada",
        "export": "png",
        "canvas": [640, 480],
    }
    base.update(kwargs)
    return base


class TestLimitesExpr(unittest.TestCase):
    def test_exponente_gigante_rechaza_rapido(self):
        t0 = time.time()
        with self.assertRaises(ValueError):
            validate_expr("9**99999999")
        self.assertLess(
            time.time() - t0, 2.0, "9**99999999 debe fallar rápido, no colgar"
        )

    def test_literal_mayor_1e6_rechaza(self):
        for expr in ["1000001", "x+2000000", "9999999*1", "-3000000"]:
            with self.assertRaises(ValueError, msg=expr):
                validate_expr(expr)

    def test_literal_mas_de_6_digitos_rechaza(self):
        # 1000000 == 1e6 pasa el valor pero tiene 7 dígitos.
        with self.assertRaises(ValueError):
            validate_expr("1000000")
        self.assertEqual(MAX_LITERAL_DIGITOS, 6)

    def test_exponente_mayor_1000_rechaza(self):
        self.assertEqual(MAX_POW_EXPONENTE, 1000)
        for expr in ["2**1001", "x**5000", "2**(500+501)"]:
            with self.assertRaises(ValueError, msg=expr):
                validate_expr(expr)

    def test_legitimos_aceptan(self):
        for expr in [
            "x**2",
            "sin(x)+cos(x)",
            "x*2+1",
            "sqrt(x)",
            "999999",
            "2**10",
            "0.5*x",
            "2**(400+500)",
        ]:
            self.assertEqual(validate_expr(expr), expr, msg=expr)

    def test_safe_eval_gigante_err_rapido(self):
        t0 = time.time()
        with self.assertRaises(ValueError):
            safe_eval("9**99999999", 0.0)
        self.assertLess(time.time() - t0, 2.0)

    def test_safe_eval_legitimo(self):
        self.assertEqual(safe_eval("x**2", 3.0), 9.0)
        self.assertAlmostEqual(safe_eval("sin(x)", 0.0), 0.0)


class TestPlaceholderExclusivo(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.viejo_cwd = os.getcwd()
        os.chdir(self.tmp.name)

    def tearDown(self):
        os.chdir(self.viejo_cwd)
        self.tmp.cleanup()

    def test_symlink_plantado_err_y_victima_intacta(self):
        victima = os.path.join(self.tmp.name, "victima.bin")
        with open(victima, "wb") as fh:
            fh.write(b"ORIGINAL")
        os.symlink(victima, os.path.join(self.tmp.name, "job1.png"))
        with self.assertRaises(FileExistsError):
            placeholder_media("job1", "png", "derivada")
        with open(victima, "rb") as fh:
            self.assertEqual(fh.read(), b"ORIGINAL")
        # 2º intento: Err sin fallback que trunque.
        with self.assertRaises(FileExistsError):
            placeholder_media("job1", "png", "derivada")
        with open(victima, "rb") as fh:
            self.assertEqual(fh.read(), b"ORIGINAL")

    def test_destino_existente_err_sin_pisar(self):
        with open(os.path.join(self.tmp.name, "job2.png"), "wb") as fh:
            fh.write(b"MIO")
        with self.assertRaises(FileExistsError):
            placeholder_media("job2", "png", "x")
        with open(os.path.join(self.tmp.name, "job2.png"), "rb") as fh:
            self.assertEqual(fh.read(), b"MIO")

    def test_legitimo_escribe_png_valido(self):
        ruta = placeholder_media("ok1", "png", "derivada")
        with open(ruta, "rb") as fh:
            data = fh.read()
        self.assertTrue(
            data.startswith(bytes.fromhex("89504e47")),
            "el stub debe ser PNG válido, no 8 bytes",
        )
        self.assertGreater(len(data), 8)


class TestPrepararRender(unittest.TestCase):
    def test_export_invalido_err(self):
        with self.assertRaises(RenderRequestError) as ctx:
            preparar_render(pedido(export="exe"))
        self.assertEqual(ctx.exception.code, "invalid_request")

    def test_canvas_invalido_err(self):
        for canvas in [[99999, 99999], [0, 0], "grande", [640]]:
            with self.assertRaises(RenderRequestError, msg=str(canvas)) as ctx:
                preparar_render(pedido(canvas=canvas))
            self.assertEqual(ctx.exception.code, "invalid_request")

    def test_template_desconocido_err_invalid_request(self):
        with self.assertRaises(RenderRequestError) as ctx:
            preparar_render(pedido(template="no-existe-xyz"))
        self.assertEqual(ctx.exception.code, "invalid_request")

    def test_job_id_invalido_err(self):
        with self.assertRaises(RenderRequestError) as ctx:
            preparar_render(pedido(job_id="../escape"))
        self.assertEqual(ctx.exception.code, "invalid_request")

    def test_alias_pythagoras_ok(self):
        p = preparar_render(pedido(template="pythagoras"))
        self.assertEqual(p["template"], "pitagoras")

    def test_sin_plantilla_ruteo_por_concepto_ok(self):
        for t in ["", "auto"]:
            p = preparar_render(pedido(template=t))
            self.assertIn(p["template"], ("", t))

    def test_legitimo_ok(self):
        p = preparar_render(pedido())
        self.assertEqual(p["job_id"], "job-1")
        self.assertEqual(p["export"], "png")
        self.assertEqual(p["canvas"], (640, 480))
        self.assertEqual(p["template"], "derivative-slope")


class TestParidad11_6(unittest.TestCase):
    def test_worker_6_en_allow(self):
        for t in WORKER_6:
            self.assertIn(t, ALLOW_TEMPLATE, msg=t)

    def test_divergencia_exacta_5_solo_rust(self):
        self.assertEqual(
            set(CANONICAL_11) - set(WORKER_6),
            set(CANONICAL_SOLO_RUST),
            "la divergencia debe ser exactamente euler/fourier/logistic/"
            "gradient/mobius",
        )

    def test_matriz_fuera_de_allow_unsupported(self):
        for t in sorted(set(CANONICAL_11) - set(WORKER_6)):
            with self.assertRaises(RenderRequestError, msg=t) as ctx:
                preparar_render(pedido(template=t))
            self.assertEqual(
                ctx.exception.code,
                "unsupported",
                f"{t} debe ser unsupported, no imagen falsa",
            )

    def test_matriz_dentro_de_allow_ok(self):
        for t in WORKER_6:
            p = preparar_render(pedido(template=t))
            self.assertEqual(p["template"], t)


if __name__ == "__main__":
    unittest.main()
