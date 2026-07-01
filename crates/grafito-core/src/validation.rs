//! Document validation to mitigate untrusted save-file DoS.

use crate::{Document, GeoObject};
use serde_json::Value;

pub const MAX_DOCUMENT_SIZE_BYTES: usize = 10_000_000;
pub const MAX_JSON_DEPTH: usize = 64;
pub const MAX_STRING_LENGTH: usize = 10_000;
pub const MAX_ARRAY_LENGTH: usize = 200_000;
pub const MAX_OBJECT_COUNT: usize = 5_000;
pub const MAX_EXPR_LENGTH: usize = 2_000;
pub const MAX_DENSITY: usize = 500;
pub const MAX_FRACTAL_RESOLUTION: usize = 1_000;
pub const MAX_FRACTAL_ITER: u32 = 10_000;
pub const MAX_ATTRACTOR_STEPS: usize = 500_000;
pub const MAX_SURFACE_MESH_RES: usize = 200;
pub const MAX_HYPERSURFACE_RES: usize = 100;

/// Validate the raw JSON before deserializing into a `Document`.
fn validate_text_nesting(json: &str) -> Result<(), String> {
    let mut depth: usize = 0;
    let mut max_depth: usize = 0;
    let mut in_string = false;
    let mut escape = false;
    for c in json.chars() {
        if in_string {
            if escape {
                escape = false;
            } else if c == '\\' {
                escape = true;
            } else if c == '"' {
                in_string = false;
            }
            continue;
        }
        match c {
            '"' => in_string = true,
            '[' | '{' => {
                depth = depth.saturating_add(1);
                max_depth = max_depth.max(depth);
                if max_depth > MAX_JSON_DEPTH {
                    return Err("Document JSON is too deeply nested".to_string());
                }
            }
            ']' | '}' => {
                depth = depth.saturating_sub(1);
            }
            _ => {}
        }
    }
    Ok(())
}

pub fn validate_document_json(json: &str) -> Result<(), String> {
    if json.len() > MAX_DOCUMENT_SIZE_BYTES {
        return Err(format!(
            "Document size {} exceeds maximum {}",
            json.len(),
            MAX_DOCUMENT_SIZE_BYTES
        ));
    }
    validate_text_nesting(json)?;
    let value: Value = serde_json::from_str(json).map_err(|e| e.to_string())?;
    validate_value(&value, 0, &mut 0)?;
    Ok(())
}

fn validate_value(value: &Value, depth: usize, nodes: &mut usize) -> Result<(), String> {
    if depth > MAX_JSON_DEPTH {
        return Err("Document JSON is too deeply nested".to_string());
    }
    *nodes += 1;
    if *nodes > 1_000_000 {
        return Err("Document JSON contains too many nodes".to_string());
    }

    match value {
        Value::Null | Value::Bool(_) | Value::Number(_) => Ok(()),
        Value::String(s) => {
            if s.len() > MAX_STRING_LENGTH {
                return Err(format!(
                    "String length {} exceeds maximum {}",
                    s.len(),
                    MAX_STRING_LENGTH
                ));
            }
            Ok(())
        }
        Value::Array(arr) => {
            if arr.len() > MAX_ARRAY_LENGTH {
                return Err(format!(
                    "Array length {} exceeds maximum {}",
                    arr.len(),
                    MAX_ARRAY_LENGTH
                ));
            }
            for v in arr {
                validate_value(v, depth + 1, nodes)?;
            }
            Ok(())
        }
        Value::Object(map) => {
            if map.len() > MAX_ARRAY_LENGTH {
                return Err(format!(
                    "Object field count {} exceeds maximum {}",
                    map.len(),
                    MAX_ARRAY_LENGTH
                ));
            }
            for (k, v) in map {
                if k.len() > MAX_STRING_LENGTH {
                    return Err(format!(
                        "Object key length {} exceeds maximum {}",
                        k.len(),
                        MAX_STRING_LENGTH
                    ));
                }
                validate_value(v, depth + 1, nodes)?;
            }
            Ok(())
        }
    }
}

/// Validate a deserialized document, capping expensive object parameters.
pub fn validate_document(doc: &Document) -> Result<(), String> {
    let count = doc.object_count();
    if count > MAX_OBJECT_COUNT {
        return Err(format!(
            "Document contains {} objects, maximum is {}",
            count, MAX_OBJECT_COUNT
        ));
    }

    // Limit the total number of constraints to bound the cost of cycle
    // detection / topological sort in `get_update_order`.
    if doc.constraints.constraint_count() > MAX_OBJECT_COUNT {
        return Err(format!(
            "Document contains {} constraints, maximum is {}",
            doc.constraints.constraint_count(),
            MAX_OBJECT_COUNT
        ));
    }

    for (_, obj) in doc.objects_iter() {
        validate_geo_object(obj)?;
    }

    Ok(())
}

fn validate_geo_object(obj: &GeoObject) -> Result<(), String> {
    match obj {
        GeoObject::Function(o) => validate_expr(&o.expr)?,
        GeoObject::Text(o) => {
            validate_expr(&o.content)?;
            if o.content.len() > MAX_STRING_LENGTH {
                return Err("Text object content is too long".to_string());
            }
        }
        GeoObject::ParametricCurve2D(o) => {
            validate_expr(&o.expr_x)?;
            validate_expr(&o.expr_y)?;
        }
        GeoObject::ParametricCurve3D(o) => {
            validate_expr(&o.expr_x)?;
            validate_expr(&o.expr_y)?;
            validate_expr(&o.expr_z)?;
        }
        GeoObject::PolarCurve(o) => validate_expr(&o.expr_r)?,
        GeoObject::Surface3D(o) => {
            validate_expr(&o.expr)?;
            if o.mesh_res > MAX_SURFACE_MESH_RES {
                return Err(format!(
                    "Surface3D mesh_res {} exceeds maximum {}",
                    o.mesh_res, MAX_SURFACE_MESH_RES
                ));
            }
        }
        GeoObject::VectorField2D(o) => {
            validate_expr(&o.expr_u)?;
            validate_expr(&o.expr_v)?;
            if o.density > MAX_DENSITY {
                return Err(format!(
                    "VectorField2D density {} exceeds maximum {}",
                    o.density, MAX_DENSITY
                ));
            }
        }
        GeoObject::VectorField3D(o) => {
            validate_expr(&o.expr_u)?;
            validate_expr(&o.expr_v)?;
            validate_expr(&o.expr_w)?;
            if o.density > MAX_DENSITY {
                return Err(format!(
                    "VectorField3D density {} exceeds maximum {}",
                    o.density, MAX_DENSITY
                ));
            }
        }
        GeoObject::ComplexGrid(o) => {
            validate_expr(&o.expr)?;
            if o.density > MAX_DENSITY {
                return Err(format!(
                    "ComplexGrid density {} exceeds maximum {}",
                    o.density, MAX_DENSITY
                ));
            }
        }
        GeoObject::ComplexMapping(o) => validate_expr(&o.expr)?,
        GeoObject::ImplicitCurve(o) => {
            validate_expr(&o.expr_lhs)?;
            validate_expr(&o.expr_rhs)?;
        }
        GeoObject::Attractor3D(o) if o.steps > MAX_ATTRACTOR_STEPS => {
            return Err(format!(
                "Attractor3D steps {} exceeds maximum {}",
                o.steps, MAX_ATTRACTOR_STEPS
            ));
        }
        GeoObject::Fractal2D(o) if o.resolution > MAX_FRACTAL_RESOLUTION => {
            return Err(format!(
                "Fractal2D resolution {} exceeds maximum {}",
                o.resolution, MAX_FRACTAL_RESOLUTION
            ));
        }
        GeoObject::Fractal2D(o) if o.max_iter > MAX_FRACTAL_ITER => {
            return Err(format!(
                "Fractal2D max_iter {} exceeds maximum {}",
                o.max_iter, MAX_FRACTAL_ITER
            ));
        }
        GeoObject::HyperSurface4D(o) if o.resolution > MAX_HYPERSURFACE_RES => {
            return Err(format!(
                "HyperSurface4D resolution {} exceeds maximum {}",
                o.resolution, MAX_HYPERSURFACE_RES
            ));
        }
        GeoObject::PhasePortrait(o) => {
            validate_expr(&o.expr_dx)?;
            validate_expr(&o.expr_dy)?;
            if o.density > MAX_DENSITY {
                return Err(format!(
                    "PhasePortrait density {} exceeds maximum {}",
                    o.density, MAX_DENSITY
                ));
            }
        }
        GeoObject::Histogram(o) if o.data.len() > MAX_ARRAY_LENGTH => {
            return Err(format!(
                "Histogram data length {} exceeds maximum {}",
                o.data.len(),
                MAX_ARRAY_LENGTH
            ));
        }
        GeoObject::ScatterPlot(o)
            if o.xs.len() > MAX_ARRAY_LENGTH || o.ys.len() > MAX_ARRAY_LENGTH =>
        {
            return Err("ScatterPlot data length exceeds maximum".to_string());
        }
        GeoObject::BoxPlot(o) if o.data.len() > MAX_ARRAY_LENGTH => {
            return Err(format!(
                "BoxPlot data length {} exceeds maximum {}",
                o.data.len(),
                MAX_ARRAY_LENGTH
            ));
        }
        GeoObject::RegressionLine(o)
            if o.xs.len() > MAX_ARRAY_LENGTH || o.ys.len() > MAX_ARRAY_LENGTH =>
        {
            return Err("RegressionLine data length exceeds maximum".to_string());
        }
        // Catch-all: validate the label length for every remaining object
        // type (Point, Line, Circle, 3D primitives, Pencil, etc.) to
        // mitigate huge label strings in untrusted save files.
        _ => {
            let label = obj.label();
            if label.len() > MAX_STRING_LENGTH {
                return Err(format!(
                    "Object label length {} exceeds maximum {}",
                    label.len(),
                    MAX_STRING_LENGTH
                ));
            }
        }
    }
    Ok(())
}

fn validate_expr(expr: &str) -> Result<(), String> {
    if expr.len() > MAX_EXPR_LENGTH {
        return Err(format!(
            "Expression length {} exceeds maximum {}",
            expr.len(),
            MAX_EXPR_LENGTH
        ));
    }
    Ok(())
}
