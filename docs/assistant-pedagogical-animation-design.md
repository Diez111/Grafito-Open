# Diseño: Asistente Pedagógico Completo + Motor de Animaciones Profesional

> **Objetivo:** Convertir al asistente de Grafito en tutor pedagógico completo (socrático, currículum, ejercicios, feedback) y al generador de animaciones en motor profesional callable e integrado.
> **Stack:** Rust 2021, egui 0.29, wgpu, Muse Spark 1.2, Manim, crates puros.

## 1. Diagnóstico actual

### Asistente (grafito-assistant, grafito-app/src/assistant.rs)
- Hace: resuelve local determinista + transporte remoto OpenAI-compat (OpenCodeGo / DeepSeek / Ollama), explica, deriva, propone ProposedPlan con catálogo acotado, valida, aplica.
- No hace: pedagogía. Sin niveles (primaria/secundaria/uni), sin scaffold socrático, sin currículum (UTN), sin generación/evaluación de ejercicios, sin feedback formativo, sin integración con animaciones.
- Budget: 32k chars, 24 turnos (recién ampliado) — bien para sesiones largas.

### Animaciones (grafito-anim, grafito-app/src/anim_native.rs)
- Hace: grafito-anim puente JSON v1 sobre stdio a worker Python+Manim, anim_native 48 frames de parábola+tangente.
- No hace: no es callable profesional. Sin trait unificado, sin Tool en catálogo, sin validación, sin UI progreso/preview/export integrada, sin GeoObject::Animation.

## 2. Principios (rust-design / rust-ui / best-practices)

- Un crate = una responsabilidad: grafito-pedagogy (nuevo, puro), grafito-anim (existente), grafito-assistant (orquestación), grafito-app (shell egui), grafito-core (modelo: GeoObject::Animation)
- DAG: pedagogy hoja, anim hoja, assistant consume ambos, app consume todos, core no depende de UI
- Traits por comportamiento: PedagogicalStrategy::scaffold, AnimEngine::render
- Errores tipados: PedagogyError, AnimError con thiserror
- Newtype: PedagogicalLevel, AnimDuration, Resolution
- Ownership: Documento dueño de AnimationObj, cachés en AnimEngine, jobs por canal
- egui: estado en GrafitoApp, AnimPreviewState, background threads, nunca bloquear UI

## 3. Arquitectura

### 3.1 Crate grafito-pedagogy (puro)

```rust
pub enum PedagogicalLevel { Primary, Secondary, University, UTN(UTNProgram) }
pub struct Scaffold { pub question: String, pub hint: Option<String>, pub explanation: String }
pub trait PedagogicalStrategy: Send + Sync {
    fn scaffold(&self, concept: &str, level: PedagogicalLevel, history: &[Turn]) -> Scaffold;
    fn generate_exercise(&self, lo: &LearningObjective, level: PedagogicalLevel) -> Exercise;
    fn assess(&self, exercise: &Exercise, answer: &str) -> Feedback;
}
```

### 3.2 Motor animaciones profesional

```rust
pub struct AnimParams {
    pub template: String, pub concept: String, pub params: BTreeMap<String, f64>,
    pub duration: AnimDuration, pub resolution: Resolution, pub export: ExportFormat,
}
pub enum AnimError { EmptyConcept, NonFiniteParam{key:String,value:f64}, InvalidResolution{w:u32,h:u32}, Timeout(Duration), Engine(String) }
pub trait AnimEngine: Send + Sync { fn render(&self, params: AnimParams) -> Result<AnimJob, AnimError>; }
```

- Budgets: line_cap 64k, job_timeout 90s, max_params 32, max_concept 512
- NativeEngine (rust 48 frames) y ManimEngine (python stdio) implementan AnimEngine
- GeoObject::Animation(AnimationObj { id, label, media_path, frames, duration, template })

### 3.3 Tool GenerateAnimation

Catalogo: GenerateAnimation[template: "derivative-slope" | "integral-area" | "taylor-series" | "conformal-map", concept: "derivada", params: {x0:1.0}]

- LLM Muse Spark emite ```grafito-anim { "template": "...", "concept": "...", "params": {...} }``` validado localmente
- Usuario [Ver] -> AnimEngine::render en background, progreso, luego AnimationObj en documento

### 3.4 UI egui

- Panel animaciones: grafito-app/src/anim_ui.rs con AnimPreviewState en GrafitoApp
- Flujo: card Ver/Descartar -> thread + ctx.request_repaint() por JobEvent::Progress -> ProgressBar + preview -> Result -> toast + export
- Background: mpsc + thread como en assistant remote_job

## 4. Plan (geometry -> core -> command -> UI -> tests)

1. Crate grafito-pedagogy + tests
2. Anim trait + GeoObject::Animation
3. Tool GenerateAnimation en catalogo
4. UI anim_ui.rs + background jobs
5. Integracion pedagogica en build_remote_request
6. Tests + cargo fmt/clippy/test + packaging

