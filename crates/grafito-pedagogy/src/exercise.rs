//! Ejercicios — generación y validación, sin red.

use crate::curriculum::LearningObjective;
use crate::level::PedagogicalLevel;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExerciseKind {
    Numeric,
    Symbolic,
    Graphical,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExerciseDifficulty {
    Easy,
    Medium,
    Hard,
}

/// Estrategia de validación de la respuesta.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
pub enum ValidatorKind {
    /// Comparación exacta tras normalizar.
    #[default]
    Exact,
    /// Tolerancia numérica relativa (ej 0.02 = 2 %).
    NumericTol(f64),
    /// Validación simbólica (requiere CAS).
    Symbolic,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Exercise {
    pub prompt: String,
    pub solution: String,
    pub kind: ExerciseKind,
    pub difficulty: ExerciseDifficulty,
    pub lo_id: String,
    #[serde(default)]
    pub params: HashMap<String, f64>,
    #[serde(default)]
    pub seed: Option<u64>,
    #[serde(default)]
    pub validator: ValidatorKind,
}

impl Exercise {
    pub fn validate(&self) -> Result<(), String> {
        if self.prompt.trim().is_empty() || self.solution.trim().is_empty() {
            return Err("ejercicio incompleto".into());
        }
        if self.prompt.len() > 500 || self.solution.len() > 500 {
            return Err("ejercicio demasiado largo".into());
        }
        match self.validator {
            ValidatorKind::NumericTol(tol) => {
                if !tol.is_finite() || tol <= 0.0 || tol > 1.0 {
                    return Err("tolerancia inválida".into());
                }
            }
            ValidatorKind::Exact | ValidatorKind::Symbolic => {}
        }
        for (k, v) in &self.params {
            if k.trim().is_empty() {
                return Err("clave de parámetro vacía".into());
            }
            if !v.is_finite() {
                return Err(format!("parámetro '{k}' no finito"));
            }
        }
        Ok(())
    }
}

/// Generador determinista — mapea LO + nivel + seed a ejercicio.
///
/// # Dificultad por nivel pedagógico (no solo `level_value` numérico)
///
/// | `PedagogicalLevel` | `ExerciseDifficulty` | `level_value` | Criterio |
/// |---|---|---|---|
/// | `Primary` | `Easy` | 2 | Operaciones con enteros 1..5, sin fracciones complejas |
/// | `Secondary` | `Medium` | 8 | Incluye fracciones, trigonometría discreta (4 variantes sin) |
/// | `University` | `Hard` | 15 | Parámetros simbólicos, tolerancia numérica 2% |
/// | `UTN(AM1)` | `Hard` | 12 | Cálculo: derivadas/integrales con `NumericTol(0.02)` |
/// | `UTN(AM2)` | `Hard` | 14 | Series/Taylor, multivariable |
/// | `UTN(Algebra)` | `Hard` | 13 | Matrices, transformaciones |
/// | `UTN(Probabilidad)` | `Hard` | 15 | Distribuciones, inferencia |
///
/// Nota: `level_value` colisionaba históricamente en 14 (AM2 vs Probabilidad);
/// la dificultad se decide por `match level` (variante enum), no por el
/// número crudo, por eso `Probabilidad` y `AM2` ambas son `Hard` aunque
/// ahora difieran en `level_value` (14 vs 15).
#[derive(Debug, Clone, Default)]
pub struct ExerciseGenerator;

const WY_CONST: u64 = 0x9E3779B97F4A7C15;
const WY_CONST2: u64 = 0xBF58476D1CE4E5B9;

fn wyhash(seed: u64) -> u64 {
    seed.wrapping_mul(WY_CONST)
}

fn wyhash2(seed: u64) -> u64 {
    seed.wrapping_mul(WY_CONST).wrapping_add(WY_CONST2)
}

impl ExerciseGenerator {
    pub fn generate(&self, lo: &LearningObjective, level: PedagogicalLevel) -> Exercise {
        self.generate_with_seed(lo, level, 0)
    }

    pub fn generate_with_seed(
        &self,
        lo: &LearningObjective,
        level: PedagogicalLevel,
        seed: u64,
    ) -> Exercise {
        let difficulty = match level {
            PedagogicalLevel::Primary => ExerciseDifficulty::Easy,
            PedagogicalLevel::Secondary => ExerciseDifficulty::Medium,
            _ => ExerciseDifficulty::Hard,
        };

        // Helpers para coeficientes deterministas vía wyhash-like
        let h0 = wyhash(seed);
        let h1 = wyhash2(h0);
        let h2 = wyhash(h1);

        let (prompt, solution, kind, validator, params) = match lo.id.as_str() {
            "am1-der" => {
                // a = 1 + seed%3 pero mezclado con wyhash para determinismo
                // Usamos h0 para a, h1 para b
                let a = 1 + (h0 % 3);
                let b = 1 + (h1 % 3);
                let prompt = format!("Deriva f(x)={}*x^2 + {}*x en x=1", a, b);
                let sol_val = 2 * a + b;
                let solution = sol_val.to_string();
                let mut params = HashMap::new();
                params.insert("a".to_string(), a as f64);
                params.insert("b".to_string(), b as f64);
                // variante por seed%5 documentada: ya implícita en a,b
                let _variant = seed % 5;
                (
                    prompt,
                    solution,
                    ExerciseKind::Symbolic,
                    ValidatorKind::NumericTol(0.02),
                    params,
                )
            }
            "am1-int" => {
                let a = 1 + (h0 % 3);
                let prompt = format!("Calcula ∫₀¹ {}*x^2 dx", a);
                let val = a as f64 / 3.0;
                // Solución con 10 decimales recortados, tolerancia 2% permite fracciones
                let solution = if a.is_multiple_of(3) {
                    (val as i64).to_string()
                } else {
                    // Para a=1 => 0.3333333333, para a=2 => 0.666...
                    // Dejamos representación decimal completa para parse numérico
                    format!("{val}")
                };
                let mut params = HashMap::new();
                params.insert("a".to_string(), a as f64);
                (
                    prompt,
                    solution,
                    ExerciseKind::Symbolic,
                    ValidatorKind::NumericTol(0.02),
                    params,
                )
            }
            "sec-trig" => {
                let k = seed % 4;
                let (prompt, solution) = match k {
                    0 => ("¿Cuánto vale sin(0)?".to_string(), "0".to_string()),
                    1 => ("¿Cuánto vale sin(π/2)?".to_string(), "1".to_string()),
                    2 => ("¿Cuánto vale sin(π)?".to_string(), "0".to_string()),
                    3 => ("¿Cuánto vale sin(3·π/2)?".to_string(), "-1".to_string()),
                    _ => ("¿Cuánto vale sin(0)?".to_string(), "0".to_string()),
                };
                let mut params = HashMap::new();
                params.insert("k".to_string(), k as f64);
                // también guardamos ángulo en radianes
                let angle = k as f64 * std::f64::consts::FRAC_PI_2;
                params.insert("angle_rad".to_string(), angle);
                (
                    prompt,
                    solution,
                    ExerciseKind::Numeric,
                    ValidatorKind::NumericTol(0.02),
                    params,
                )
            }
            _ => {
                // Genérico paramétrico: Si f(x)=a*x+b, evalúa en x=c
                // a,b,c en 1..5 vía wyhash
                let a = 1 + (h0 % 5);
                let b = 1 + (h1 % 5);
                let c = 1 + (h2 % 5);
                let prompt = format!("Si f(x)={}*x+{}, evalúa en x={}", a, b, c);
                let sol = a * c + b;
                let solution = sol.to_string();
                let mut params = HashMap::new();
                params.insert("a".to_string(), a as f64);
                params.insert("b".to_string(), b as f64);
                params.insert("c".to_string(), c as f64);
                // para LOs conocidos, ajustamos prompt levemente para variedad
                let (prompt, kind) = match lo.id.as_str() {
                    "am1-func" => (prompt, ExerciseKind::Numeric),
                    "am1-lim" => {
                        // Variante límite: usa mismo a,b,c pero frasea como límite
                        let p =
                            format!("Si f(x)={}*x+{}, ¿cuánto vale lim_{{x→{}}} f(x)?", a, b, c);
                        (p, ExerciseKind::Symbolic)
                    }
                    "am1-cont" => (prompt, ExerciseKind::Symbolic),
                    "am1-der-aplic" | "am1-int-aplic" | "am1-sucesiones" => {
                        (prompt, ExerciseKind::Numeric)
                    }
                    id if id.starts_with("sec-") => (prompt, ExerciseKind::Numeric),
                    _ => (prompt, ExerciseKind::Graphical),
                };
                // Validator: numérico si solución es numérica
                let validator = ValidatorKind::NumericTol(0.02);
                (prompt, solution, kind, validator, params)
            }
        };

        Exercise {
            prompt,
            solution,
            kind,
            difficulty,
            lo_id: lo.id.clone(),
            params,
            seed: Some(seed),
            validator,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn generate_valid() {
        let lo = LearningObjective::new("am1-der", "Derivadas", "...", None);
        let ex = ExerciseGenerator.generate(&lo, PedagogicalLevel::Secondary);
        assert!(ex.validate().is_ok());
    }

    #[test]
    fn generate_with_seed_deterministic() {
        let lo = LearningObjective::new("am1-der", "Derivadas", "...", None);
        let gen = ExerciseGenerator;
        let ex1 = gen.generate_with_seed(&lo, PedagogicalLevel::Secondary, 42);
        let ex2 = gen.generate_with_seed(&lo, PedagogicalLevel::Secondary, 42);
        assert_eq!(ex1.prompt, ex2.prompt);
        assert_eq!(ex1.solution, ex2.solution);
        assert_eq!(ex1.seed, Some(42));
        assert_eq!(ex1.params, ex2.params);
    }

    #[test]
    fn generate_with_seed_varies() {
        let lo = LearningObjective::new("am1-der", "Derivadas", "...", None);
        let gen = ExerciseGenerator;
        let ex0 = gen.generate_with_seed(&lo, PedagogicalLevel::Secondary, 0);
        let ex1 = gen.generate_with_seed(&lo, PedagogicalLevel::Secondary, 1);
        // con wyhash deberían diferir en al menos prompt o params
        assert!(ex0.prompt != ex1.prompt || ex0.params != ex1.params);
    }

    #[test]
    fn generate_seed_zero_is_generate() {
        let lo = LearningObjective::new("am1-der", "Derivadas", "...", None);
        let gen = ExerciseGenerator;
        let a = gen.generate(&lo, PedagogicalLevel::Secondary);
        let b = gen.generate_with_seed(&lo, PedagogicalLevel::Secondary, 0);
        assert_eq!(a.prompt, b.prompt);
        assert_eq!(a.solution, b.solution);
    }

    #[test]
    fn am1_der_coefficients_and_solution() {
        let lo = LearningObjective::new("am1-der", "Derivadas", "...", None);
        let gen = ExerciseGenerator;
        for seed in 0..10u64 {
            let ex = gen.generate_with_seed(&lo, PedagogicalLevel::Secondary, seed);
            assert!(ex.prompt.contains("Deriva"));
            assert!(ex.params.contains_key("a"));
            assert!(ex.params.contains_key("b"));
            let a = ex.params["a"];
            let b = ex.params["b"];
            assert!((1.0..=3.0).contains(&a));
            assert!((1.0..=3.0).contains(&b));
            let expected = 2.0 * a + b;
            let sol: f64 = ex.solution.parse().unwrap_or(f64::NAN);
            assert!(
                (sol - expected).abs() < 1e-9,
                "seed {seed} sol {sol} expected {expected}"
            );
            match ex.validator {
                ValidatorKind::NumericTol(t) => assert!((t - 0.02).abs() < 1e-9),
                _ => panic!("validator debe ser NumericTol"),
            }
        }
    }

    #[test]
    fn am1_int_solution() {
        let lo = LearningObjective::new("am1-int", "Integrales", "...", None);
        let gen = ExerciseGenerator;
        let ex = gen.generate_with_seed(&lo, PedagogicalLevel::Secondary, 0);
        assert!(ex.prompt.contains("∫₀¹"));
        let a = ex.params["a"];
        let sol: f64 = ex.solution.parse().unwrap_or_else(|_| {
            // si es fracción a/3 como "1/3", parse manual
            if ex.solution.contains('/') {
                let parts: Vec<&str> = ex.solution.split('/').collect();
                parts[0].parse::<f64>().unwrap() / parts[1].parse::<f64>().unwrap()
            } else {
                f64::NAN
            }
        });
        let expected = a / 3.0;
        assert!((sol - expected).abs() < 1e-9);
        assert!(matches!(ex.validator, ValidatorKind::NumericTol(_)));
    }

    #[test]
    fn sec_trig_variants() {
        let lo = LearningObjective::new("sec-trig", "Trigonometría", "...", None);
        let gen = ExerciseGenerator;
        let expectations = [0.0, 1.0, 0.0, -1.0];
        for k in 0..4u64 {
            let ex = gen.generate_with_seed(&lo, PedagogicalLevel::Secondary, k);
            let sol: f64 = ex.solution.parse().unwrap();
            assert_eq!(sol, expectations[k as usize]);
            assert!(ex.prompt.contains("sin"));
            assert!(matches!(ex.validator, ValidatorKind::NumericTol(_)));
        }
    }

    #[test]
    fn generic_parametric() {
        let lo = LearningObjective::new("am1-func", "Funciones", "...", None);
        let gen = ExerciseGenerator;
        let ex = gen.generate_with_seed(&lo, PedagogicalLevel::Secondary, 7);
        assert!(ex.prompt.contains("f(x)"));
        assert!(ex.params.contains_key("a"));
        assert!(ex.params.contains_key("b"));
        assert!(ex.params.contains_key("c"));
        let a = ex.params["a"];
        let b = ex.params["b"];
        let c = ex.params["c"];
        let expected = a * c + b;
        let sol: f64 = ex.solution.parse().unwrap();
        assert!((sol - expected).abs() < 1e-9);
        assert_eq!(ex.seed, Some(7));
    }

    #[test]
    fn validate_rejects_bad_validator() {
        let mut ex = ExerciseGenerator.generate(
            &LearningObjective::new("am1-der", "D", "...", None),
            PedagogicalLevel::Secondary,
        );
        ex.validator = ValidatorKind::NumericTol(f64::NAN);
        assert!(ex.validate().is_err());
        ex.validator = ValidatorKind::NumericTol(0.0);
        assert!(ex.validate().is_err());
        ex.validator = ValidatorKind::NumericTol(2.0);
        assert!(ex.validate().is_err());
        ex.validator = ValidatorKind::NumericTol(0.02);
        assert!(ex.validate().is_ok());
    }

    #[test]
    fn validate_rejects_non_finite_param() {
        let mut ex = ExerciseGenerator.generate(
            &LearningObjective::new("am1-der", "D", "...", None),
            PedagogicalLevel::Secondary,
        );
        ex.params.insert("a".to_string(), f64::INFINITY);
        assert!(ex.validate().is_err());
    }

    #[test]
    fn serde_backward_compat() {
        // JSON sin params/seed/validator debe deserializar con defaults
        let json =
            r#"{"prompt":"p","solution":"s","kind":"Numeric","difficulty":"Easy","lo_id":"x"}"#;
        let ex: Exercise = serde_json::from_str(json).expect("deserializa");
        assert!(ex.params.is_empty());
        assert!(ex.seed.is_none());
        assert_eq!(ex.validator, ValidatorKind::Exact);
        assert!(ex.validate().is_ok());
    }

    #[test]
    fn difficulty_maps_level() {
        let lo = LearningObjective::new("am1-der", "D", "...", None);
        let gen = ExerciseGenerator;
        let ex = gen.generate_with_seed(&lo, PedagogicalLevel::Primary, 0);
        assert_eq!(ex.difficulty, ExerciseDifficulty::Easy);
        let ex = gen.generate_with_seed(&lo, PedagogicalLevel::Secondary, 0);
        assert_eq!(ex.difficulty, ExerciseDifficulty::Medium);
        let ex = gen.generate_with_seed(&lo, PedagogicalLevel::University, 0);
        assert_eq!(ex.difficulty, ExerciseDifficulty::Hard);
    }
}
