#![allow(clippy::unwrap_used, clippy::expect_used)]
#![allow(deprecated)]
//! Igualdad AABB cacheado == muestreo completo.
//!
//! `object_world_aabb` deriva el AABB de curvas paramétricas/polares desde las
//! muestras cacheadas (4000 pts, idénticas a las que se dibujan). Estos tests
//! pinean que el hit de caché es bit-idéntico al muestreo frío y que ambos
//! coinciden con un fold manual independiente sobre el guard cacheado.
//!
//! Re-correr: `cargo test -p grafito-render --locked --test cache_regression`

use grafito_core::parametric_sampling::{samples_or_compute_curve_2d, samples_or_compute_polar};
use grafito_core::{Document, GeoObject, ParametricCurve2DObj, PolarCurveObj};
use grafito_geometry::{Point2, ViewTransform, AABB};
use grafito_render::object_world_aabb;
use std::f64::consts::TAU;

/// Fold manual con el mismo algoritmo que `samples_aabb` (privada en el crate):
/// primer punto finito como semilla, luego `expand`. Orden-independiente para
/// min/max, así que debe coincidir bit a bit con `object_world_aabb`.
fn fold_manual(samples: &[(f64, f64)]) -> Option<AABB> {
    let mut aabb: Option<AABB> = None;
    for &(x, y) in samples {
        if x.is_finite() && y.is_finite() {
            let punto = Point2::new(x, y);
            match &mut aabb {
                Some(aabb) => aabb.expand(&punto),
                None => aabb = Some(AABB::new(punto, punto)),
            }
        }
    }
    aabb
}

#[test]
fn aabb_cacheado_igual_a_muestreo_completo_curva() {
    let view = ViewTransform::new(800.0, 600.0);
    let document = Document::new();
    let objeto = GeoObject::ParametricCurve2D(ParametricCurve2DObj::new(
        "t",
        "sin(37*t)+0.5*sin(71*t)",
        0.0,
        TAU * 4.0,
    ));

    let frio = object_world_aabb(&view, &document, &objeto).expect("aabb frío curva");
    let hit = object_world_aabb(&view, &document, &objeto).expect("aabb hit curva");
    assert_eq!(
        frio, hit,
        "el hit de caché debe ser bit-idéntico al muestreo completo"
    );

    let GeoObject::ParametricCurve2D(curva) = &objeto else {
        panic!("el fixture debe seguir siendo una curva paramétrica");
    };
    let guard = samples_or_compute_curve_2d(curva, 4000, &document.variables);
    assert_eq!(
        guard.len(),
        4001,
        "el contrato es 4000 pasos → 4001 muestras"
    );
    assert_eq!(
        Some(frio),
        fold_manual(&guard),
        "el AABB debe coincidir con el fold manual de las 4000 muestras"
    );
    assert!(
        frio.min.x.is_finite()
            && frio.min.y.is_finite()
            && frio.max.x.is_finite()
            && frio.max.y.is_finite(),
        "AABB finito, got: {frio:?}"
    );
    assert!(
        frio.max.x >= frio.min.x && frio.max.y >= frio.min.y,
        "AABB no degenerado, got: {frio:?}"
    );
}

#[test]
fn aabb_cacheado_igual_a_muestreo_completo_polar() {
    let view = ViewTransform::new(800.0, 600.0);
    let document = Document::new();
    let objeto = GeoObject::PolarCurve(PolarCurveObj::new(
        "3+sin(9*t)+0.5*cos(23*t)",
        0.0,
        TAU * 2.0,
    ));

    let frio = object_world_aabb(&view, &document, &objeto).expect("aabb frío polar");
    let hit = object_world_aabb(&view, &document, &objeto).expect("aabb hit polar");
    assert_eq!(
        frio, hit,
        "el hit de caché polar debe ser bit-idéntico al muestreo completo"
    );

    let GeoObject::PolarCurve(polar) = &objeto else {
        panic!("el fixture debe seguir siendo una curva polar");
    };
    let guard = samples_or_compute_polar(polar, 4000, &document.variables);
    assert_eq!(
        guard.len(),
        4001,
        "el contrato es 4000 pasos → 4001 muestras"
    );
    assert_eq!(
        Some(frio),
        fold_manual(&guard),
        "el AABB polar debe coincidir con el fold manual de las 4000 muestras"
    );
}
