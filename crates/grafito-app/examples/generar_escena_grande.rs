//! Genera una escena grande para medición de RSS (`docs/profiling.md` T2).
//!
//! Uso:
//! ```sh
//! cargo run --release -p grafito-app --example generar_escena_grande [/tmp/escena.json]
//! /usr/bin/grafito /tmp/escena.json &  # o el binario a medir
//! grep -E 'VmRSS|VmHWM' /proc/$!/status  # tras ~10 s idle
//! ```
//!
//! La escena (2000 objetos etiquetados, bajo `MAX_OBJECT_COUNT`) usa solo
//! expresiones del corpus que la app acepta, así el `serialize_document`
//! valida sin rechazos.

use grafito_core::{
    CircleObj, Document, FunctionObj, GeoObject, ImplicitCurveObj, ParametricCurve2DObj, PointObj,
    PolygonObj, RelationOperator, VectorField2DObj,
};
use grafito_geometry::Point2;

fn agregar(doc: &mut Document, obj: GeoObject) {
    if let Err(error) = doc.try_add_object(obj) {
        panic!("la escena debe agregar sin rechazos: {error}");
    }
}

fn main() {
    let destino = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "/tmp/escena_grande.json".to_string());
    let mut doc = Document::new();
    for i in 0..1200 {
        let expr = match i % 4 {
            0 => format!("sin({} * x)", i + 1),
            1 => format!("x^2 / {} - {i}", i + 2),
            2 => format!("{i} * cos(x / {})", i + 1),
            _ => format!("exp(-x^2 / {})", i + 1),
        };
        let mut fun = FunctionObj::new(expr);
        fun.label = format!("f{i}");
        agregar(&mut doc, GeoObject::Function(fun));
    }
    for i in 0..300 {
        let mut pc = ParametricCurve2DObj::new("cos(t)", "sin(t)", 0.0, std::f64::consts::TAU);
        pc.label = format!("c{i}");
        agregar(&mut doc, GeoObject::ParametricCurve2D(pc));
    }
    for _ in 0..200 {
        agregar(
            &mut doc,
            GeoObject::VectorField2D(VectorField2DObj::new("y", "-x")),
        );
    }
    for i in 0..100 {
        agregar(
            &mut doc,
            GeoObject::ImplicitCurve(ImplicitCurveObj::new(
                &format!("x^2 + y^2 - {}", (i + 1) * (i + 1)),
                "0",
                RelationOperator::Eq,
            )),
        );
    }
    for i in 0..100 {
        let x = f64::from(i) * 0.5;
        agregar(
            &mut doc,
            GeoObject::Point(PointObj::new(Point2::new(x, x.sin()))),
        );
    }
    for i in 0..50 {
        agregar(
            &mut doc,
            GeoObject::Circle(CircleObj::new(Point2::new(f64::from(i), 0.0), 1.0)),
        );
    }
    for _ in 0..50 {
        agregar(
            &mut doc,
            GeoObject::Polygon(PolygonObj::new(vec![
                Point2::new(0.0, 0.0),
                Point2::new(1.0, 0.0),
                Point2::new(1.0, 1.0),
            ])),
        );
    }
    assert_eq!(doc.object_count(), 2000);
    let json = match grafito_core::serialize_document(&doc) {
        Ok(json) => json,
        Err(error) => {
            eprintln!("la escena debe serializar: {error}");
            std::process::exit(1);
        }
    };
    if let Err(error) = std::fs::write(&destino, &json) {
        eprintln!("escribir escena: {error}");
        std::process::exit(1);
    }
    println!(
        "escena: {} objetos, {} bytes -> {destino}",
        doc.object_count(),
        json.len()
    );
}
