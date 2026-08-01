use grafito_core::{
    deserialize_document,
    parametric_sampling::{evaluate_parametric_curve_3d, evaluate_surface_3d},
    serialize_document, Document, GeoObject, ParametricCurve3DObj, Surface3DObj,
};
use grafito_geometry::Point3D;
use std::collections::HashMap;

#[test]
fn curve_3d_sampling_uses_the_declared_parameter() {
    let curve = ParametricCurve3DObj::new("s", "s^2", "2*s", 0.0, 2.0).with_parameter("s");

    let samples = evaluate_parametric_curve_3d(&curve, 2, &HashMap::new());

    assert_eq!(samples.len(), 3);
    assert_eq!(samples[2], (2.0, 4.0, 4.0));
}

#[test]
fn curve_3d_parameter_round_trips_and_defaults_for_legacy_documents() {
    let curve = ParametricCurve3DObj::new("s", "s^2", "2*s", 0.0, 2.0).with_parameter("s");

    let mut document = Document::new();
    document.add_object(GeoObject::ParametricCurve3D(curve.clone()));
    let saved = serialize_document(&document).expect("document must serialize");
    let restored_document = deserialize_document(&saved).expect("document must deserialize");
    let restored_curve = restored_document
        .objects_iter()
        .find_map(|(_, object)| match object {
            GeoObject::ParametricCurve3D(curve) => Some(curve),
            _ => None,
        })
        .expect("saved document must retain its curve");
    assert_eq!(restored_curve.parameter, "s");

    let serialized = serde_json::to_value(&curve).expect("curve must serialize");
    assert_eq!(serialized["parameter"], "s");
    let restored: ParametricCurve3DObj =
        serde_json::from_value(serialized.clone()).expect("curve must deserialize");
    assert_eq!(restored.parameter, "s");

    let mut legacy = serialized;
    legacy
        .as_object_mut()
        .expect("curve JSON must be an object")
        .remove("parameter");
    let restored: ParametricCurve3DObj =
        serde_json::from_value(legacy).expect("legacy curve must deserialize");
    assert_eq!(restored.parameter, "t");
}

#[test]
fn surface_sampling_keeps_document_xyz_for_explicit_and_parametric_surfaces() {
    let explicit = Surface3DObj::new("10*x + y", (1.0, 2.0), (3.0, 4.0));
    let explicit_samples = evaluate_surface_3d(&explicit, 1, &HashMap::new());
    assert_eq!(explicit_samples[0][0], Point3D::new(1.0, 3.0, 13.0));

    let parametric =
        Surface3DObj::new_parametric("u", "10 + v", "100 + u + v", (1.0, 2.0), (3.0, 4.0));
    let parametric_samples = evaluate_surface_3d(&parametric, 1, &HashMap::new());
    assert_eq!(parametric_samples[0][0], Point3D::new(1.0, 13.0, 104.0));
}
