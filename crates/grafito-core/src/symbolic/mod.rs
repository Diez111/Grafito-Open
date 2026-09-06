//! Paridad GeoGebra honesta del cerebro (frentes F10-C y G-A).
//!
//! Reúne los S/M cerrados sin pisar geometría exacta, A11Y ni perf:
//! CSV RFC 4180 ([`csv`]), volumen/área 3D y vistas ortográficas
//! ([`solids`]), capas/tabla viva/SVG/PDF/stubs ([`exchange`]), puerta
//! CAS G-A ([`cas_motor`]: Gruntz, Risch-Norman, EDO 1er orden,
//! Laurent/residuos) y Buchberger acotado para Groebner.

pub mod cas_motor;
pub mod csv;
pub mod exchange;
pub mod solids;

pub use cas_motor::{
    cas_definite_risch, cas_groebner, cas_integrate_risch, cas_limit_gruntz,
    cas_limit_gruntz_infinite, cas_principal_part, cas_residue, cas_solve_ode,
    cas_solve_ode_linear, cas_solve_ode_separable,
};

pub use csv::{escape_field, parse_csv, to_csv, CsvError, MAX_CSV_BYTES, MAX_CSV_ROWS};
pub use exchange::{
    bar_chart_stub, clipboard_png_stub, clipboard_svg, datatable_cell, datatable_rows,
    datatable_to_csv, document_to_pdf, document_to_svg, l_stub, pie_chart_stub, ExchangeError,
    LayerTable, MAX_EXCHANGE_OBJECTS, MAX_LAYERS, MAX_TABLE_ROWS,
};
pub use solids::{
    cone_area, cone_volume, cube_area, cube_volume, cylinder_area, cylinder_volume, project_ortho,
    solid_area, solid_measure_status, solid_volume, sphere_area, sphere_volume, tetrahedron_area,
    tetrahedron_volume, torus_area, torus_volume, OrthoView, SolidError,
};

/// Puerta Groebner por Buchberger acotado (`grafito_geometry::cas`).
///
/// Resuelve sistemas polinómicos de hasta `MAX_GROEBNER_POLYS` 8
/// polinomios en `MAX_GROEBNER_VARS` 4 variables con un máximo de
/// `MAX_GROEBNER_S_POLY` 128 S-polinomios (criterio de pares primos
/// relativos incluido). Más allá, o ante entrada no polinómica, devuelve
/// error honesto que deriva a `Eliminate[...]`.
///
/// El Buchberger general sin cotas sigue siendo L en Tasks.md F10.W5.
pub fn groebner_gate(polys: &[String], vars: &[String]) -> Result<String, String> {
    match grafito_geometry::cas::buchberger_basis(polys, vars) {
        Ok(outcome) => Ok(format!("{{{}}}", outcome.basis.join(", "))),
        Err(grafito_geometry::cas::CasError::Unsupported { hint, .. }) => Err(format!(
            "{hint}; usa Eliminate[...] o reduce el sistema. Buchberger general en Tasks.md F10.W5"
        )),
        Err(grafito_geometry::cas::CasError::ResourceLimit { detail }) => Err(detail),
        Err(other) => Err(format!(
            "Groebner no convergió ({other}); usa Eliminate[...]"
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn groebner_gate_rejects_over_budget_honestly() {
        let polys: Vec<String> = (0..20).map(|i| format!("x + {i}")).collect();
        let vars = vec!["x".to_string()];
        let err = groebner_gate(&polys, &vars).expect_err(">cota debe fallar honesto");
        assert!(err.contains("Eliminate"));
    }

    #[test]
    fn groebner_gate_solves_linear_2x2() {
        let polys = vec!["x + y - 3".to_string(), "x - y - 1".to_string()];
        let vars = vec!["x".to_string(), "y".to_string()];
        let base = groebner_gate(&polys, &vars).expect("2x2 lineal");
        assert!(!base.contains("no implementado"));
    }

    #[test]
    fn groebner_gate_solves_linear_3x3_bounded() {
        let polys = vec![
            "x+y+z-6".to_string(),
            "x-y+z-2".to_string(),
            "2*x+y-z-1".to_string(),
        ];
        let vars = vec!["x".to_string(), "y".to_string(), "z".to_string()];
        let base = groebner_gate(&polys, &vars).expect("3x3 lineal acotado");
        assert!(!base.contains("no implementado"));
        assert!(base.contains('x'));
    }

    #[test]
    fn groebner_gate_rejects_nonpolynomial_honestly() {
        let polys = vec!["sin(x)+y".to_string(), "x-y".to_string()];
        let vars = vec!["x".to_string(), "y".to_string()];
        let err = groebner_gate(&polys, &vars).expect_err("no polinomio");
        assert!(err.contains("Eliminate"));
    }
}
