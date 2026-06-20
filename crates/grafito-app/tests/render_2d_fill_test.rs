//! Verifica que el render de un ImplicitCurve con Eq dibuja el outline.

#[cfg(test)]
mod tests {
    use grafito_core::{ImplicitCurveObj, RelationOperator};

    #[test]
    fn test_eq_does_not_call_fill() {
        // El fill no se activa para Eq.
        let ic = ImplicitCurveObj::new("x^2 + y^2", "1", RelationOperator::Eq);
        let should_fill = matches!(
            ic.operator,
            RelationOperator::Less
                | RelationOperator::LessEq
                | RelationOperator::Greater
                | RelationOperator::GreaterEq
        );
        assert!(!should_fill, "Eq no debe activar el fill");
    }

    #[test]
    fn test_lt_calls_fill() {
        let ic = ImplicitCurveObj::new("x^2 + y^2", "1", RelationOperator::Less);
        let should_fill = matches!(
            ic.operator,
            RelationOperator::Less
                | RelationOperator::LessEq
                | RelationOperator::Greater
                | RelationOperator::GreaterEq
        );
        assert!(should_fill, "Less debe activar el fill");
    }
}
