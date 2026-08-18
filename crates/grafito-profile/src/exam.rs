//! Banco docente de mini-exámenes por rama: preguntas deterministas con
//! corrección (numérica tolerante o textual exacta) para que el tutor «Mora»
//! evalúe y luego registre el resultado en el perfil.

use crate::ExamResult;

/// Genera las preguntas de un mini-examen para una rama (determinista).
pub fn mini_exam_questions(branch_id: &str) -> Vec<String> {
    match branch_id {
        "calculus" => vec![
            "Derivá x^2 respecto de x.".to_string(),
            "Derivá x^3 + 2*x respecto de x.".to_string(),
            "¿Cuál es la derivada de sin(x)?".to_string(),
        ],
        "algebra" => vec![
            "Resolvé 2*x + 3 = 11 (x = ?).".to_string(),
            "Factorizá x^2 - 9.".to_string(),
            "Resolvé x^2 - 5*x + 6 = 0 (raíces).".to_string(),
        ],
        "functions" => vec![
            "¿Cuál es la raíz de f(x)=x-4?".to_string(),
            "¿La pendiente de y=2x+1 es?".to_string(),
            "Evaluá f(2) si f(x)=x^2+1.".to_string(),
        ],
        "trigonometry" => vec![
            "¿Cuánto vale sin(0)?".to_string(),
            "¿Cuánto vale cos(90°)?".to_string(),
            "¿La amplitud de sin(2x) es?".to_string(),
        ],
        "geometry" => vec![
            "Área de un cuadrado de lado 3 (unidades²).".to_string(),
            "Perímetro de un cuadrado de lado 5 (unidades).".to_string(),
            "Volumen de un cubo de arista 2 (unidades³).".to_string(),
        ],
        "stats" => vec![
            "Media de {2, 4, 6}.".to_string(),
            "Rango de {1, 3, 8}.".to_string(),
            "Mediana de {1, 5, 9}.".to_string(),
        ],
        "complex" => vec![
            "Parte real de 3+4i.".to_string(),
            "Módulo de 3+4i.".to_string(),
            "Conjugado de 2-3i.".to_string(),
        ],
        _ => vec![
            "¿Qué tema viste en la última explicación?".to_string(),
            "¿Qué definición de la rama recordás?".to_string(),
            "¿Qué ejemplo resolviste?".to_string(),
        ],
    }
}

/// Respuesta correcta canónica por pregunta (rama + índice).
fn mini_exam_answer(branch_id: &str, index: usize) -> &'static str {
    match branch_id {
        "calculus" => ["2*x", "3*x^2 + 2", "cos(x)"],
        "algebra" => ["4", "(x-3)(x+3)", "2 y 3"],
        "functions" => ["4", "2", "5"],
        "trigonometry" => ["0", "0", "1"],
        "geometry" => ["9", "20", "8"],
        "stats" => ["4", "7", "5"],
        "complex" => ["3", "5", "2+3i"],
        _ => ["", "", ""],
    }
    .get(index)
    .copied()
    .unwrap_or("")
}

/// Corrige una respuesta libre: numérica con tolerancia 2% o textual exacta.
pub fn mini_exam_grade(branch_id: &str, index: usize, answer: &str) -> bool {
    let expected = mini_exam_answer(branch_id, index);
    let answer = answer.trim();
    // Normalización textual: ignora espacios, asteriscos y el signo · para
    // aceptar «2x» / «2·x» como respuestas de «2*x».
    let normalize = |text: &str| {
        text.chars()
            .filter(|character| !matches!(character, ' ' | '*' | '·'))
            .collect::<String>()
            .to_lowercase()
    };
    if !expected.is_empty() && normalize(answer) == normalize(expected) {
        return true;
    }
    if let (Ok(left), Ok(right)) = (expected.parse::<f64>(), answer.parse::<f64>()) {
        if (left - right).abs() <= left.abs() * 0.02 + 1e-6 {
            return true;
        }
    }
    false
}

/// Convierte aciertos/total en un `ExamResult` (aprueba con 2/3 o más).
pub fn mini_exam_result(branch_id: &str, epoch: u64, correct: u32, total: u32) -> ExamResult {
    ExamResult {
        epoch,
        branch_id: branch_id.to_string(),
        score: correct.min(total),
        total,
        passed: total > 0 && correct.min(total) * 3 >= total * 2,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn calculus_bank_has_three_questions_and_grades_numeric_tolerance() {
        assert_eq!(mini_exam_questions("calculus").len(), 3);
        assert!(mini_exam_grade("calculus", 0, "2x"));
        assert!(mini_exam_grade("calculus", 0, "2·x"));
        assert!(!mini_exam_grade("calculus", 0, "x"));
    }

    #[test]
    fn numeric_answers_accept_small_deviation_but_reject_wrong_ones() {
        assert!(mini_exam_grade("geometry", 0, "8.95")); // 9 ± 2%
        assert!(!mini_exam_grade("geometry", 0, "3"));
        assert!(mini_exam_grade("stats", 1, "7"));
    }

    #[test]
    fn result_passes_only_at_two_thirds_or_more() {
        assert!(mini_exam_result("algebra", 1, 2, 3).passed);
        assert!(!mini_exam_result("algebra", 1, 1, 3).passed);
    }
}
