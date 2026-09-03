//! Análisis CAS — extraído de `commands.rs` para reducir el god file.
//!
//! Contiene handlers específicos para
//! TangentAt/NormalAt/ArcLength/CurvatureAt/Volume/Surface + ODE stubs.
//! Fase 1: mueve lógica CurvatureAt/SequenceLimit corregida y centraliza
//! helpers. Si el split completo es riesgoso, deja TODO para fase 2.

use grafito_core::{Document, GeoObject, LineObj};
use grafito_geometry::analysis::{
    arc_length, curvature_at, normal_line_at, surface_of_revolution, tangent_line_at,
    volume_of_revolution,
};
use grafito_geometry::Point2;

use crate::cas_parse::CasCmd;
use crate::commands::CommandOutcome;
use std::collections::HashMap;

fn try_insert_typed(document: &mut Document, object: GeoObject) -> Result<(), String> {
    let label = object.label().to_string();
    let mut obj = object;
    if !label.is_empty() && !document.object_ids_by_label(&label).is_empty() {
        let new_label = format!("{}_{}", label, document.object_count());
        obj.set_label(new_label);
    }
    document
        .try_add_object(obj)
        .map(|_| ())
        .map_err(|e| e.to_string())
}

fn require_finite_local(value: Result<f64, String>) -> Result<f64, String> {
    let v = value?;
    if v.is_finite() {
        Ok(v)
    } else {
        Err(format!("No es finito: {v}"))
    }
}

fn parse_numeric_arg_local(s: &str, variables: &HashMap<String, f64>) -> Result<f64, String> {
    crate::commands::parse_numeric_arg(s, variables)
}

fn substitute_document_vars_local(expr: &str, document: &Document) -> String {
    let mut out = expr.to_string();
    for (k, v) in &document.variables {
        if k == "x" || !v.is_finite() {
            continue;
        }
        out = crate::helpers::replace_variable(&out, k, &format!("({})", v));
    }
    out
}

pub(crate) fn handle_curvature_at(
    document: &mut Document,
    expr_raw: &str,
    x: f64,
) -> Result<String, String> {
    let expr = substitute_document_vars_local(expr_raw, document);
    match curvature_at(&expr, x) {
        Ok(kappa) => {
            if kappa.is_finite() && kappa.abs() > 1e-15 {
                let radius = 1.0 / kappa;
                if radius.is_finite() {
                    Ok(format!(
                        "Curvatura en x={:.4}: κ = {:.6}, Radio = {:.6}",
                        x, kappa, radius
                    ))
                } else {
                    Ok(format!(
                        "Curvatura en x={:.4}: κ = {:.6}, Radio = ∞ (recta)",
                        x, kappa
                    ))
                }
            } else {
                Ok(format!(
                    "Curvatura en x={:.4}: κ = {:.6}, Radio = ∞ (recta)",
                    x, kappa
                ))
            }
        }
        Err(e) => Err(format!("Error en CurvatureAt: {e}")),
    }
}

pub(crate) fn handle_tangent_at(
    document: &mut Document,
    expr_raw: &str,
    x: f64,
) -> Result<String, String> {
    let expr = substitute_document_vars_local(expr_raw, document);
    match tangent_line_at(&expr, x) {
        Ok((x0, fx, slope)) => {
            let p1 = Point2::new(x0, fx);
            let p2 = Point2::new(x0 + 1.0, fx + slope);
            try_insert_typed(
                document,
                GeoObject::Line(LineObj::new(p1, p2).with_label("tangente")),
            )?;
            Ok(format!(
                "Tangente en x={:.4}: y = {:.4} + {:.4}·(x − {:.4})",
                x0, fx, slope, x0
            ))
        }
        Err(e) => Err(format!("Error en TangentAt: {e}")),
    }
}

pub(crate) fn handle_normal_at(
    document: &mut Document,
    expr_raw: &str,
    x: f64,
) -> Result<String, String> {
    let expr = substitute_document_vars_local(expr_raw, document);
    match normal_line_at(&expr, x) {
        Ok((x0, fx, normal_slope)) => {
            let p1 = Point2::new(x0, fx);
            let p2 = if normal_slope.is_infinite() {
                Point2::new(x0, fx + 1.0)
            } else {
                Point2::new(x0 + 1.0, fx + normal_slope)
            };
            try_insert_typed(
                document,
                GeoObject::Line(LineObj::new(p1, p2).with_label("normal")),
            )?;
            Ok(format!("Normal en x={:.4}", x0))
        }
        Err(e) => Err(format!("Error en NormalAt: {e}")),
    }
}

pub(crate) fn handle_arc_length(
    document: &Document,
    expr_raw: &str,
    a: f64,
    b: f64,
) -> Result<String, String> {
    let expr = substitute_document_vars_local(expr_raw, document);
    match arc_length(&expr, a, b) {
        Ok(length) => Ok(format!(
            "Longitud de arco de {:.4} a {:.4}: {:.6}",
            a, b, length
        )),
        Err(e) => Err(format!("Error en ArcLength: {e}")),
    }
}

pub(crate) fn handle_volume_of_revolution(
    document: &Document,
    expr_raw: &str,
    a: f64,
    b: f64,
) -> Result<String, String> {
    let expr = substitute_document_vars_local(expr_raw, document);
    match volume_of_revolution(&expr, a, b) {
        Ok(volume) => Ok(format!(
            "Volumen de revolución de {:.4} a {:.4}: {:.6}",
            a, b, volume
        )),
        Err(e) => Err(format!("Error en VolumeOfRevolution: {e}")),
    }
}

pub(crate) fn handle_surface_of_revolution(
    document: &Document,
    expr_raw: &str,
    a: f64,
    b: f64,
) -> Result<String, String> {
    let expr = substitute_document_vars_local(expr_raw, document);
    match surface_of_revolution(&expr, a, b) {
        Ok(surface) => Ok(format!(
            "Superficie de revolución de {:.4} a {:.4}: {:.6}",
            a, b, surface
        )),
        Err(e) => Err(format!("Error en SurfaceOfRevolution: {e}")),
    }
}

#[allow(dead_code)]
pub(crate) fn cmd_err(msg: impl Into<String>) -> CommandOutcome {
    CommandOutcome::Error(msg.into())
}

#[allow(dead_code)]
pub(crate) const TODO_MIGRATION: &str =
    "TODO fase 2: migrar Derivative/Integral/Solve/Taylor/Limit* completos";

pub(crate) fn try_execute_analysis_command(
    document: &mut Document,
    cmd: &CasCmd,
) -> Option<Result<String, String>> {
    let numeric_variables = document.variables.clone();
    let finite_arg = |index: usize, name: &str| {
        let value = cmd
            .args
            .get(index)
            .ok_or_else(|| format!("falta el argumento {name}"))?;
        require_finite_local(parse_numeric_arg_local(value, &numeric_variables))
            .map_err(|error| format!("argumento {name} inválido: {error}"))
    };
    match cmd.command.as_str() {
        "TangentAt" => {
            let expr_raw = cmd.args.first()?.trim();
            let x = match finite_arg(1, "x") {
                Ok(v) => v,
                Err(e) => return Some(Err(format!("TangentAt: {e}"))),
            };
            Some(handle_tangent_at(document, expr_raw, x))
        }
        "NormalAt" => {
            let expr_raw = cmd.args.first()?.trim();
            let x = match finite_arg(1, "x") {
                Ok(v) => v,
                Err(e) => return Some(Err(format!("NormalAt: {e}"))),
            };
            Some(handle_normal_at(document, expr_raw, x))
        }
        "ArcLength" => {
            let expr_raw = cmd.args.first()?.trim();
            let a = match finite_arg(1, "a") {
                Ok(v) => v,
                Err(e) => return Some(Err(format!("ArcLength: {e}"))),
            };
            let b = match finite_arg(2, "b") {
                Ok(v) => v,
                Err(e) => return Some(Err(format!("ArcLength: {e}"))),
            };
            Some(handle_arc_length(document, expr_raw, a, b))
        }
        "CurvatureAt" => {
            let expr_raw = cmd.args.first()?.trim();
            let x = match finite_arg(1, "x") {
                Ok(v) => v,
                Err(e) => return Some(Err(format!("CurvatureAt: {e}"))),
            };
            Some(handle_curvature_at(document, expr_raw, x))
        }
        "VolumeOfRevolution" => {
            let expr_raw = cmd.args.first()?.trim();
            let a = match finite_arg(1, "a") {
                Ok(v) => v,
                Err(e) => return Some(Err(format!("VolumeOfRevolution: {e}"))),
            };
            let b = match finite_arg(2, "b") {
                Ok(v) => v,
                Err(e) => return Some(Err(format!("VolumeOfRevolution: {e}"))),
            };
            Some(handle_volume_of_revolution(document, expr_raw, a, b))
        }
        "SurfaceOfRevolution" => {
            let expr_raw = cmd.args.first()?.trim();
            let a = match finite_arg(1, "a") {
                Ok(v) => v,
                Err(e) => return Some(Err(format!("SurfaceOfRevolution: {e}"))),
            };
            let b = match finite_arg(2, "b") {
                Ok(v) => v,
                Err(e) => return Some(Err(format!("SurfaceOfRevolution: {e}"))),
            };
            Some(handle_surface_of_revolution(document, expr_raw, a, b))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use grafito_core::Document;
    #[test]
    fn curvature_straight_line_infinite_radius() {
        let mut doc = Document::new();
        let res = handle_curvature_at(&mut doc, "x", 0.0).unwrap();
        assert!(res.contains("∞") || !res.contains("inf"));
        assert!(res.contains("∞ (recta)") || res.contains("recta"));
        assert!(!res.contains("inf"));
    }
    #[test]
    fn curvature_nonzero_finite() {
        let mut doc = Document::new();
        let res = handle_curvature_at(&mut doc, "x^2", 0.0).unwrap();
        assert!(res.contains("κ"));
        assert!(!res.to_lowercase().contains("inf") || res.contains("∞ (recta)"));
    }
    #[test]
    fn cmd_err_helper() {
        let out = cmd_err("test error");
        match out {
            CommandOutcome::Error(msg) => assert_eq!(msg, "test error"),
            _ => panic!(),
        }
    }
}
