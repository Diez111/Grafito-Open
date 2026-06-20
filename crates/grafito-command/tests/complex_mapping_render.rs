//! Verifica que el ComplexMapping con ImplicitCurve como target
//! se renderiza correctamente.

use grafito_command::commands::process_input;
use grafito_core::{GeoObject, ImplicitCurveObj, RelationOperator};
use std::collections::HashMap;

#[test]
fn test_complex_mapping_1_over_z_of_circle() {
    // El caso reportado: x^2 + y^2 < 1, mapeado por 1/z.
    let mut doc = grafito_core::Document::new();
    process_input(&mut doc, &mut "x^2 + y^2 < 1".to_string());
    process_input(&mut doc, &mut "ComplexMapping[1/z, I]".to_string());
    // Buscar el ComplexMapping.
    let cm = doc.objects_iter().find_map(|(_, o)| {
        if let GeoObject::ComplexMapping(cm) = o {
            Some(cm)
        } else {
            None
        }
    });
    let ic = doc.objects_iter().find_map(|(_, o)| {
        if let GeoObject::ImplicitCurve(ic) = o {
            Some(ic)
        } else {
            None
        }
    });
    assert!(cm.is_some(), "no se creó ComplexMapping");
    assert!(ic.is_some(), "no se encontró ImplicitCurve");
    let cm = cm.unwrap();
    let ic = ic.unwrap();
    // El conformal_cache debe estar lleno.
    assert!(
        cm.conformal_cache.is_some(),
        "conformal_cache debe estar lleno"
    );
    // El fill del ImplicitCurve debe estar habilitado.
    assert!(ic.fill_color.is_some());
    assert_eq!(ic.operator, RelationOperator::Less);
}
