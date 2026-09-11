#![allow(unknown_lints, float_literal_f32_fallback)]
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]
#![allow(deprecated)]
//! Procesador de comandos compartido de Grafito.
//!
//! Este crate es el único punto de entrada en Rust para los comandos de texto
//! usados por los frontends de escritorio y FFI. Aquí viven el parseo de
//! comandos, la construcción geométrica, el despacho del CAS, la estadística
//! y los comandos de análisis matemático (`Root`, `Extremum`, `Inflection`,
//! `YIntercept`, `Analyze`).
//!
//! # Ejemplo mínimo
//!
//! ```
//! use grafito_command::process_input;
//! use grafito_core::Document;
//!
//! let mut doc = Document::new();
//! let mut input = "A = (1, 2)".to_string();
//! process_input(&mut doc, &mut input);
//!
//! assert!(doc.objects_iter().any(|(_, obj)| obj.label() == "A"));
//! ```

pub mod assistant_bridge;
pub mod assistant_context;
pub mod assistant_plan;
pub mod assistant_proposals;
pub mod cas_analysis;
pub mod cas_parse;
pub mod command_registry;
pub mod commands;
pub mod ggbscript;
pub mod helpers;
pub mod inspector_equation;

pub use commands::{parse_point_str, parse_preview, process_cas_worksheet_cell, process_input};

#[cfg(test)]
mod tests {
    use super::process_input;
    use crate::commands::CommandOutcome;
    use grafito_core::{Document, GeoObject, LineObj, PointObj};
    use grafito_geometry::Point2;

    #[test]
    fn creates_basic_function_from_expression() {
        let mut doc = Document::new();
        let mut input = "sin(x)".to_string();

        let result = process_input(&mut doc, &mut input);

        assert!(matches!(result, CommandOutcome::Ok));
        assert!(input.is_empty());
        assert!(doc
            .objects_iter()
            .any(|(_, obj)| matches!(obj, GeoObject::Function(_))));
    }

    #[test]
    fn creates_graphical_objects_for_solve() {
        let mut doc = Document::new();
        let mut input = "Solve[x^2-4, x, -3, 3]".to_string();

        let result = match process_input(&mut doc, &mut input) {
            CommandOutcome::Message(msg) => msg,
            other => panic!("solve should return a message, got {:?}", other),
        };

        assert!(result.contains("Graficado"));
        assert!(doc
            .objects_iter()
            .any(|(_, obj)| matches!(obj, GeoObject::Function(_))));
        assert!(doc
            .objects_iter()
            .any(|(_, obj)| matches!(obj, GeoObject::Point(_))));
    }

    #[test]
    fn creates_statistics_plot() {
        let mut doc = Document::new();
        let mut input = "Histogram[{1,2,2,3,4}, 3]".to_string();

        let result = match process_input(&mut doc, &mut input) {
            CommandOutcome::Message(msg) => msg,
            other => panic!("histogram should return a message, got {:?}", other),
        };

        assert!(result.contains("Histogram"));
        assert!(doc
            .objects_iter()
            .any(|(_, obj)| matches!(obj, GeoObject::Histogram(_))));
    }

    #[test]
    fn creates_phase_portrait() {
        let mut doc = Document::new();
        let mut input = "PhasePortrait[x+y, x-y]".to_string();

        let result = process_input(&mut doc, &mut input);

        assert!(matches!(result, CommandOutcome::Message(_)));
        assert!(doc
            .objects_iter()
            .any(|(_, obj)| matches!(obj, GeoObject::PhasePortrait(_))));
    }

    #[test]
    fn setvalue_triggers_propagation() {
        let mut doc = Document::new();
        // Create two free points
        let a = doc.add_object(GeoObject::Point(
            grafito_core::PointObj::new(grafito_geometry::Point2::new(1.0, 2.0)).with_label("A"),
        ));
        let b = doc.add_object(GeoObject::Point(
            grafito_core::PointObj::new(grafito_geometry::Point2::new(5.0, 6.0)).with_label("B"),
        ));
        // Create midpoint via constructed object
        let (m, _) = doc.add_constructed_object(
            GeoObject::Point(
                grafito_core::PointObj::new(grafito_geometry::Point2::new(3.0, 4.0))
                    .with_label("M"),
            ),
            "Midpoint",
            &[a, b],
        );
        // Move A via SetValue
        let mut input = "SetValue[A, (3, 0)]".to_string();
        process_input(&mut doc, &mut input);
        // The midpoint should still exist and have been updated
        assert!(
            doc.get_object(m).is_some(),
            "Midpoint should still exist after SetValue"
        );
    }

    // ── Frente W-B: comandos label-aware crean construcción viva ─────────
    #[test]
    fn line_segment_circle_measure_with_labels_create_live_constructions() {
        let mut doc = Document::new();
        process_input(&mut doc, &mut "A = (0, 0)".to_string());
        process_input(&mut doc, &mut "B = (4, 0)".to_string());

        for command in [
            "Line[A, B]",
            "Segment[A, B]",
            "Circle[A, B]",
            "MeasureDistance[A, B]",
        ] {
            let outcome = process_input(&mut doc, &mut command.to_string());
            assert!(
                matches!(outcome, CommandOutcome::Ok),
                "{command} should succeed, got {outcome:?}"
            );
        }

        // Toda la familia declara padres en el grafo de dependencias.
        let mut live = 0;
        for (_, object) in doc.objects_iter() {
            if doc.creator_of(&object.id()).is_some() {
                live += 1;
            }
        }
        assert_eq!(
            live, 4,
            "Line+Segment+Circle+MeasureDistance deben ser vivos"
        );

        // La medida nace con el valor actual.
        let measure = doc
            .objects_iter()
            .find(|(_, object)| matches!(object, GeoObject::Text(_)))
            .expect("MeasureDistance creates a text");
        if let GeoObject::Text(text) = measure.1 {
            assert_eq!(text.content, "4.000");
        }

        // Arrastrar A mueve recta, círculo y medida (camino real del gesto).
        let a = doc.try_find_object_by_label("A").unwrap().unwrap();
        assert!(doc
            .try_move_point_and_re_evaluate(a, Point2::new(0.0, 3.0))
            .unwrap());
        let measure = doc
            .objects_iter()
            .find(|(_, object)| matches!(object, GeoObject::Text(_)))
            .expect("measure survives the drag");
        if let GeoObject::Text(text) = measure.1 {
            assert_eq!(text.content, format!("{:.3}", 25.0f64.sqrt()));
            assert!((text.position.x - 2.0).abs() < 1e-9);
            assert!((text.position.y - 1.5).abs() < 1e-9);
        } else {
            panic!("expected measure text");
        }
        let live_line = doc
            .objects_iter()
            .find(|(_, object)| {
                matches!(object, GeoObject::Line(_))
                    && doc
                        .creator_of(&object.id())
                        .is_some_and(|cons| cons.name == "LineByTwoPoints")
            })
            .expect("live line survives the drag");
        if let GeoObject::Line(line) = live_line.1 {
            assert_eq!(line.start, Point2::new(0.0, 3.0));
            assert_eq!(line.end, Point2::new(4.0, 0.0));
        }
    }

    #[test]
    fn line_with_literals_stays_free_and_honest() {
        let mut doc = Document::new();
        let outcome = process_input(&mut doc, &mut "Line[(0, 0), (1, 1)]".to_string());
        assert!(matches!(outcome, CommandOutcome::Ok));
        let (_, object) = doc
            .objects_iter()
            .find(|(_, object)| matches!(object, GeoObject::Line(_)))
            .expect("literal line is created");
        assert!(
            doc.creator_of(&object.id()).is_none(),
            "sin etiquetas no hay padres: libre documentado, no silencio"
        );
    }

    #[test]
    fn circle_center_radius_follows_center_with_frozen_radius() {
        let mut doc = Document::new();
        process_input(&mut doc, &mut "A = (1, 1)".to_string());
        let outcome = process_input(&mut doc, &mut "Circle[A, 2]".to_string());
        assert!(matches!(outcome, CommandOutcome::Ok));
        let a = doc.try_find_object_by_label("A").unwrap().unwrap();
        assert!(doc
            .try_move_point_and_re_evaluate(a, Point2::new(5.0, 1.0))
            .unwrap());
        let (_, object) = doc
            .objects_iter()
            .find(|(_, object)| matches!(object, GeoObject::Circle(_)))
            .expect("circle survives the drag");
        if let GeoObject::Circle(circle) = object {
            assert_eq!(circle.center, Point2::new(5.0, 1.0));
            assert!((circle.radius - 2.0).abs() < 1e-12);
        } else {
            panic!("expected circle");
        }
    }

    #[test]
    fn midpoint_with_labels() {
        let mut doc = Document::new();
        let mut input = "A = (1, 2)".to_string();
        process_input(&mut doc, &mut input);
        let mut input = "B = (5, 6)".to_string();
        process_input(&mut doc, &mut input);
        let mut input = "Midpoint[A, B]".to_string();
        process_input(&mut doc, &mut input);
        let midpoint = doc
            .objects_iter()
            .find(|(_, obj)| matches!(obj, GeoObject::Point(p) if p.label.starts_with('M')));
        assert!(midpoint.is_some(), "Midpoint[A, B] should create midpoint");
        // Verify constraint was registered
        assert!(
            doc.constraints.constraint_count() >= 1,
            "Should have at least one constraint"
        );
    }

    #[test]
    fn test_parse_distance_command() {
        let mut doc = Document::new();
        let a = doc.add_object(GeoObject::Point(
            PointObj::new(Point2::new(0.0, 0.0)).with_label("A"),
        ));
        let b = doc.add_object(GeoObject::Point(
            PointObj::new(Point2::new(3.0, 4.0)).with_label("B"),
        ));
        let mut input = "Distance[A, B, 5]".to_string();
        process_input(&mut doc, &mut input);

        assert_eq!(doc.constraints.constraint_count(), 1);
        let cons = doc.constraints.get_constraint(0).expect("constraint 0");
        assert_eq!(cons.name, "Distance");
        assert_eq!(cons.inputs, vec![a, b]);
        assert_eq!(cons.params.get("distance"), Some(&5.0));
    }

    #[test]
    fn test_parse_angle_command() {
        let mut doc = Document::new();
        let l1 = doc.add_object(GeoObject::Line(
            LineObj::new(Point2::new(0.0, 0.0), Point2::new(1.0, 0.0)).with_label("l1"),
        ));
        let l2 = doc.add_object(GeoObject::Line(
            LineObj::new(Point2::new(0.0, 0.0), Point2::new(0.0, 1.0)).with_label("l2"),
        ));
        let mut input = "Angle[l1, l2, 90]".to_string();
        process_input(&mut doc, &mut input);

        assert_eq!(doc.constraints.constraint_count(), 1);
        let cons = doc.constraints.get_constraint(0).expect("constraint 0");
        assert_eq!(cons.name, "Angle");
        assert_eq!(cons.inputs, vec![l1, l2]);
        assert_eq!(cons.params.get("angle"), Some(&90.0));
    }

    #[test]
    fn test_parse_boolean_command() {
        let mut doc = Document::new();
        process_input(&mut doc, &mut "RegularPolygon[(0,0), 4, 1]".to_string());
        process_input(&mut doc, &mut "RegularPolygon[(0.5,0), 4, 1]".to_string());

        let polygon_labels: Vec<String> = doc
            .objects_iter()
            .filter(|(_, obj)| matches!(obj, GeoObject::Polygon(_)))
            .map(|(_, obj)| obj.label().to_string())
            .collect();
        assert_eq!(polygon_labels.len(), 2);

        let mut cmd = format!("PolygonUnion[{}, {}]", polygon_labels[0], polygon_labels[1]);
        process_input(&mut doc, &mut cmd);

        assert!(
            doc.objects_iter().any(|(_, obj)| obj.label() == "U"),
            "union result polygon labeled 'U' should exist"
        );
    }

    #[test]
    fn test_parse_conic_command() {
        let mut doc = Document::new();
        let labels = ["A", "B", "C", "D", "E"];
        for (i, label) in labels.iter().enumerate() {
            let angle = i as f64 / 5.0 * std::f64::consts::TAU;
            doc.add_object(GeoObject::Point(
                PointObj::new(Point2::new(angle.cos(), angle.sin())).with_label(*label),
            ));
        }
        let mut input = "ConicByFivePoints[A, B, C, D, E]".to_string();
        let outcome = process_input(&mut doc, &mut input);

        assert_eq!(
            doc.constraints.constraint_count(),
            1,
            "unexpected command outcome: {outcome:?}"
        );
        let cons = doc
            .constraints
            .get_constraint(0)
            .expect("conic constraint should exist");
        assert_eq!(cons.name, "ConicByFivePoints");
        assert_eq!(cons.inputs.len(), 5);
        assert!(
            !cons.outputs.is_empty(),
            "conic constraint should produce an output object"
        );
    }
}
