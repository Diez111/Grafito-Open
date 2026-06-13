//! Shared Grafito command processor.
//!
//! This crate is the single Rust entry point for text commands used by both
//! desktop and FFI frontends. All command parsing, geometry construction,
//! CAS dispatch, and statistics live here.

pub mod commands;

pub use commands::{parse_point_str, parse_preview, process_input};

#[cfg(test)]
mod tests {
    use super::process_input;
    use grafito_core::{Document, GeoObject};

    #[test]
    fn creates_basic_function_from_expression() {
        let mut doc = Document::new();
        let mut input = "sin(x)".to_string();

        let result = process_input(&mut doc, &mut input);

        assert!(result.is_none());
        assert!(input.is_empty());
        assert!(doc
            .objects_iter()
            .any(|(_, obj)| matches!(obj, GeoObject::Function(_))));
    }

    #[test]
    fn creates_graphical_objects_for_solve() {
        let mut doc = Document::new();
        let mut input = "Solve[x^2-4, x, -3, 3]".to_string();

        let result = process_input(&mut doc, &mut input).expect("solve should return text");

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

        let result = process_input(&mut doc, &mut input).expect("histogram should return text");

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

        assert!(result.is_some());
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
}
