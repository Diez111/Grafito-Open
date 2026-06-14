use criterion::{black_box, criterion_group, criterion_main, Criterion};
use grafito_core::{
    CircleObj, Document, FunctionObj, GeoObject, LineKind, LineObj, PointObj, PolygonObj,
};
use grafito_geometry::Point2;

/// Creates 50 free points arranged in a deterministic grid.
fn make_doc_with_50_points() -> (Document, Vec<grafito_core::ObjectId>) {
    let mut doc = Document::new();
    let mut points = Vec::with_capacity(50);
    for i in 0..50 {
        let x = (i % 10) as f64 * 2.0;
        let y = (i / 10) as f64 * 2.0;
        let id = doc.add_object(GeoObject::Point(PointObj::new(Point2::new(x, y))));
        points.push(id);
    }
    (doc, points)
}

fn bench_re_evaluate_constraints_many_points(c: &mut Criterion) {
    let (mut doc, points) = make_doc_with_50_points();

    // 10 Midpoint constraints.
    for i in 0..10 {
        let a = points[i * 2];
        let b = points[i * 2 + 1];
        doc.add_constructed_object(
            GeoObject::Point(PointObj::new(Point2::new(0.0, 0.0)).with_label("M")),
            "Midpoint",
            &[a, b],
        );
    }

    // 8 Perpendicular constraints: line + point -> perpendicular line.
    for i in 0..8 {
        let a = points[20 + i * 2];
        let b = points[20 + i * 2 + 1];
        let line = doc.add_object(GeoObject::Line(LineObj::new_with_kind(
            doc.point_position(a).unwrap(),
            doc.point_position(b).unwrap(),
            LineKind::Segment,
        )));
        let pt = points[36 + i];
        doc.add_constructed_object(
            GeoObject::Line(LineObj::new_with_kind(
                Point2::new(0.0, 0.0),
                Point2::new(1.0, 1.0),
                LineKind::Line,
            )),
            "Perpendicular",
            &[line, pt],
        );
    }

    // 7 Parallel constraints: line + point -> parallel line.
    for i in 0..7 {
        let a = points[i * 2];
        let b = points[i * 2 + 1];
        let line = doc.add_object(GeoObject::Line(LineObj::new_with_kind(
            doc.point_position(a).unwrap(),
            doc.point_position(b).unwrap(),
            LineKind::Segment,
        )));
        let pt = points[14 + i];
        doc.add_constructed_object(
            GeoObject::Line(LineObj::new_with_kind(
                Point2::new(0.0, 0.0),
                Point2::new(1.0, 1.0),
                LineKind::Line,
            )),
            "Parallel",
            &[line, pt],
        );
    }

    let movable = points[0];
    let order = doc.propagation_order(&[movable]);

    c.bench_function("re_evaluate_constraints_many_points", |b| {
        b.iter(|| {
            // Reset the driver point to a deterministic perturbation each iteration.
            doc.move_point(movable, Point2::new(0.5, 0.5));
            doc.re_evaluate_constraints(black_box(&order));
            black_box(doc.object_count());
        })
    });
}

fn bench_numeric_solver_constraints(c: &mut Criterion) {
    let mut doc = Document::new();
    let mut points = Vec::with_capacity(8);
    for i in 0..8 {
        let angle = i as f64 * std::f64::consts::FRAC_PI_4;
        let id = doc.add_object(GeoObject::Point(PointObj::new(Point2::new(
            angle.cos() * 5.0,
            angle.sin() * 5.0,
        ))));
        points.push(id);
    }

    let l0 = doc.add_object(GeoObject::Line(LineObj::new(
        doc.point_position(points[0]).unwrap(),
        doc.point_position(points[1]).unwrap(),
    )));
    let l1 = doc.add_object(GeoObject::Line(LineObj::new(
        doc.point_position(points[2]).unwrap(),
        doc.point_position(points[3]).unwrap(),
    )));
    let l2 = doc.add_object(GeoObject::Line(LineObj::new(
        doc.point_position(points[4]).unwrap(),
        doc.point_position(points[5]).unwrap(),
    )));
    let l3 = doc.add_object(GeoObject::Line(LineObj::new(
        doc.point_position(points[6]).unwrap(),
        doc.point_position(points[7]).unwrap(),
    )));

    doc.add_distance_constraint(points[0], points[2], 3.0);
    doc.add_angle_constraint(l0, l1, 45.0);
    doc.add_horizontal_constraint(l2);
    doc.add_equal_length_constraint(l0, l3);

    let movable = points[0];
    let order = doc.propagation_order(&[movable]);

    c.bench_function("numeric_solver_constraints", |b| {
        b.iter(|| {
            doc.move_point(movable, Point2::new(1.0, 0.0));
            doc.re_evaluate_constraints(black_box(&order));
            black_box(doc.object_count());
        })
    });
}

fn bench_expression_binding_update(c: &mut Criterion) {
    let mut doc = Document::new();
    doc.set_variable("a".to_string(), 2.0);
    doc.set_variable("r".to_string(), 3.0);
    doc.set_variable("dom_min".to_string(), -5.0);
    doc.set_variable("dom_max".to_string(), 5.0);

    let mut point = PointObj::new(Point2::new(0.0, 0.0));
    point.x_expr = Some("a".to_string());
    doc.add_object(GeoObject::Point(point));

    let mut circle = CircleObj::new(Point2::new(0.0, 0.0), 1.0);
    circle.radius_expr = Some("r".to_string());
    doc.add_object(GeoObject::Circle(circle));

    let mut function = FunctionObj::new("sin(x)");
    function.domain_min_expr = Some("dom_min".to_string());
    function.domain_max_expr = Some("dom_max".to_string());
    doc.add_object(GeoObject::Function(function));

    c.bench_function("expression_binding_update", |b| {
        b.iter(|| {
            doc.set_variable("a".to_string(), black_box(4.0));
            doc.set_variable("r".to_string(), black_box(5.0));
            doc.set_variable("dom_min".to_string(), black_box(-3.0));
            doc.set_variable("dom_max".to_string(), black_box(3.0));
            black_box(doc.object_count());
        })
    });
}

fn make_large_document() -> Document {
    let mut doc = Document::new();
    let mut ids = Vec::with_capacity(200);

    for i in 0..200 {
        let obj = match i % 10 {
            0 | 1 => GeoObject::Point(PointObj::new(Point2::new(
                (i as f64) * 0.1,
                (i as f64).sin(),
            ))),
            2 | 3 => GeoObject::Line(LineObj::new(
                Point2::new(i as f64, 0.0),
                Point2::new(i as f64 + 1.0, 1.0),
            )),
            4 | 5 => GeoObject::Circle(CircleObj::new(
                Point2::new((i as f64) * 0.2, (i as f64) * 0.1),
                0.5 + (i as f64) * 0.01,
            )),
            6 | 7 => GeoObject::Polygon(PolygonObj::new(vec![
                Point2::new(0.0, 0.0),
                Point2::new(1.0, 0.0),
                Point2::new(1.0, 1.0),
                Point2::new(0.0, 1.0),
            ])),
            _ => GeoObject::Function(FunctionObj::new(&format!("sin({}*x)", i))),
        };
        ids.push(doc.add_object(obj));
    }
    // Touch ids so it is not unused.
    black_box(ids.len());
    doc
}

fn bench_serialize_large_document(c: &mut Criterion) {
    let doc = make_large_document();

    c.bench_function("serialize_large_document", |b| {
        b.iter(|| {
            let json = serde_json::to_string(black_box(&doc)).unwrap();
            let round_trip: Document = serde_json::from_str(black_box(&json)).unwrap();
            black_box(round_trip.object_count());
        })
    });
}

fn bench_spatial_index_rebuild(c: &mut Criterion) {
    let mut doc = Document::new();
    for i in 0..500 {
        let angle = i as f64 * 0.1;
        doc.add_object(GeoObject::Point(PointObj::new(Point2::new(
            angle.cos() * (i as f64),
            angle.sin() * (i as f64),
        ))));
    }

    c.bench_function("spatial_index_rebuild", |b| {
        b.iter(|| {
            doc.rebuild_spatial_index();
            black_box(doc.object_count());
        })
    });
}

criterion_group!(
    benches,
    bench_re_evaluate_constraints_many_points,
    bench_numeric_solver_constraints,
    bench_expression_binding_update,
    bench_serialize_large_document,
    bench_spatial_index_rebuild
);
criterion_main!(benches);
