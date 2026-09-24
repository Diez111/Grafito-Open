//! Ventana "Colab Pro": pareo en un clic + jobs pesados del lab.
//!
//! Estilo escandinavo quiet (patrón `draw_about_window`): título centrado,
//! ritmo SPACE_MD/LG, hairlines, cards sutiles sobre `toolbar_bg`, botón
//! primario en acento y secundario en `panel_bg`. Sin adornos.
//!
//! Regla de la casa: cero I/O en el dibujado — los botones solo mandan
//! comandos al worker de `colab_link` y el `poll` drena eventos. El trabajo
//! pesado (spawn, stdio, timeouts) vive en el hilo worker.

use crate::colab_link::{
    self, lab_jobs_dir, list_lab_jobs, mcp_call_once, pick_string_arg, read_job_script, ColabPhase,
    JobMeta, ToolInfo,
};
use crate::GrafitoApp;
use grafito_ui::i18n::t;
use grafito_ui::theme::{current_theme, Theme};
use grafito_ui::tokens::{
    RADIUS_MD, RADIUS_SM, SPACE_MD, SPACE_SM, SPACE_XS, TYPE_LG, TYPE_SM, TYPE_XS,
};
use serde_json::{json, Value};
use std::time::{Duration, Instant};

/// Ventana de frescura del cache de jobs: `draw_jobs` re-lee el disco como
/// máximo una vez cada 2 s (cero I/O en el resto de los frames).
pub const JOBS_CACHE_TTL: Duration = Duration::from_secs(2);

/// Job seleccionado en el panel (no persiste entre sesiones a propósito).
#[derive(Debug, Default)]
pub struct ColabPanelState {
    pub selected_job: Option<String>,
    pub script_preview: String,
    pub import_note: String,
    /// Cache de `list_lab_jobs` para no tocar fs en cada frame.
    pub jobs_cache: Vec<JobMeta>,
    /// Cuándo se llenó `jobs_cache` (`None` = nunca: carga en el primer frame).
    pub jobs_cached_at: Option<Instant>,
}

impl ColabPanelState {
    /// Refresca el cache si está vacío o vencido el TTL. Única vía con I/O;
    /// `draw_jobs` solo lee `cached_jobs` (cero I/O por frame).
    pub fn refresh_jobs_if_stale(&mut self, now: Instant) {
        self.refresh_jobs_with(now, list_lab_jobs);
    }

    /// Seam testeable: misma política con loader inyectado (sin fs en tests).
    pub fn refresh_jobs_with(&mut self, now: Instant, listar: impl FnOnce() -> Vec<JobMeta>) {
        let vencido = self
            .jobs_cached_at
            .is_none_or(|cuando| now.saturating_duration_since(cuando) >= JOBS_CACHE_TTL);
        if vencido {
            self.jobs_cache = listar();
            self.jobs_cached_at = Some(now);
        }
    }

    /// Lectura del cache (sin I/O).
    pub fn cached_jobs(&self) -> &[JobMeta] {
        &self.jobs_cache
    }

    /// Invalida el cache (llamar tras importar/ejecutar que cree jobs).
    pub fn invalidate_jobs(&mut self) {
        self.jobs_cached_at = None;
    }
}

/// Dibuja la ventana Colab. Llamar una vez por frame cuando visible.
pub fn draw_colab_window(app: &mut GrafitoApp, ctx: &egui::Context) {
    let locale = app.config_locale();
    let theme = current_theme(ctx);
    // Drena eventos del worker antes de dibujar (cambia fase/tools/log).
    let changed = app.colab.poll();
    if changed || app.colab.is_busy() {
        ctx.request_repaint();
    }
    let mut open = app.show_colab_window;
    egui::Window::new(t("colab.title", locale))
        .id(egui::Id::new("colab_window"))
        .open(&mut open)
        .collapsible(false)
        .resizable(true)
        .default_width(460.0)
        .frame(
            egui::Frame::window(&ctx.style())
                .fill(theme.toolbar_bg)
                .stroke(egui::Stroke::new(1.0, theme.separator.gamma_multiply(0.10)))
                .inner_margin(egui::Margin::symmetric(20.0, 16.0)),
        )
        .show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.label(
                    egui::RichText::new("Colab Pro")
                        .size(TYPE_LG)
                        .strong()
                        .color(theme.accent),
                );
                ui.add_space(SPACE_XS);
                ui.label(
                    egui::RichText::new("cómputo pesado · cuenta Pro")
                        .size(TYPE_XS)
                        .color(theme.text_secondary),
                );
            });
            ui.add_space(SPACE_MD);
            draw_status_card(ui, app, ctx, theme);
            ui.add_space(SPACE_MD);
            section(ui, theme, "Notebook", |ui| draw_tools(ui, app, theme));
            ui.add_space(SPACE_MD);
            section(ui, theme, "Jobs del lab", |ui| {
                draw_jobs(ui, app, ctx, theme)
            });
            ui.add_space(SPACE_MD);
            section(ui, theme, "Registro", |ui| draw_log(ui, app, theme));
        });
    app.show_colab_window = open;
}

/// Sección quiet: título en acento + hairline + contenido con aire.
fn section(ui: &mut egui::Ui, theme: &Theme, title: &str, content: impl FnOnce(&mut egui::Ui)) {
    ui.label(
        egui::RichText::new(title)
            .strong()
            .size(TYPE_SM)
            .color(theme.accent),
    );
    ui.add_space(SPACE_XS);
    ui.separator();
    ui.add_space(SPACE_SM);
    content(ui);
}

/// Botón primario (acento) y secundario (panel) del sistema.
fn primary_button(ui: &mut egui::Ui, theme: &Theme, label: &str) -> egui::Response {
    ui.add_sized(
        egui::vec2(ui.available_width(), 32.0),
        egui::Button::new(
            egui::RichText::new(label)
                .size(TYPE_SM)
                .color(egui::Color32::WHITE)
                .strong(),
        )
        .rounding(RADIUS_MD)
        .fill(theme.accent)
        .stroke(egui::Stroke::NONE),
    )
}

fn secondary_button(ui: &mut egui::Ui, theme: &Theme, label: String) -> egui::Response {
    ui.add(
        egui::Button::new(
            egui::RichText::new(label)
                .size(TYPE_SM)
                .color(theme.text_secondary),
        )
        .rounding(RADIUS_MD)
        .fill(theme.panel_bg)
        .stroke(egui::Stroke::new(1.0, theme.separator)),
    )
}

fn status_dot(ui: &mut egui::Ui, phase: ColabPhase, theme: &Theme) {
    let (color, label) = match phase {
        ColabPhase::Idle => (theme.text_secondary, "○"),
        ColabPhase::Busy => (theme.accent, "◌"),
        ColabPhase::Ready => (egui::Color32::from_rgb(0x5D, 0xA0, 0x5D), "●"),
        ColabPhase::Failed => (egui::Color32::from_rgb(0xB0, 0x4A, 0x4A), "●"),
    };
    ui.label(egui::RichText::new(label).size(TYPE_LG).color(color));
}

fn draw_status_card(ui: &mut egui::Ui, app: &mut GrafitoApp, ctx: &egui::Context, theme: &Theme) {
    let locale = app.config_locale();
    egui::Frame::none()
        .fill(theme.panel_bg)
        .rounding(RADIUS_SM)
        .inner_margin(egui::Margin::symmetric(12.0, 10.0))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                status_dot(ui, app.colab.phase, theme);
                ui.label(
                    egui::RichText::new(app.colab.status.clone())
                        .strong()
                        .size(TYPE_SM),
                );
                if app.colab.is_busy() {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.spinner();
                    });
                }
            });
            ui.add_space(SPACE_XS);
            ui.label(
                egui::RichText::new(t("colab.connect_hint", locale))
                    .size(TYPE_XS)
                    .color(theme.text_secondary),
            );
            ui.add_space(SPACE_SM);
            if app.colab.phase == ColabPhase::Ready {
                if secondary_button(ui, theme, "Desconectar".to_string()).clicked() {
                    app.colab.request_disconnect();
                }
            } else {
                let label = if app.colab.phase == ColabPhase::Failed {
                    t("colab.retry", locale)
                } else {
                    t("colab.connect", locale)
                };
                ui.add_enabled_ui(!app.colab.is_busy(), |ui| {
                    if primary_button(ui, theme, label).clicked() {
                        app.colab.output.clear();
                        app.colab_panel.import_note.clear();
                        app.colab.request_connect();
                        ctx.request_repaint();
                    }
                });
            }
        });
}

fn draw_tools(ui: &mut egui::Ui, app: &mut GrafitoApp, theme: &Theme) {
    if app.colab.tools.is_empty() {
        ui.label(
            egui::RichText::new("Sin tools: pareá primero.")
                .size(TYPE_XS)
                .color(theme.text_secondary),
        );
        return;
    }
    egui::ScrollArea::vertical()
        .max_height(110.0)
        .show(ui, |ui| {
            let names: Vec<(String, String)> = app
                .colab
                .tools
                .iter()
                .map(|t: &ToolInfo| (t.name.clone(), t.description.clone()))
                .collect();
            for (name, desc) in names {
                let selected = app.colab.executor.as_deref() == Some(&name);
                let short = if desc.len() > 90 {
                    format!("{}…", &desc[..90.min(desc.len())])
                } else {
                    desc
                };
                if ui.radio(selected, &name).on_hover_text(short).clicked() {
                    app.colab.executor = Some(name);
                }
            }
        });
    if let Some(exec) = app.colab.executor.clone() {
        let arg = app
            .colab
            .tools
            .iter()
            .find(|t| t.name == exec)
            .and_then(|t| pick_string_arg(&t.input_schema));
        ui.add_space(SPACE_XS);
        ui.label(
            egui::RichText::new(format!(
                "Ejecutor: {exec} (arg: {})",
                arg.as_deref().unwrap_or("?")
            ))
            .size(TYPE_XS)
            .color(theme.text_secondary),
        );
    }
}

fn draw_jobs(ui: &mut egui::Ui, app: &mut GrafitoApp, ctx: &egui::Context, theme: &Theme) {
    ui.label(
        egui::RichText::new(format!("carpeta: {}", lab_jobs_dir().display()))
            .size(TYPE_XS)
            .color(theme.text_secondary),
    );
    ui.add_space(SPACE_XS);
    // Cero I/O por frame: una sola lectura cada `JOBS_CACHE_TTL`, el resto
    // lee el cache en memoria.
    app.colab_panel.refresh_jobs_if_stale(Instant::now());
    let jobs: Vec<JobMeta> = app.colab_panel.cached_jobs().to_vec();
    if jobs.is_empty() {
        ui.label(
            egui::RichText::new("Sin jobs todavía: el agente los crea con export_colab_job.")
                .size(TYPE_XS)
                .color(theme.text_secondary),
        );
        return;
    }
    egui::ScrollArea::vertical()
        .max_height(90.0)
        .show(ui, |ui| {
            for job in &jobs {
                let selected = app.colab_panel.selected_job.as_deref() == Some(&job.job_id);
                ui.horizontal(|ui| {
                    if ui.radio(selected, &job.job_id[..12]).clicked() {
                        app.colab_panel.selected_job = Some(job.job_id.clone());
                        match read_job_script(&job.job_id, 1200) {
                            Ok(preview) => {
                                app.colab_panel.script_preview = preview;
                                app.colab_panel.import_note.clear();
                            }
                            Err(e) => {
                                app.colab_panel.script_preview.clear();
                                app.colab_panel.import_note = format!("script ilegible: {e}");
                            }
                        }
                    }
                    ui.label(
                        egui::RichText::new(format!("… · {}", job.kind))
                            .size(TYPE_XS)
                            .color(theme.text_secondary),
                    );
                });
            }
        });
    if app.colab_panel.selected_job.is_none() {
        return;
    }
    ui.add_space(SPACE_SM);
    egui::Frame::none()
        .fill(theme.panel_bg)
        .rounding(RADIUS_SM)
        .inner_margin(egui::Margin::symmetric(12.0, 8.0))
        .show(ui, |ui| {
            egui::ScrollArea::vertical()
                .max_height(110.0)
                .show(ui, |ui| {
                    ui.label(
                        egui::RichText::new(app.colab_panel.script_preview.clone())
                            .monospace()
                            .size(TYPE_XS),
                    );
                });
        });
    ui.add_space(SPACE_SM);
    ui.horizontal(|ui| {
        if secondary_button(ui, theme, "Copiar script".to_string()).clicked() {
            if let Some(id) = app.colab_panel.selected_job.clone() {
                match read_job_script(&id, 1_000_000) {
                    Ok(full) => ctx.copy_text(full),
                    Err(e) => {
                        app.colab_panel.import_note = format!("script ilegible: {e}");
                    }
                }
            }
        }
        let can_run = app.colab.phase == ColabPhase::Ready
            && app.colab.executor.is_some()
            && !app.colab.is_busy();
        ui.add_enabled_ui(can_run, |ui| {
            if primary_button(ui, theme, "Ejecutar en Colab").clicked() {
                run_selected_job(app);
                ctx.request_repaint();
            }
        });
    });
    if !app.colab.output.is_empty() {
        ui.add_space(SPACE_SM);
        ui.label(
            egui::RichText::new("Salida (recorte):")
                .strong()
                .size(TYPE_XS),
        );
        ui.add_space(SPACE_XS);
        egui::Frame::none()
            .fill(theme.panel_bg)
            .rounding(RADIUS_SM)
            .inner_margin(egui::Margin::symmetric(12.0, 8.0))
            .show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .max_height(110.0)
                    .show(ui, |ui| {
                        ui.label(
                            egui::RichText::new(app.colab.output.clone())
                                .monospace()
                                .size(TYPE_XS),
                        );
                    });
            });
        ui.add_space(SPACE_SM);
        if secondary_button(ui, theme, "Importar y verificar".to_string()).clicked() {
            import_last_output(app);
        }
    }
    if !app.colab_panel.import_note.is_empty() {
        ui.add_space(SPACE_XS);
        ui.label(egui::RichText::new(app.colab_panel.import_note.clone()).size(TYPE_XS));
    }
}

/// Ejecuta el script del job con el ejecutor elegido (vía worker).
fn run_selected_job(app: &mut GrafitoApp) {
    let (Some(job_id), Some(exec)) = (
        app.colab_panel.selected_job.clone(),
        app.colab.executor.clone(),
    ) else {
        return;
    };
    let script = match read_job_script(&job_id, 1_000_000) {
        Ok(s) => s,
        Err(e) => {
            app.colab.output = format!("error: {e}");
            return;
        }
    };
    let arg = app
        .colab
        .tools
        .iter()
        .find(|t| t.name == exec)
        .and_then(|t| pick_string_arg(&t.input_schema));
    let Some(key) = arg else {
        app.colab.output =
            format!("error: '{exec}' no tiene parámetro string; elegí otro ejecutor");
        return;
    };
    let mut args = serde_json::Map::new();
    args.insert(key, Value::String(script));
    app.colab.output.clear();
    app.colab_panel.invalidate_jobs();
    app.colab.request_run(exec, Value::Object(args));
}

/// Importa la última salida contra `grafito-mcp` instalado (sidecar por
/// stdio, mismo patrón que ffmpeg: binario en PATH o error honesto).
fn import_last_output(app: &mut GrafitoApp) {
    let Some(job_id) = app.colab_panel.selected_job.clone() else {
        return;
    };
    // La salida del notebook puede venir envuelta; busca el JSON con runs/results/verdict.
    let text = app.colab.output.clone();
    let parsed = extract_json(&text);
    let Some(result) = parsed else {
        app.colab_panel.import_note =
            "la salida no trae JSON (runs/results/verdict); nada que importar".into();
        return;
    };
    let bin = match colab_link::find_in_path("grafito-mcp") {
        Some(p) => p,
        None => {
            app.colab_panel.import_note =
                "grafito-mcp no está en el PATH (instalalo en /usr/bin)".into();
            return;
        }
    };
    let params = json!({"job_id": job_id, "result": result});
    match mcp_call_once(
        &bin,
        &[],
        "import_colab_result",
        params,
        Duration::from_secs(60),
    ) {
        Ok(value) => {
            let ok = value
                .get("structuredContent")
                .and_then(|s| s.get("ok"))
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let level = value
                .get("structuredContent")
                .and_then(|s| s.get("verification"))
                .and_then(Value::as_str)
                .unwrap_or("?");
            app.colab_panel.import_note =
                format!("import: ok={ok} verificación={level} (ver lab_colab.jsonl)");
            app.colab_panel.invalidate_jobs();
        }
        Err(e) => {
            app.colab_panel.import_note = format!("import falló: {e}");
        }
    }
}

/// Extrae el primer objeto JSON con pinta de resultado Colab.
fn extract_json(text: &str) -> Option<Value> {
    // Intento directo + búsqueda de {…} balanceado simple.
    if let Ok(v) = serde_json::from_str::<Value>(text.trim()) {
        if looks_like_result(&v) {
            return Some(v);
        }
    }
    let bytes = text.as_bytes();
    let mut start: Option<usize> = None;
    let mut depth = 0i32;
    let mut in_str = false;
    let mut esc = false;
    for (i, &b) in bytes.iter().enumerate() {
        if in_str {
            if esc {
                esc = false;
            } else if b == b'\\' {
                esc = true;
            } else if b == b'"' {
                in_str = false;
            }
            continue;
        }
        match b {
            b'"' => in_str = true,
            b'{' => {
                if depth == 0 {
                    start = Some(i);
                }
                depth += 1;
            }
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    if let Some(s) = start {
                        if let Ok(v) = serde_json::from_str::<Value>(&text[s..=i]) {
                            if looks_like_result(&v) {
                                return Some(v);
                            }
                        }
                    }
                    start = None;
                }
            }
            _ => {}
        }
    }
    None
}

fn looks_like_result(v: &Value) -> bool {
    v.get("runs").is_some() || v.get("results").is_some() || v.get("verdict").is_some()
}

fn draw_log(ui: &mut egui::Ui, app: &mut GrafitoApp, theme: &Theme) {
    egui::Frame::none()
        .fill(theme.panel_bg)
        .rounding(RADIUS_SM)
        .inner_margin(egui::Margin::symmetric(12.0, 8.0))
        .show(ui, |ui| {
            egui::ScrollArea::vertical()
                .max_height(90.0)
                .show(ui, |ui| {
                    for line in app.colab.log.iter() {
                        ui.label(egui::RichText::new(line).monospace().size(TYPE_XS));
                    }
                });
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jobs_cache_evita_fs_por_frame() {
        let hace_job = |n: usize| JobMeta {
            job_id: format!("{n:064x}"),
            kind: "integral".to_string(),
        };
        let mut estado = ColabPanelState::default();
        let mut llamadas = 0u32;
        let t0 = Instant::now();
        // Apertura: primera lectura va al loader.
        estado.refresh_jobs_with(t0, || {
            llamadas += 1;
            vec![hace_job(1)]
        });
        assert_eq!(llamadas, 1);
        assert_eq!(estado.cached_jobs().len(), 1);
        // Segundo frame dentro del TTL: NO toca el loader aunque haya job nuevo.
        estado.refresh_jobs_with(t0 + Duration::from_millis(100), || {
            llamadas += 1;
            vec![hace_job(1), hace_job(2)]
        });
        assert_eq!(llamadas, 1, "el segundo frame debe leer el cache, sin fs");
        assert_eq!(estado.cached_jobs().len(), 1);
        // Vencido el TTL: re-lee y ve el job nuevo.
        estado.refresh_jobs_with(t0 + JOBS_CACHE_TTL + Duration::from_millis(1), || {
            llamadas += 1;
            vec![hace_job(1), hace_job(2)]
        });
        assert_eq!(llamadas, 2);
        assert_eq!(estado.cached_jobs().len(), 2);
        // Invalidar (tras importar/ejecutar) fuerza re-lectura inmediata.
        estado.invalidate_jobs();
        estado.refresh_jobs_with(t0 + JOBS_CACHE_TTL + Duration::from_millis(1), || {
            llamadas += 1;
            vec![]
        });
        assert_eq!(llamadas, 3);
        assert!(estado.cached_jobs().is_empty());
    }

    #[test]
    fn extract_encuentra_json_embebido() {
        let v = extract_json("log...\n{\"runs\": [1]} ...fin").unwrap();
        assert_eq!(v["runs"], json!([1]));
        assert!(extract_json("sin json").is_none());
        assert!(extract_json("{\"otro\": 1}").is_none());
        let v2 = extract_json("{\"verdict\": true}").unwrap();
        assert!(looks_like_result(&v2));
    }
}
