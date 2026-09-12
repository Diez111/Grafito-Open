//! Puente guion→sesión (F2 slice 2): 1 acto = 1 [`TeachingStep`].
//!
//! [`GuionASesion`] es el adaptador mínimo entre el director de guion
//! (`grafito-anim`) y la sesión de enseñanza: cada acto baja a un paso con
//! `id "g{i}"`, `explanation` como join de textos, `manim_template`
//! `"guion:{i}/{n}"`, `math_expr` = primera verificada (vía
//! [`verify_math_expr`]; si ninguna parsea → `None`, el paso se conserva),
//! `cue_ms` tomado de `Playlist::schedule` (NO cues manuales: el paso del
//! acto arranca donde el schedule dice) y `frame_range` como partición del
//! total de frames del guion (`[acc, acc+frames_acto)`).
//!
//! Decisiones honestas (el wire del guion no trae estos datos):
//! - `check` queda `None` en todos los pasos: `PasoGuion` no trae
//!   `probe`/`expected` y fabricar un remate sería mentir. El integrador lo
//!   suma después con `with_final_check`.
//! - Si `a_playlist` falla (inalcanzable vía `try_new`, posible armando el
//!   `Guion` por struct literal tras deserializar), los cues caen a `0`
//!   en vez de paniquear: la sesión se conserva sin binding temporal.
//!
//! Cerebro puro: sin egui, sin wgpu, sin I/O, sin red, sin pánicos.

use grafito_anim::guion::{Guion, PasoGuion};

use crate::teaching::{verify_math_expr, TeachingSession, TeachingStep, TeachingTopic};

/// Adaptador mínimo guion→sesión: 1 acto = 1 paso de enseñanza.
///
/// Se implementa para [`Guion`] (trait local sobre tipo foráneo: sin ciclo
/// de dependencias — `grafito-anim` no depende de `grafito-pedagogy`).
pub trait GuionASesion {
    /// Baja el guion a [`TeachingSession`] (total: nunca falla; ver cues).
    fn a_sesion(&self) -> TeachingSession;
    /// Pistas de pizarra de todo el guion unidas con `"\n"` (vacías fuera).
    fn whiteboard_hint(&self) -> String;
}

/// Primera `math_expr` del acto que pasa el CAS-gate (`None` si ninguna).
fn primera_verificada(pasos: &[PasoGuion]) -> Option<String> {
    pasos
        .iter()
        .filter_map(|p| p.math_expr.clone())
        .find(|e| verify_math_expr(e))
}

/// Hints no vacíos unidos con `"\n"` (vacío si no hay ninguno).
fn unir_hints(pasos: &[PasoGuion]) -> String {
    pasos
        .iter()
        .map(|p| p.whiteboard_hint.trim())
        .filter(|h| !h.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

impl GuionASesion for Guion {
    fn a_sesion(&self) -> TeachingSession {
        let actos = self.actos();
        let n = actos.len();
        // Cues vía `Playlist::schedule` (NO manuales): el cue del acto es el
        // `start_ms` del primer step aplanado del acto. La playlist guarda
        // 1 step por paso (espera → pausa), así el índice aplanado coincide.
        let cues: Vec<u64> = match self.a_playlist() {
            Ok(lista) => {
                let agenda = lista.schedule();
                let mut cues = Vec::with_capacity(n);
                let mut plano: usize = 0;
                for acto in actos {
                    cues.push(agenda.get(plano).map_or(0, |s| s.start_ms));
                    plano = plano.saturating_add(acto.pasos.len());
                }
                cues
            }
            Err(_) => vec![0; n],
        };
        let mut steps = Vec::with_capacity(n);
        let mut acc: u32 = 0;
        for (i, acto) in actos.iter().enumerate() {
            let k = i.saturating_add(1);
            let explanation = acto
                .pasos
                .iter()
                .map(|p| p.texto.trim())
                .filter(|t| !t.is_empty())
                .collect::<Vec<_>>()
                .join("\n");
            // Partición del total: ventana `[acc, acc+frames_acto)`.
            let frames_acto: u32 = acto
                .pasos
                .iter()
                .map(|p| p.frames.min(u32::MAX as usize) as u32)
                .fold(0, |a: u32, f| a.saturating_add(f));
            let inicio = acc;
            acc = acc.saturating_add(frames_acto);
            let mut step = TeachingStep::new(format!("g{k}"), acto.titulo.clone(), explanation)
                .with_whiteboard(unir_hints(&acto.pasos))
                .with_manim(format!("guion:{k}/{n}"))
                .with_cue(cues.get(i).copied().unwrap_or(0));
            if acc > inicio {
                step = step.with_frames(inicio, acc);
            }
            if let Some(expr) = primera_verificada(&acto.pasos) {
                step = step.with_math(expr);
            }
            // `check`: None honesto (sin probe/expected en el wire).
            steps.push(step);
        }
        // `TeachingSession::new` re-aplica el CAS-gate y marca `verified`.
        TeachingSession::new(TeachingTopic::from_text(self.concepto()), steps)
    }

    fn whiteboard_hint(&self) -> String {
        self.actos()
            .iter()
            .flat_map(|a| a.pasos.iter())
            .map(|p| p.whiteboard_hint.trim())
            .filter(|h| !h.is_empty())
            .collect::<Vec<_>>()
            .join("\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use grafito_anim::guion::{ActoTexto, GuionTexto, PasoTexto};
    use std::collections::BTreeMap;

    fn paso(
        texto: &str,
        math: Option<&str>,
        hint: &str,
        template: &str,
        efecto: &str,
    ) -> PasoTexto {
        PasoTexto {
            texto: texto.to_string(),
            math_expr: math.map(str::to_string),
            whiteboard_hint: hint.to_string(),
            template_hint: template.to_string(),
            params: BTreeMap::new(),
            efecto: efecto.to_string(),
            frames: 8,
            run_ms: 1000,
            wait_after_ms: 200,
            voiceover: None,
        }
    }

    fn acto(titulo: &str, pasos: Vec<PasoTexto>) -> ActoTexto {
        ActoTexto {
            titulo: titulo.to_string(),
            fondo: None,
            limpiar: false,
            pasos,
        }
    }

    /// Guion derivada: 3 actos × 2 pasos = 6 steps (el caso del slice).
    fn guion_derivada_3x2() -> Guion {
        let texto = GuionTexto {
            concepto: "derivada".to_string(),
            width: 640,
            height: 480,
            actos: vec![
                acto(
                    "apertura",
                    vec![
                        paso(
                            "recta secante",
                            Some("x^2"),
                            "ejes",
                            "derivative-slope",
                            "create",
                        ),
                        paso(
                            "cociente",
                            Some("((x+h)^2-x^2)/h"),
                            "secante",
                            "derivative-slope",
                            "write",
                        ),
                    ],
                ),
                acto(
                    "nudo",
                    vec![
                        paso("límite", Some("2*x"), "tangente", "integral-area", "fade"),
                        paso("área", Some("x^2"), "región", "integral-area", "grow"),
                    ],
                ),
                acto(
                    "cierre",
                    // Matemática inválida → `None` honesto (el paso sigue).
                    vec![
                        paso(
                            "verificá",
                            Some("f'(x)=2x"),
                            "pizarra",
                            "pitagoras",
                            "indicate",
                        ),
                        paso("cierre final", None, "resumen", "pitagoras", "tracker"),
                    ],
                ),
            ],
        };
        Guion::try_new(texto).expect("guion 3x2 válido")
    }

    #[test]
    fn tres_actos_dan_seis_steps_y_total_coherente() {
        let g = guion_derivada_3x2();
        let lista = g.a_playlist().expect("playlist válida");
        assert_eq!(lista.len(), 6);
        assert_eq!(lista.total_duration_ms(), g.duracion_total_ms());
        assert_eq!(lista.total_duration_ms(), 6 * (1000 + 200));
        // `sample_at` cae en el acto correcto: el acto 2 arranca en el
        // plano 2 (2 pasos por acto).
        let agenda = lista.schedule();
        assert_eq!(agenda[2].start_ms, 2 * 1200);
        let (idx, _) = lista
            .sample_at(agenda[2].start_ms.saturating_add(1))
            .expect("dentro del acto 2");
        assert_eq!(idx, 2);
        // El último instante válido sigue dentro; el total ya terminó.
        assert!(lista.sample_at(lista.total_duration_ms()).is_none());
    }

    #[test]
    fn sesion_un_acto_un_paso_con_cues_del_schedule() {
        let g = guion_derivada_3x2();
        let s = g.a_sesion();
        assert_eq!(s.topic, TeachingTopic::Derivada);
        assert_eq!(s.steps.len(), 3);
        // Ids, títulos, templates y explicación join.
        assert_eq!(s.steps[0].id, "g1");
        assert_eq!(s.steps[1].id, "g2");
        assert_eq!(s.steps[2].id, "g3");
        assert_eq!(s.steps[0].title, "apertura");
        assert_eq!(s.steps[0].manim_template.as_deref(), Some("guion:1/3"));
        assert_eq!(s.steps[2].manim_template.as_deref(), Some("guion:3/3"));
        assert_eq!(s.steps[0].explanation, "recta secante\ncociente");
        // Cues = starts del schedule en bordes de acto (NO manuales).
        let lista = g.a_playlist().expect("playlist válida");
        let agenda = lista.schedule();
        assert_eq!(s.steps[0].cue_ms, agenda[0].start_ms);
        assert_eq!(s.steps[1].cue_ms, agenda[2].start_ms);
        assert_eq!(s.steps[2].cue_ms, agenda[4].start_ms);
        assert_eq!(s.steps[0].cue_ms, 0);
        assert_eq!(s.steps[1].cue_ms, 2400);
        assert_eq!(s.steps[2].cue_ms, 4800);
        // Revelado progresivo coherente con los cues.
        assert_eq!(s.revealed_count(0), 1);
        assert_eq!(s.revealed_count(2399), 1);
        assert_eq!(s.revealed_count(2400), 2);
        assert_eq!(s.revealed_count(u64::MAX), 3);
        assert!(!s.revealed_hints(0).is_empty());
        // Partición del total: 16 frames por acto sobre 48.
        assert_eq!(s.steps[0].frame_range, Some((0, 16)));
        assert_eq!(s.steps[1].frame_range, Some((16, 32)));
        assert_eq!(s.steps[2].frame_range, Some((32, 48)));
        // `cue_frame_index` cae dentro de la ventana del acto.
        let idx = TeachingSession::cue_frame_index(&s.steps[1], 2400, 12.0, 48).expect("índice");
        assert!((16..32).contains(&idx), "idx={idx}");
        // Matemática: primera verificada; acto 3 sin verificable → None.
        assert_eq!(s.steps[0].math_expr.as_deref(), Some("x^2"));
        assert!(s.steps[0].verified);
        assert_eq!(s.steps[1].math_expr.as_deref(), Some("2*x"));
        assert_eq!(s.steps[2].math_expr, None);
        assert!(!s.steps[2].verified);
        // Remate: None honesto (el wire no trae probe/expected).
        assert!(s.steps.iter().all(|st| st.check.is_none()));
        // Hint global: join de los 6 hints.
        assert_eq!(
            g.whiteboard_hint(),
            "ejes\nsecante\ntangente\nregión\npizarra\nresumen"
        );
    }

    #[test]
    fn viewport_mixto_falla_honesto() {
        let r = grafito_anim::guion::comprobar_viewport_unico(&[(640, 480), (800, 600)]);
        let e = r.expect_err("viewport mixto debe fallar");
        assert!(e.to_string().contains("mixto"));
        assert!(grafito_anim::guion::comprobar_viewport_unico(&[(640, 480), (640, 480)]).is_ok());
    }
}
