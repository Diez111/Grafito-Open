//! Ventana "Colab Pro": pareo en un clic + jobs pesados del lab.
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
use grafito_ui::tokens::{SPACE_SM, SPACE_XS, TYPE_LG, TYPE_SM, TYPE_XS};
use serde_json::{json, Value};
use std::time::Duration;

/// Job seleccionado en el panel (no persiste entre sesiones a propósito).
#[derive(Debug, Default)]
pub struct ColabPanelState {
    pub selected_job: Option<String>,
    pub script_preview: String,
    pub import_note: String,
}

/// Dibuja la ventana Colab. Llamar una vez por frame cuando visible.
pub fn draw_colab_window(app: &mut GrafitoApp, ctx: &egui::Context) {
    let locale = app.config_locale();
    // Drena eventos del worker antes de dibujar (cambia fase/tools/log).
    let changed = app.colab.poll();
    if changed || app.colab.is_busy() {
        ctx.request_repaint();
    }
    let mut open = app.show_colab_window;
    egui::Window::new(t("colab.title", locale))
        .open(&mut open)
        .default_width(480.0)
        .show(ctx, |ui| {
            draw_status(ui, app, ctx);
            ui.add_space(SPACE_SM);
            ui.separator();
            draw_tools(ui, app);
            ui.add_space(SPACE_SM);
            ui.separator();
            draw_jobs(ui, app, ctx);
            ui.add_space(SPACE_SM);
            ui.separator();
            draw_log(ui, app);
        });
    app.show_colab_window = open;
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

fn draw_status(ui: &mut egui::Ui, app: &mut GrafitoApp, ctx: &egui::Context) {
    let locale = app.config_locale();
    let theme = current_theme(ctx);
    ui.horizontal(|ui| {
        status_dot(ui, app.colab.phase, theme);
        ui.label(
            egui::RichText::new(app.colab.status.clone())
                .strong()
                .size(TYPE_SM),
        );
    });
    ui.label(
        egui::RichText::new(t("colab.connect_hint", locale))
            .size(TYPE_XS)
            .color(theme.text_secondary),
    );
    ui.add_space(SPACE_XS);
    ui.horizontal(|ui| {
        let busy = app.colab.is_busy();
        if app.colab.phase == ColabPhase::Ready {
            if ui.button("Desconectar").clicked() {
                app.colab.request_disconnect();
            }
        } else {
            let label = if app.colab.phase == ColabPhase::Failed {
                t("colab.retry", locale)
            } else {
                t("colab.connect", locale)
            };
            if ui.add_enabled(!busy, egui::Button::new(label)).clicked() {
                app.colab.output.clear();
                app.colab_panel.import_note.clear();
                app.colab.request_connect();
                ctx.request_repaint();
            }
        }
        if busy {
            ui.spinner();
        }
    });
}

fn draw_tools(ui: &mut egui::Ui, app: &mut GrafitoApp) {
    let theme = current_theme(ui.ctx());
    ui.label(
        egui::RichText::new("Notebook")
            .strong()
            .size(TYPE_SM)
            .color(theme.accent),
    );
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

fn draw_jobs(ui: &mut egui::Ui, app: &mut GrafitoApp, ctx: &egui::Context) {
    let theme = current_theme(ctx);
    ui.label(
        egui::RichText::new("Jobs del lab")
            .strong()
            .size(TYPE_SM)
            .color(theme.accent),
    );
    ui.label(
        egui::RichText::new(format!("carpeta: {}", lab_jobs_dir().display()))
            .size(TYPE_XS)
            .color(theme.text_secondary),
    );
    let jobs: Vec<JobMeta> = list_lab_jobs();
    if jobs.is_empty() {
        ui.label(
            egui::RichText::new("Sin jobs: el agente los crea con export_colab_job.")
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
                let short = format!("{}… · {}", &job.job_id[..12], job.kind);
                if ui.radio(selected, short).clicked() {
                    app.colab_panel.selected_job = Some(job.job_id.clone());
                    app.colab_panel.script_preview =
                        read_job_script(&job.job_id, 1200).unwrap_or_default();
                    app.colab_panel.import_note.clear();
                }
            }
        });
    if app.colab_panel.selected_job.is_none() {
        return;
    }
    ui.add_space(SPACE_XS);
    egui::ScrollArea::vertical()
        .max_height(120.0)
        .show(ui, |ui| {
            ui.label(
                egui::RichText::new(app.colab_panel.script_preview.clone())
                    .monospace()
                    .size(TYPE_XS),
            );
        });
    ui.horizontal(|ui| {
        if ui.button("Copiar script").clicked() {
            if let Some(id) = app.colab_panel.selected_job.clone() {
                let full = read_job_script(&id, 1_000_000).unwrap_or_default();
                ctx.copy_text(full);
            }
        }
        let can_run = app.colab.phase == ColabPhase::Ready
            && app.colab.executor.is_some()
            && !app.colab.is_busy();
        if ui
            .add_enabled(can_run, egui::Button::new("Ejecutar en Colab"))
            .clicked()
        {
            run_selected_job(app);
            ctx.request_repaint();
        }
    });
    if !app.colab.output.is_empty() {
        ui.add_space(SPACE_XS);
        ui.label(
            egui::RichText::new("Salida (recorte):")
                .strong()
                .size(TYPE_XS),
        );
        egui::ScrollArea::vertical()
            .max_height(120.0)
            .show(ui, |ui| {
                ui.label(
                    egui::RichText::new(app.colab.output.clone())
                        .monospace()
                        .size(TYPE_XS),
                );
            });
        ui.horizontal(|ui| {
            if ui.button("Importar y verificar").clicked() {
                import_last_output(app);
            }
        });
    }
    if !app.colab_panel.import_note.is_empty() {
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

fn draw_log(ui: &mut egui::Ui, app: &mut GrafitoApp) {
    let theme = current_theme(ui.ctx());
    ui.label(
        egui::RichText::new("Registro")
            .strong()
            .size(TYPE_SM)
            .color(theme.accent),
    );
    egui::ScrollArea::vertical()
        .max_height(90.0)
        .show(ui, |ui| {
            for line in app.colab.log.iter() {
                ui.label(egui::RichText::new(line).monospace().size(TYPE_XS));
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;

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
