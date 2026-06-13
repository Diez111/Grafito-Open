//! Conversión de GeoObject → ObjectDto para las 34 variantes

use crate::dto::{ColorDto, ObjectDto, PropertyDto};
use grafito_core::{GeoObject, ObjectId};
use grafito_geometry::Color;

/// Convierte Color de grafito-geometry a ColorDto
pub fn color_to_dto(c: Color) -> ColorDto {
    ColorDto {
        r: c.r,
        g: c.g,
        b: c.b,
        a: c.a,
    }
}

/// Convierte ObjectId a String (UUID corto)
pub fn id_to_string(id: ObjectId) -> String {
    id.0.to_string()[..8].to_string()
}

/// Convierte GeoObject a ObjectDto con todas sus propiedades
pub fn geo_object_to_dto(obj: &GeoObject) -> ObjectDto {
    let id = id_to_string(obj.id());
    let label = obj.label().to_string();
    let object_type = obj.name().to_string();
    let visible = obj.is_visible();
    let color = color_to_dto(obj.color());

    let (properties, summary) = match obj {
        // 2D básicos
        GeoObject::Point(p) => {
            let props = vec![
                PropertyDto {
                    name: "x".into(),
                    value: format!("{:.3}", p.position.x),
                    editable: true,
                },
                PropertyDto {
                    name: "y".into(),
                    value: format!("{:.3}", p.position.y),
                    editable: true,
                },
            ];
            let summary = format!("({:.2}, {:.2})", p.position.x, p.position.y);
            (props, summary)
        }
        GeoObject::Line(l) => {
            let length = l.length();
            let props = vec![
                PropertyDto {
                    name: "start".into(),
                    value: format!("({:.2}, {:.2})", l.start.x, l.start.y),
                    editable: false,
                },
                PropertyDto {
                    name: "end".into(),
                    value: format!("({:.2}, {:.2})", l.end.x, l.end.y),
                    editable: false,
                },
                PropertyDto {
                    name: "length".into(),
                    value: format!("{:.3}", length),
                    editable: false,
                },
            ];
            let summary = format!("length={:.2}", length);
            (props, summary)
        }
        GeoObject::Circle(c) => {
            let area = std::f64::consts::PI * c.radius * c.radius;
            let circumference = 2.0 * std::f64::consts::PI * c.radius;
            let props = vec![
                PropertyDto {
                    name: "center".into(),
                    value: format!("({:.2}, {:.2})", c.center.x, c.center.y),
                    editable: false,
                },
                PropertyDto {
                    name: "radius".into(),
                    value: format!("{:.3}", c.radius),
                    editable: true,
                },
                PropertyDto {
                    name: "area".into(),
                    value: format!("{:.3}", area),
                    editable: false,
                },
                PropertyDto {
                    name: "circumference".into(),
                    value: format!("{:.3}", circumference),
                    editable: false,
                },
            ];
            let summary = format!("r={:.2}, area={:.2}", c.radius, area);
            (props, summary)
        }
        GeoObject::Polygon(poly) => {
            let perimeter = poly
                .vertices
                .windows(2)
                .map(|w| w[0].distance(&w[1]))
                .chain(std::iter::once(
                    poly.vertices
                        .last()
                        .map_or(0.0, |v| v.distance(&poly.vertices[0])),
                ))
                .sum::<f64>();
            let area = polygon_area(&poly.vertices);
            let props = vec![
                PropertyDto {
                    name: "vertices".into(),
                    value: format!("{}", poly.vertices.len()),
                    editable: false,
                },
                PropertyDto {
                    name: "perimeter".into(),
                    value: format!("{:.3}", perimeter),
                    editable: false,
                },
                PropertyDto {
                    name: "area".into(),
                    value: format!("{:.3}", area),
                    editable: false,
                },
            ];
            let summary = format!("{} vertices, area={:.2}", poly.vertices.len(), area);
            (props, summary)
        }
        GeoObject::Function(f) => {
            let props = vec![
                PropertyDto {
                    name: "expression".into(),
                    value: f.expr.clone(),
                    editable: true,
                },
                PropertyDto {
                    name: "domain".into(),
                    value: format!(
                        "[{}, {}]",
                        f.domain_min
                            .map(|v| format!("{:.1}", v))
                            .unwrap_or("-∞".into()),
                        f.domain_max
                            .map(|v| format!("{:.1}", v))
                            .unwrap_or("∞".into())
                    ),
                    editable: true,
                },
            ];
            let summary = format!("f(x) = {}", f.expr);
            (props, summary)
        }
        GeoObject::Ellipse(e) => {
            let area = std::f64::consts::PI * e.rx * e.ry;
            let props = vec![
                PropertyDto {
                    name: "center".into(),
                    value: format!("({:.2}, {:.2})", e.center.x, e.center.y),
                    editable: false,
                },
                PropertyDto {
                    name: "rx".into(),
                    value: format!("{:.3}", e.rx),
                    editable: true,
                },
                PropertyDto {
                    name: "ry".into(),
                    value: format!("{:.3}", e.ry),
                    editable: true,
                },
                PropertyDto {
                    name: "area".into(),
                    value: format!("{:.3}", area),
                    editable: false,
                },
            ];
            let summary = format!("rx={:.2}, ry={:.2}", e.rx, e.ry);
            (props, summary)
        }
        GeoObject::Parabola(p) => {
            let props = vec![
                PropertyDto {
                    name: "vertex".into(),
                    value: format!("({:.2}, {:.2})", p.vertex.x, p.vertex.y),
                    editable: false,
                },
                PropertyDto {
                    name: "p".into(),
                    value: format!("{:.3}", p.p),
                    editable: true,
                },
                PropertyDto {
                    name: "vertical".into(),
                    value: p.vertical.to_string(),
                    editable: true,
                },
            ];
            let summary = format!("p={:.2}", p.p);
            (props, summary)
        }
        GeoObject::Hyperbola(h) => {
            let props = vec![
                PropertyDto {
                    name: "center".into(),
                    value: format!("({:.2}, {:.2})", h.center.x, h.center.y),
                    editable: false,
                },
                PropertyDto {
                    name: "a".into(),
                    value: format!("{:.3}", h.a),
                    editable: true,
                },
                PropertyDto {
                    name: "b".into(),
                    value: format!("{:.3}", h.b),
                    editable: true,
                },
            ];
            let summary = format!("a={:.2}, b={:.2}", h.a, h.b);
            (props, summary)
        }

        // 3D básicos
        GeoObject::Point3D(p) => {
            let props = vec![
                PropertyDto {
                    name: "x".into(),
                    value: format!("{:.3}", p.position.x),
                    editable: true,
                },
                PropertyDto {
                    name: "y".into(),
                    value: format!("{:.3}", p.position.y),
                    editable: true,
                },
                PropertyDto {
                    name: "z".into(),
                    value: format!("{:.3}", p.position.z),
                    editable: true,
                },
            ];
            let summary = format!(
                "({:.2}, {:.2}, {:.2})",
                p.position.x, p.position.y, p.position.z
            );
            (props, summary)
        }
        GeoObject::Sphere3D(s) => {
            let volume = (4.0 / 3.0) * std::f64::consts::PI * s.radius.powi(3);
            let surface_area = 4.0 * std::f64::consts::PI * s.radius.powi(2);
            let props = vec![
                PropertyDto {
                    name: "center".into(),
                    value: format!("({:.2}, {:.2}, {:.2})", s.center.x, s.center.y, s.center.z),
                    editable: false,
                },
                PropertyDto {
                    name: "radius".into(),
                    value: format!("{:.3}", s.radius),
                    editable: true,
                },
                PropertyDto {
                    name: "volume".into(),
                    value: format!("{:.3}", volume),
                    editable: false,
                },
                PropertyDto {
                    name: "surface".into(),
                    value: format!("{:.3}", surface_area),
                    editable: false,
                },
            ];
            let summary = format!("r={:.2}, V={:.2}", s.radius, volume);
            (props, summary)
        }
        GeoObject::Cube3D(c) => {
            let volume = c.size.powi(3);
            let surface_area = 6.0 * c.size.powi(2);
            let props = vec![
                PropertyDto {
                    name: "size".into(),
                    value: format!("{:.3}", c.size),
                    editable: true,
                },
                PropertyDto {
                    name: "volume".into(),
                    value: format!("{:.3}", volume),
                    editable: false,
                },
                PropertyDto {
                    name: "surface".into(),
                    value: format!("{:.3}", surface_area),
                    editable: false,
                },
            ];
            let summary = format!("size={:.2}, V={:.2}", c.size, volume);
            (props, summary)
        }

        GeoObject::Text(txt) => {
            let props = vec![
                PropertyDto {
                    name: "content".into(),
                    value: txt.content.clone(),
                    editable: true,
                },
                PropertyDto {
                    name: "position".into(),
                    value: format!("({:.2}, {:.2})", txt.position.x, txt.position.y),
                    editable: true,
                },
                PropertyDto {
                    name: "font_size".into(),
                    value: format!("{:.1}", txt.font_size),
                    editable: true,
                },
            ];
            let summary = format!("\"{}\"", txt.content.chars().take(20).collect::<String>());
            (props, summary)
        }
        GeoObject::Segment3D(s) => {
            let dist = s.a.distance(&s.b);
            let props = vec![
                PropertyDto {
                    name: "start".into(),
                    value: format!("({:.2}, {:.2}, {:.2})", s.a.x, s.a.y, s.a.z),
                    editable: false,
                },
                PropertyDto {
                    name: "end".into(),
                    value: format!("({:.2}, {:.2}, {:.2})", s.b.x, s.b.y, s.b.z),
                    editable: false,
                },
                PropertyDto {
                    name: "length".into(),
                    value: format!("{:.3}", dist),
                    editable: false,
                },
            ];
            let summary = format!("length={:.2}", dist);
            (props, summary)
        }
        GeoObject::Pyramid3D(p) => {
            let props = vec![
                PropertyDto {
                    name: "base_center".into(),
                    value: format!(
                        "({:.2}, {:.2}, {:.2})",
                        p.base_center.x, p.base_center.y, p.base_center.z
                    ),
                    editable: false,
                },
                PropertyDto {
                    name: "base_size".into(),
                    value: format!("{:.3}", p.base_size),
                    editable: true,
                },
            ];
            let summary = format!("base={:.2}", p.base_size);
            (props, summary)
        }
        GeoObject::Cone3D(c) => {
            let props = vec![PropertyDto {
                name: "radius".into(),
                value: format!("{:.3}", c.radius),
                editable: true,
            }];
            let summary = format!("r={:.2}", c.radius);
            (props, summary)
        }
        GeoObject::Cylinder3D(c) => {
            let props = vec![PropertyDto {
                name: "radius".into(),
                value: format!("{:.3}", c.radius),
                editable: true,
            }];
            let summary = format!("r={:.2}", c.radius);
            (props, summary)
        }
        GeoObject::Torus3D(t) => {
            let props = vec![
                PropertyDto {
                    name: "r_major".into(),
                    value: format!("{:.3}", t.r_major),
                    editable: true,
                },
                PropertyDto {
                    name: "r_minor".into(),
                    value: format!("{:.3}", t.r_minor),
                    editable: true,
                },
            ];
            let summary = format!("R={:.2}/r={:.2}", t.r_major, t.r_minor);
            (props, summary)
        }
        GeoObject::MoebiusStrip(m) => {
            let props = vec![
                PropertyDto {
                    name: "radius".into(),
                    value: format!("{:.3}", m.radius),
                    editable: true,
                },
                PropertyDto {
                    name: "width".into(),
                    value: format!("{:.3}", m.width_r),
                    editable: true,
                },
            ];
            let summary = format!("r={:.2}", m.radius);
            (props, summary)
        }
        GeoObject::Surface3D(s) => {
            let props = vec![
                PropertyDto {
                    name: "expr".into(),
                    value: s.expr.clone(),
                    editable: true,
                },
                PropertyDto {
                    name: "mesh".into(),
                    value: format!("{}×{}", s.mesh_res, s.mesh_res),
                    editable: false,
                },
            ];
            let summary = format!("f(x,y) = {}", s.expr);
            (props, summary)
        }
        GeoObject::ParametricCurve2D(pc) => {
            let props = vec![
                PropertyDto {
                    name: "x(t)".into(),
                    value: pc.expr_x.clone(),
                    editable: true,
                },
                PropertyDto {
                    name: "y(t)".into(),
                    value: pc.expr_y.clone(),
                    editable: true,
                },
                PropertyDto {
                    name: "t_range".into(),
                    value: format!("[{:.1}, {:.1}]", pc.t_min, pc.t_max),
                    editable: true,
                },
            ];
            let summary = format!("(x(t), y(t)) t∈[{:.1},{:.1}]", pc.t_min, pc.t_max);
            (props, summary)
        }
        GeoObject::ParametricCurve3D(pc) => {
            let props = vec![
                PropertyDto {
                    name: "x(t)".into(),
                    value: pc.expr_x.clone(),
                    editable: true,
                },
                PropertyDto {
                    name: "y(t)".into(),
                    value: pc.expr_y.clone(),
                    editable: true,
                },
                PropertyDto {
                    name: "z(t)".into(),
                    value: pc.expr_z.clone(),
                    editable: true,
                },
            ];
            let summary = format!("(x,y,z)(t) t∈[{:.1},{:.1}]", pc.t_min, pc.t_max);
            (props, summary)
        }
        GeoObject::PolarCurve(pol) => {
            let props = vec![
                PropertyDto {
                    name: "r(θ)".into(),
                    value: pol.expr_r.clone(),
                    editable: true,
                },
                PropertyDto {
                    name: "θ_range".into(),
                    value: format!("[{:.1}, {:.1}]", pol.t_min, pol.t_max),
                    editable: true,
                },
            ];
            let summary = format!("r(θ) = {}", pol.expr_r);
            (props, summary)
        }
        GeoObject::ImplicitCurve(ic) => {
            let op = match ic.operator {
                grafito_core::RelationOperator::Eq => "=",
                grafito_core::RelationOperator::Less => "<",
                grafito_core::RelationOperator::Greater => ">",
                grafito_core::RelationOperator::LessEq => "≤",
                grafito_core::RelationOperator::GreaterEq => "≥",
            };
            let props = vec![
                PropertyDto {
                    name: "lhs".into(),
                    value: ic.expr_lhs.clone(),
                    editable: false,
                },
                PropertyDto {
                    name: "op".into(),
                    value: op.to_string(),
                    editable: false,
                },
                PropertyDto {
                    name: "rhs".into(),
                    value: ic.expr_rhs.clone(),
                    editable: false,
                },
            ];
            let summary = format!("{} {} {}", ic.expr_lhs, op, ic.expr_rhs);
            (props, summary)
        }
        GeoObject::VectorField2D(vf) => {
            let props = vec![
                PropertyDto {
                    name: "u".into(),
                    value: vf.expr_u.clone(),
                    editable: true,
                },
                PropertyDto {
                    name: "v".into(),
                    value: vf.expr_v.clone(),
                    editable: true,
                },
                PropertyDto {
                    name: "density".into(),
                    value: vf.density.to_string(),
                    editable: true,
                },
            ];
            let summary = format!("F = ({}, {})", vf.expr_u, vf.expr_v);
            (props, summary)
        }
        GeoObject::VectorField3D(vf) => {
            let props = vec![
                PropertyDto {
                    name: "u".into(),
                    value: vf.expr_u.clone(),
                    editable: true,
                },
                PropertyDto {
                    name: "v".into(),
                    value: vf.expr_v.clone(),
                    editable: true,
                },
                PropertyDto {
                    name: "w".into(),
                    value: vf.expr_w.clone(),
                    editable: true,
                },
            ];
            let summary = format!("F = ({}, {}, {})", vf.expr_u, vf.expr_v, vf.expr_w);
            (props, summary)
        }
        GeoObject::ComplexGrid(cg) => {
            let mode = match cg.render_mode {
                1 => "Domain coloring",
                2 => "Heat map",
                _ => "Grid lines",
            };
            let props = vec![
                PropertyDto {
                    name: "expr".into(),
                    value: cg.expr.clone(),
                    editable: true,
                },
                PropertyDto {
                    name: "mode".into(),
                    value: mode.to_string(),
                    editable: false,
                },
                PropertyDto {
                    name: "density".into(),
                    value: cg.density.to_string(),
                    editable: true,
                },
            ];
            let summary = format!("f(z) = {}", cg.expr);
            (props, summary)
        }
        GeoObject::ComplexMapping(cm) => {
            let props = vec![PropertyDto {
                name: "expr".into(),
                value: cm.expr.clone(),
                editable: true,
            }];
            let summary = format!("f(z) = {}", cm.expr);
            (props, summary)
        }
        GeoObject::Attractor3D(at) => {
            let props = vec![
                PropertyDto {
                    name: "type".into(),
                    value: at.attractor_type.clone(),
                    editable: false,
                },
                PropertyDto {
                    name: "steps".into(),
                    value: at.steps.to_string(),
                    editable: true,
                },
            ];
            let summary = format!("{} attractor", at.attractor_type);
            (props, summary)
        }
        GeoObject::Fractal2D(fr) => {
            let props = vec![
                PropertyDto {
                    name: "type".into(),
                    value: fr.fractal_type.clone(),
                    editable: false,
                },
                PropertyDto {
                    name: "resolution".into(),
                    value: fr.resolution.to_string(),
                    editable: true,
                },
                PropertyDto {
                    name: "max_iter".into(),
                    value: fr.max_iter.to_string(),
                    editable: true,
                },
            ];
            let summary = format!("{} fractal", fr.fractal_type);
            (props, summary)
        }
        GeoObject::HyperSurface4D(hs) => {
            let props = vec![
                PropertyDto {
                    name: "type".into(),
                    value: hs.surface_type.clone(),
                    editable: false,
                },
                PropertyDto {
                    name: "resolution".into(),
                    value: hs.resolution.to_string(),
                    editable: true,
                },
            ];
            let summary = format!("{} hypersurface", hs.surface_type);
            (props, summary)
        }
        GeoObject::Histogram(h) => {
            let props = vec![
                PropertyDto {
                    name: "data_points".into(),
                    value: h.data.len().to_string(),
                    editable: false,
                },
                PropertyDto {
                    name: "bins".into(),
                    value: h.bins.to_string(),
                    editable: true,
                },
            ];
            let summary = format!("{} bins, {} points", h.bins, h.data.len());
            (props, summary)
        }
        GeoObject::ScatterPlot(sp) => {
            let props = vec![PropertyDto {
                name: "points".into(),
                value: sp.xs.len().to_string(),
                editable: false,
            }];
            let summary = format!("{} points", sp.xs.len());
            (props, summary)
        }
        GeoObject::BoxPlot(bp) => {
            let props = vec![PropertyDto {
                name: "data_points".into(),
                value: bp.data.len().to_string(),
                editable: false,
            }];
            let summary = format!("{} points", bp.data.len());
            (props, summary)
        }
        GeoObject::RegressionLine(rl) => {
            let props = vec![
                PropertyDto {
                    name: "slope".into(),
                    value: format!("{:.4}", rl.slope),
                    editable: false,
                },
                PropertyDto {
                    name: "intercept".into(),
                    value: format!("{:.4}", rl.intercept),
                    editable: false,
                },
                PropertyDto {
                    name: "r²".into(),
                    value: format!("{:.4}", rl.r_squared),
                    editable: false,
                },
            ];
            let summary = format!(
                "y={:.3}x+{:.3} r²={:.3}",
                rl.slope, rl.intercept, rl.r_squared
            );
            (props, summary)
        }
        GeoObject::PhasePortrait(pp) => {
            let props = vec![
                PropertyDto {
                    name: "dx/dt".into(),
                    value: pp.expr_dx.clone(),
                    editable: true,
                },
                PropertyDto {
                    name: "dy/dt".into(),
                    value: pp.expr_dy.clone(),
                    editable: true,
                },
                PropertyDto {
                    name: "density".into(),
                    value: pp.density.to_string(),
                    editable: true,
                },
            ];
            let summary = format!("dx/dt={}, dy/dt={}", pp.expr_dx, pp.expr_dy);
            (props, summary)
        }
    };

    ObjectDto {
        id,
        label,
        object_type,
        visible,
        color,
        properties,
        summary,
    }
}

/// Calcula el área de un polígono usando la fórmula del shoelace
fn polygon_area(vertices: &[grafito_geometry::Point2]) -> f64 {
    if vertices.len() < 3 {
        return 0.0;
    }
    let mut area = 0.0;
    let n = vertices.len();
    for i in 0..n {
        let j = (i + 1) % n;
        area += vertices[i].x * vertices[j].y;
        area -= vertices[j].x * vertices[i].y;
    }
    area.abs() / 2.0
}
