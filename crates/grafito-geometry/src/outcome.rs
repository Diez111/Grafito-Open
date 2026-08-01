//! Resultados tipados para operaciones matemáticas que pueden no producir un valor.

/// Límite de bytes aceptado por las APIs matemáticas que reciben expresiones.
///
/// Evita que una sola entrada no confiable agote la memoria o la pila del parser.
pub const MAX_MATH_INPUT_BYTES: usize = 100_000;

/// Resultado de una operación matemática.
///
/// `Exact` se reserva para resultados simbólicos o algebraicos comprobados. Las
/// cuadraturas y otros métodos numéricos deben devolver `Approximate` con una
/// estimación explícita de su error. Los demás casos expresan el estado de la
/// operación mediante [`MathError`] sin que el consumidor deba clasificar texto.
#[derive(Debug, Clone, PartialEq)]
pub enum MathResult<T> {
    /// Resultado matemáticamente exacto.
    Exact(T),
    /// Resultado numérico junto con una cota/estimación absoluta de error.
    Approximate { value: T, error_estimate: f64 },
    /// La expresión o alguno de sus valores no está definido en el dominio pedido.
    DomainError(MathError),
    /// El método iterativo agotó su profundidad antes de satisfacer la tolerancia.
    NotConverged(MathError),
    /// La operación solicitada no tiene una implementación matemática disponible.
    Unsupported(MathError),
    /// La entrada o el cálculo excedió un presupuesto de recursos establecido.
    ResourceLimit(MathError),
}

/// Detalles estructurados para un resultado matemático no satisfactorio.
#[derive(Debug, Clone, PartialEq)]
pub enum MathError {
    /// La expresión no pudo analizarse para la operación indicada.
    InvalidExpression {
        operation: MathOperation,
        expression: String,
        reason: String,
    },
    /// Una entrada textual superó el presupuesto de tamaño permitido.
    InputTooLarge {
        operation: MathOperation,
        provided_bytes: usize,
        maximum_bytes: usize,
    },
    /// No existe una regla implementada para la derivada solicitada.
    DerivativeUnavailable {
        expression: String,
        variable: String,
        reason: String,
    },
    /// No existe una antiderivada simbólica implementada para la expresión.
    AntiderivativeUnavailable {
        expression: String,
        variable: String,
    },
    /// El intervalo contiene o puede contener un punto fuera del dominio.
    IntervalDomainViolation {
        expression: String,
        variable: String,
        lower: f64,
        upper: f64,
    },
    /// La evaluación produjo un valor no finito en un punto concreto.
    NonFiniteValue {
        expression: String,
        variable: String,
        at: f64,
    },
    /// La cuadratura no alcanzó la tolerancia antes de agotar su profundidad.
    RecursionLimit {
        operation: MathOperation,
        lower: f64,
        upper: f64,
        max_depth: u32,
        tolerance: f64,
        error_estimate: f64,
    },
    /// Los límites laterales no convergen al mismo valor finito.
    LimitDoesNotExist {
        expression: String,
        variable: String,
        at: f64,
    },
    /// El enfoque pedido no es un número real finito.
    NonFiniteLimitPoint {
        expression: String,
        variable: String,
        at: f64,
    },
}

/// Operación que originó un [`MathError`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MathOperation {
    /// Derivación simbólica.
    SymbolicDerivative,
    /// Integración indefinida simbólica.
    IndefiniteIntegration,
    /// Integración definida con preferencia simbólica.
    DefiniteIntegration,
    /// Cuadratura numérica directa.
    NumericalIntegration,
    /// Límite numérico bilateral.
    Limit,
}
