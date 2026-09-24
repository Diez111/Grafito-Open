//! Frente fantasma-2: los 72 comandos que ya tenían brazo de handler en
//! `commands.rs` y cero apariciones en `command_registry.rs` (no salían en la
//! paleta, la ayuda ni el catálogo del asistente) — segunda tanda del mismo
//! bug de visibilidad del frente fantasma+nuevos.
//!
//! Acá se registran 71: `Image` queda SIN spec a propósito porque su brazo
//! (`commands.rs`, `"Image"`) es un stub que solo devuelve error honesto (no
//! hay modelo persistente de imagen en el documento); queda documentado en
//! `command_registry::registry_tests::orphan_detection_reports_counts`.
//!
//! Cada test (1) resuelve el spec (visibilidad) y (2) ejecuta el comando real
//! vía `process_input` con una invocación que debe responder Ok/Message.

#![allow(clippy::unwrap_used, clippy::expect_used)]
#![allow(deprecated)]
use grafito_command::command_registry;
use grafito_command::commands::{process_input, CommandOutcome};
use grafito_core::{Document, GeoObject};

fn run(doc: &mut Document, text: &str) -> CommandOutcome {
    let mut input = text.to_string();
    process_input(doc, &mut input)
}

/// El comando debe estar registrado, ser visible en la paleta y normalizarse.
fn assert_registered_visible(canonical: &str) {
    let spec = command_registry::resolve(canonical)
        .unwrap_or_else(|| panic!("{canonical} debe estar en command_registry"));
    assert_eq!(
        spec.canonical, canonical,
        "{canonical} resuelve a otro spec"
    );
    assert!(
        spec.palette_visible,
        "{canonical} debe verse en la paleta de comandos"
    );
    assert!(
        command_registry::canonicalize(canonical).is_some(),
        "{canonical} debe normalizarse en el parser"
    );
}

/// Ejecuta el comando y exige que responda (Ok o Message), nunca error.
fn assert_ejecuta(doc: &mut Document, invocacion: &str) {
    let out = run(doc, invocacion);
    assert!(
        matches!(out, CommandOutcome::Ok | CommandOutcome::Message(_)),
        "{invocacion} debe responder Ok/Message, got {out:?}"
    );
}

fn etiqueta(doc: &Document, variante: &str) -> String {
    doc.objects_iter()
        .find(|(_, obj)| match obj {
            GeoObject::Point3D(_) => variante == "punto3d",
            GeoObject::Line3D(_) => variante == "recta3d",
            GeoObject::Plane3D(_) => variante == "plano3d",
            GeoObject::Point(_) => variante == "punto",
            GeoObject::Line(_) => variante == "recta",
            _ => false,
        })
        .map(|(_, obj)| obj.label().to_string())
        .unwrap_or_else(|| panic!("falta un objeto {variante} en el documento"))
}

// ── visibilidad: los 71 specs resuelven y son visibles ─────────────────────

#[test]
fn los_71_fantasmas_resuelven_en_el_registro() {
    for canonical in [
        // Matrices (20)
        "Trace",
        "Rank",
        "NullSpace",
        "LU",
        "QR",
        "Cholesky",
        "ConditionNumber",
        "P2Dependence",
        "P2Basis",
        "P2Equations",
        "SubspaceDimension",
        "SubspaceBasis",
        "SubspaceIntersection",
        "OrthogonalComplement",
        "MatrixParamSolve",
        "GaussJordanSolve",
        "Cofactor",
        "Adjugate",
        "LaplaceExpansion",
        "LinearTransformationMatrix",
        // AM2 (13)
        "Gradient",
        "CriticalPoints",
        "DirectionalDerivative",
        "TangentPlane",
        "Divergence",
        "Curl",
        "SurfaceArea",
        "LineIntegralScalar",
        "SurfaceIntegralScalar",
        "IsConservative",
        "PotentialFunction",
        "StokesTheorem",
        "ChangeOfVariables",
        // AM1 (7)
        "RolleCheck",
        "CauchyMeanValueCheck",
        "AlternatingSeriesTest",
        "IntegralTest",
        "AbsoluteConvergence",
        "RatioTest",
        "RootTest",
        // Crear (7)
        "Cardioid",
        "Rose",
        "ArchimedeanSpiral",
        "LogarithmicSpiral",
        "Lissajous",
        "Epicycloid",
        "Hypocycloid",
        // CAS (7)
        "LnGamma",
        "BesselJ",
        "BesselY",
        "BesselI",
        "Erf",
        "Erfc",
        "Digamma",
        // 3D (7)
        "EquidistantFrom",
        "Solve3DGeometry",
        "Projection3D",
        "PlaneThroughLines",
        "PlaneThroughLinePoint",
        "LineRelation3D",
        "SolveLine3DParameters",
        // Construir (3)
        "PointOnObject",
        "CircleByCenterRadius",
        "CircleByThreePoints",
        // Dinámica (3)
        "Script",
        "Erase",
        "EraseAll",
        // Estadística (2)
        "CIMean",
        "CIProportion",
        // Análisis / Complejos (2)
        "FunctionInspector",
        "ComplexSymbol",
    ] {
        assert_registered_visible(canonical);
    }
}

#[test]
fn image_no_se_registra_por_ser_stub_honesto() {
    // El brazo existe pero responde error "no disponible": registrarle spec lo
    // promocionaría a paleta sin implementación. Queda fuera del registro.
    let mut doc = Document::new();
    let out = run(&mut doc, "Image[ruta]");
    assert!(
        matches!(out, CommandOutcome::Error(ref m) if m.contains("no está disponible")),
        "Image debe seguir respondiendo error honesto, got {out:?}"
    );
    assert!(
        command_registry::resolve("Image").is_none(),
        "Image no debe estar en el registro hasta que exista modelo de imagen"
    );
}

// ── ejecución real: matrices y álgebra lineal ──────────────────────────────

#[test]
fn matrices_fantasmas_ejecutan() {
    let mut doc = Document::new();
    for caso in [
        "Trace[[[1, 2], [3, 4]]]",
        "Rank[[[1, 2], [2, 4]]]",
        "NullSpace[[[1, 2], [2, 4]]]",
        "LU[[[1, 2], [3, 4]]]",
        "QR[[[1, 2], [3, 4]]]",
        "Cholesky[[[2, 1], [1, 2]]]",
        "ConditionNumber[[[1, 2], [3, 4]]]",
        "P2Dependence[{x, x^2, x + x^2}]",
        "P2Basis[{1, x, x^2}]",
        "P2Equations[{x, x^2}]",
        "SubspaceDimension[[[1, 0], [2, 0]]]",
        "SubspaceBasis[[[1, 0], [2, 0]]]",
        "SubspaceIntersection[[[1, 0]], [[1, 0]]]",
        "OrthogonalComplement[[[1, 0]]]",
        "MatrixParamSolve[[[t, 0], [0, 1]], t]",
        "GaussJordanSolve[[[2, 1], [1, 3]], [5, 10]]",
        "Cofactor[[[1, 2], [3, 4]], 1, 1]",
        "Adjugate[[[1, 2], [3, 4]]]",
        "LaplaceExpansion[[[1, 2], [3, 4]], row, 1]",
        "LinearTransformationMatrix[[[1, 0], [0, 1]], [[2, 0], [0, 2]]]",
    ] {
        assert_ejecuta(&mut doc, caso);
    }
}

// ── ejecución real: cálculo vectorial y series (AM1/AM2) ───────────────────

#[test]
fn calculo_vectorial_fantasma_ejecuta() {
    let mut doc = Document::new();
    for caso in [
        "Gradient[x^2 + y^2, [x, y]]",
        "CriticalPoints[x^2 + y^2, [x, y], -1, 1, -1, 1]",
        "DirectionalDerivative[x^2 + y^2, [x, y], [1, 1], [1, 0]]",
        "TangentPlane[x^2 + y^2, [1, 1], [x, y]]",
        "Divergence[[x^2, y^2], [x, y]]",
        "Curl[[x*y, y^2], [x, y]]",
        "SurfaceArea[1, x, 0, 1, y, 0, 1, 8]",
        "LineIntegralScalar[1, [t, 0], t, 0, 2, 200]",
        "SurfaceIntegralScalar[1, [u, v, u + v], [u, v], 0, 1, 0, 1, 8]",
        "IsConservative[[2*x*y, x^2], [x, y]]",
        "PotentialFunction[[2*x*y, x^2], [x, y]]",
        "StokesTheorem[[x, y, z], [u, v, u + v], [u, v], 0, 1, 0, 1, 8]",
        "ChangeOfVariables[1, [u + v, u - v], [u, v]]",
    ] {
        assert_ejecuta(&mut doc, caso);
    }
}

#[test]
fn teoremas_y_series_fantasmas_ejecutan() {
    let mut doc = Document::new();
    for caso in [
        "RolleCheck[x^2 - 1, x, -1, 1]",
        "CauchyMeanValueCheck[x^2, x^3, x, 1, 2]",
        "AlternatingSeriesTest[(-1)^n/n, n]",
        "IntegralTest[1/n^2, n, 1]",
        "AbsoluteConvergence[(-1)^n/n^2, n]",
        "RatioTest[1/n^2, n]",
        "RootTest[1/n^2, n]",
    ] {
        assert_ejecuta(&mut doc, caso);
    }
}

// ── ejecución real: curvas clásicas, funciones especiales y estadística ────

#[test]
fn curvas_clasicas_fantasmas_ejecutan() {
    let mut doc = Document::new();
    for caso in [
        "Cardioid[1]",
        "Rose[1, 3, 1]",
        "ArchimedeanSpiral[1, 2, 6.28]",
        "LogarithmicSpiral[1, 0.2, 6.28]",
        "Epicycloid[1, 3]",
        "Hypocycloid[1, 3]",
    ] {
        assert_ejecuta(&mut doc, caso);
    }
    assert_eq!(
        doc.object_count(),
        6,
        "cada curva clásica debe crear su polígono muestreado"
    );
}

#[test]
fn lissajous_inserta_su_poligono_autointersectado() {
    // REGRESIÓN (antes era bug conocido): el polígono de la curva de
    // Lissajous es auto-intersecado y `grafito-core/src/validation.rs` lo
    // rechazaba midiendo la degeneración con shoelace CON SIGNO, cuyo área
    // algebraica neta se anula por simetría. Ahora la degeneración se mide
    // con la suma de áreas ABSOLUTAS de los triángulos abanicados, así que
    // los lazos insertan y los colineales siguen rechazándose.
    assert_registered_visible("Lissajous");
    let mut doc = Document::new();
    for caso in [
        "Lissajous[1, 1, 3, 2, 0]",
        "Lissajous[1, 1, 1, 2, 1.5707963267948966]",
        "Lissajous[2, 1, 3, 2, 0.5]",
    ] {
        let antes = doc.object_count();
        assert_ejecuta(&mut doc, caso);
        assert_eq!(
            doc.object_count(),
            antes + 1,
            "{caso} debe crear su polígono muestreado"
        );
    }
}

#[test]
fn funciones_especiales_fantasmas_ejecutan() {
    let mut doc = Document::new();
    for caso in [
        "LnGamma[3]",
        "BesselJ[0, 1]",
        "BesselY[0, 1]",
        "BesselI[0, 1]",
        "Erf[1]",
        "Erfc[1]",
        "Digamma[3]",
    ] {
        assert_ejecuta(&mut doc, caso);
    }
}

#[test]
fn intervalos_de_confianza_fantasmas_ejecutan() {
    let mut doc = Document::new();
    assert_ejecuta(&mut doc, "CIMean[{1, 2, 3, 4}]");
    assert_ejecuta(&mut doc, "CIProportion[3, 10]");
    assert_ejecuta(&mut doc, "FunctionInspector[x^2]");
}

// ── ejecución real: scripting, borrado y símbolo complejo ──────────────────

#[test]
fn scripting_y_borrado_fantasmas_ejecutan() {
    let mut doc = Document::new();
    assert_ejecuta(&mut doc, "ComplexSymbol[w]");
    assert_ejecuta(&mut doc, "Script[(0, 0); (1, 1)]");
    assert_eq!(doc.object_count(), 2, "Script debe crear los dos puntos");
    let primero = doc
        .objects_iter()
        .find_map(|(_, obj)| match obj {
            GeoObject::Point(p) => Some(p.label.clone()),
            _ => None,
        })
        .unwrap_or_else(|| panic!("Script debe crear un punto con etiqueta"));

    let borrado = run(&mut doc, &format!("Erase[{primero}]"));
    assert!(
        matches!(borrado, CommandOutcome::Message(ref m) if m.contains("borrado")),
        "Erase[{primero}] debe borrar el punto, got {borrado:?}"
    );
    assert_eq!(doc.object_count(), 1, "Erase debe sacar un objeto");

    let limpio = run(&mut doc, "EraseAll[]");
    assert!(
        matches!(limpio, CommandOutcome::Message(ref m) if m.contains("borrado")),
        "EraseAll debe borrar todo, got {limpio:?}"
    );
    assert_eq!(doc.object_count(), 0, "EraseAll debe vaciar el documento");
}

// ── ejecución real: construcciones y 3D (con objetos previos) ──────────────

#[test]
fn construcciones_fantasmas_ejecutan() {
    let mut doc = Document::new();
    run(&mut doc, "Line[(0, 0), (2, 0)]");
    run(&mut doc, "Point[(1, 3)]");
    run(&mut doc, "Point[(0, 0)]");
    run(&mut doc, "Point[(2, 0)]");
    let recta = etiqueta(&doc, "recta");
    // Las etiquetas auto de los tres puntos libres son distintas; las capturo
    // por orden de creación.
    let puntos: Vec<String> = doc
        .objects_iter()
        .filter_map(|(_, obj)| match obj {
            GeoObject::Point(p) => Some(p.label.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(puntos.len(), 3, "tres puntos libres creados: {puntos:?}");
    let (p0, p1, p2) = (puntos[0].clone(), puntos[1].clone(), puntos[2].clone());

    assert_ejecuta(&mut doc, &format!("PointOnObject[{recta}, {p0}]"));
    assert_ejecuta(&mut doc, &format!("CircleByCenterRadius[{p1}, 2]"));
    assert_ejecuta(&mut doc, &format!("CircleByThreePoints[{p0}, {p1}, {p2}]"));
}

#[test]
fn geometria_3d_fantasma_ejecuta() {
    let mut doc = Document::new();
    // Plano y dos rectas paralelas (coplanarias) + un punto fuera del plano
    // de la primera recta, como en analytic_geometry_3d.rs.
    run(&mut doc, "Plane3D[1, 0, 1, 4]");
    run(&mut doc, "Line3D[0, 0, 0, 1, 0, 0]");
    run(&mut doc, "Line3D[0, 1, 0, 1, 0, 0]");
    run(&mut doc, "Point3D[0, 0, 2]");
    let plano = etiqueta(&doc, "plano3d");
    let punto = etiqueta(&doc, "punto3d");
    let rectas: Vec<String> = doc
        .objects_iter()
        .filter_map(|(_, obj)| match obj {
            GeoObject::Line3D(l) => Some(l.label.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(rectas.len(), 2, "dos rectas 3D creadas: {rectas:?}");
    let (r1, r2) = (rectas[0].clone(), rectas[1].clone());

    assert_ejecuta(
        &mut doc,
        &format!("EquidistantFrom[{plano}, {r1}, \"y-axis\"]"),
    );
    assert_ejecuta(
        &mut doc,
        &format!("Solve3DGeometry[\"dist(P,{plano})=dist(P,{r1})\", y, \"P=(0,y,0)\"]"),
    );
    assert_ejecuta(&mut doc, &format!("Projection3D[{punto}, {plano}]"));
    assert_ejecuta(&mut doc, &format!("PlaneThroughLines[{r1}, {r2}]"));
    assert_ejecuta(&mut doc, &format!("PlaneThroughLinePoint[{r1}, {punto}]"));
    assert_ejecuta(&mut doc, &format!("LineRelation3D[{r1}, {r2}]"));
    assert_ejecuta(
        &mut doc,
        "SolveLine3DParameters[[t, 1, 0], perpendicular, [1, 0, 1], t]",
    );
}
