#![allow(clippy::unwrap_used, clippy::expect_used)]
use grafito_command::commands::{process_input, CommandOutcome};
use grafito_core::{Document, GeoObject};

fn count_kind(doc: &Document, kind: &str) -> usize {
    doc.objects_iter().filter(|(_, o)| o.name() == kind).count()
}

#[test]
fn convex_hull_square_produces_quadrilateral() {
    let mut doc = Document::new();
    let mut inp = "ConvexHull[{(0,0),(1,0),(1,1),(0,1),(0.5,0.5)}]".to_string();
    let out = process_input(&mut doc, &mut inp);
    assert!(matches!(out, CommandOutcome::Message(_)), "got {:?}", out);
    let polys: Vec<_> = doc
        .objects_iter()
        .filter_map(|(_, o)| match o {
            GeoObject::Polygon(p) => Some(p.vertices.len()),
            _ => None,
        })
        .collect();
    assert!(
        polys.contains(&4),
        "esperaba casco de 4 vértices, obtuvo {polys:?}"
    );
}

#[test]
fn convex_hull_from_datatable() {
    let mut doc = Document::new();
    let mut dt = "DataTable[{0,1,2},{0,1,0}]".to_string();
    let out = process_input(&mut doc, &mut dt);
    assert!(
        matches!(out, CommandOutcome::Message(_)),
        "DataTable failed {out:?}"
    );
    let dt_label = doc
        .objects_iter()
        .find_map(|(_, o)| {
            if o.name() == "DataTable" {
                Some(o.label().to_string())
            } else {
                None
            }
        })
        .expect("DataTable label");
    let mut inp = format!("ConvexHull[{dt_label}]");
    let out2 = process_input(&mut doc, &mut inp);
    assert!(
        matches!(out2, CommandOutcome::Message(_)),
        "ConvexHull from table {out2:?}"
    );
    assert!(count_kind(&doc, "Polygon") >= 1);
}

#[test]
fn mst_creates_edges() {
    let mut doc = Document::new();
    let mut inp = "MinimumSpanningTree[{(0,0),(1,0),(0,1)}]".to_string();
    let out = process_input(&mut doc, &mut inp);
    assert!(
        matches!(out, CommandOutcome::Message(ref m) if m.contains("aristas")),
        "got {out:?}"
    );
    assert_eq!(count_kind(&doc, "Line"), 2);
}

#[test]
fn voronoi_stub_creates_cells() {
    let mut doc = Document::new();
    let mut inp = "Voronoi[{(0,0),(1,1)}]".to_string();
    let out = process_input(&mut doc, &mut inp);
    assert!(matches!(out, CommandOutcome::Message(_)), "got {out:?}");
    assert_eq!(count_kind(&doc, "Polygon"), 2);
}

#[test]
fn delaunay_fan_creates_triangles() {
    let mut doc = Document::new();
    let mut inp = "DelaunayTriangulation[{(0,0),(1,0),(0,1),(1,1)}]".to_string();
    let out = process_input(&mut doc, &mut inp);
    assert!(matches!(out, CommandOutcome::Message(_)), "got {out:?}");
    assert_eq!(count_kind(&doc, "Polygon"), 2);
}

#[test]
fn traveling_salesman_tour() {
    let mut doc = Document::new();
    let mut inp = "TravelingSalesman[{(0,0),(1,0),(1,1),(0,1)}]".to_string();
    let out = process_input(&mut doc, &mut inp);
    assert!(
        matches!(out, CommandOutcome::Message(ref m) if m.contains("tour")),
        "got {out:?}"
    );
    assert_eq!(count_kind(&doc, "Polygon"), 1);
}

#[test]
fn shortest_distance_point_to_circle() {
    let mut doc = Document::new();
    // punto A en origen
    let mut a = "A = (0,0)".to_string();
    let _ = process_input(&mut doc, &mut a);
    let mut c = "Circle[(1,0), 1]".to_string();
    let _ = process_input(&mut doc, &mut c);
    let circle_label = doc
        .objects_iter()
        .find_map(|(_, o)| {
            if o.name() == "Circle" {
                Some(o.label().to_string())
            } else {
                None
            }
        })
        .expect("circle label");
    let mut inp = format!("ShortestDistance[(0,0), {}]", circle_label);
    let out = process_input(&mut doc, &mut inp);
    assert!(
        matches!(out, CommandOutcome::Message(ref m) if m.contains("ShortestDistance")),
        "got {out:?}"
    );
    if let CommandOutcome::Message(m) = out {
        // distancia punto (0,0) a círculo centro (1,0) r=1 es 0
        assert!(
            m.contains("0.000000") || m.contains("0.0"),
            "distancia esperada 0, obtuvo {m}"
        );
    }
}

#[test]
fn shortest_distance_point_to_polygon() {
    let mut doc = Document::new();
    let mut poly = "Polygon[(0,0),(2,0),(2,2),(0,2)]".to_string();
    let _ = process_input(&mut doc, &mut poly);
    let label = doc
        .objects_iter()
        .find_map(|(_, o)| {
            if o.name() == "Polygon" {
                Some(o.label().to_string())
            } else {
                None
            }
        })
        .expect("polygon label");
    let mut inp = format!("ShortestDistance[(3,1), {}]", label);
    let out = process_input(&mut doc, &mut inp);
    assert!(matches!(out, CommandOutcome::Message(_)), "got {out:?}");
    if let CommandOutcome::Message(m) = out {
        assert!(m.contains("1.0"), "esperaba distancia 1, got {m}");
    }
}

#[test]
fn discrete_rejects_non_finite() {
    let mut doc = Document::new();
    // NaN no es finito
    let mut inp = "ConvexHull[{(0,0),(NaN,0)}]".to_string();
    let out = process_input(&mut doc, &mut inp);
    // Puede fallar en parse o en validación
    assert!(
        matches!(out, CommandOutcome::Error(_)),
        "debería fallar con NaN, got {out:?}"
    );
}
