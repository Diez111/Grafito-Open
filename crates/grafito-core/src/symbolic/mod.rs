//! Paridad GeoGebra honesta del cerebro (frente F10-C).
//!
//! Reúne los S/M cerrados sin pisar geometría exacta, A11Y ni perf:
//! CSV RFC 4180 ([`csv`]), volumen/área 3D y vistas ortográficas
//! ([`solids`]), capas/tabla viva/SVG/PDF/stubs ([`exchange`]) y la
//! compuerta honesta de Groebner (exacta 2×2 lineal, error >2×2).

pub mod csv;
pub mod exchange;
pub mod solids;

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

/// Límite honesto de Groebner: el motor (`grafito_geometry::symbolic`)
/// resuelve exacto 2 polinomios lineales en 2 variables; más allá se
/// devuelve error explicativo que deriva a `Eliminate`.
///
/// El diseño completo (Buchberger) es L en Tasks.md F10.W5.
pub fn groebner_gate(polys: &[String], vars: &[String]) -> Result<String, String> {
    if polys.len() > 2 || vars.len() > 2 {
        return Err(format!(
            "Groebner limitado a 2 polinomios lineales en 2 variables (recibidos {}x{}); usa Eliminate[...] o reduce el sistema. Buchberger completo en Tasks.md F10.W5",
            polys.len(),
            vars.len()
        ));
    }
    match grafito_geometry::symbolic::groebner_basis_typed(polys, vars) {
        grafito_geometry::MathResult::Exact(base) => Ok(base),
        other => Err(format!("Groebner no convergió: {other:?}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn groebner_gate_rejects_bigger_than_2x2() {
        let polys = vec![
            "x+y+z".to_string(),
            "x-y+z".to_string(),
            "2*x+y-z".to_string(),
        ];
        let vars = vec!["x".to_string(), "y".to_string(), "z".to_string()];
        let err = groebner_gate(&polys, &vars).expect_err("3x3 debe fallar honesto");
        assert!(err.contains("2 polinomios lineales"));
        assert!(err.contains("Eliminate"));
    }

    #[test]
    fn groebner_gate_solves_linear_2x2() {
        let polys = vec!["x + y - 3".to_string(), "x - y - 1".to_string()];
        let vars = vec!["x".to_string(), "y".to_string()];
        let base = groebner_gate(&polys, &vars).expect("2x2 lineal");
        assert!(!base.contains("no implementado"));
    }
}
