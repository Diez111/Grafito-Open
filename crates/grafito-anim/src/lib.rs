#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]
//! Puente de Grafito hacia motores de animación externos.
//!
//! El motor corre fuera del proceso Rust (p. ej. Python + Manim) y habla un
//! protocolo JSON versionado sobre stdio. Este crate gestiona el ciclo de vida
//! del worker, los jobs y los presupuestos, sin dependencias de egui ni de red.
//! Incluye generador universal estilo canal de YouTube: cualquier texto produce
//! una animación profesional en <2s con fallback garantizado.

pub mod engine;
pub mod guion;
pub mod parametric;
pub mod player;
pub mod protocol;
pub mod scene;

pub use engine::{
    playlist_desde_escena, run_job, AnimEngine, EngineConfig, JobEvent, CANCEL_GRACE,
    DEFAULT_IDLE_TIMEOUT_SECS, DEFAULT_JOB_TIMEOUT_SECS, DEFAULT_LINE_CAP_BYTES,
    MAX_IDLE_TIMEOUT_SECS, MAX_JOB_TIMEOUT_SECS, MAX_LINE_CAP_BYTES, MIN_IDLE_TIMEOUT_SECS,
    MIN_JOB_TIMEOUT_SECS, MIN_LINE_CAP_BYTES,
};
pub use parametric::{
    infer_parametric_anim, parametric_hint, suaviza_bezier, taylor_orden_para_anim,
    taylor_spec_para_anim, FrameCount, ParamName, ParametricAnim, ParametricError, ParametricKind,
    PolylineMorph, ShapeEasing, TaylorSpec, PARAMETRIC_DEFAULT_FRAMES, PARAMETRIC_EVAL_MAX_DEPTH,
    PARAMETRIC_MAX_BYTES, PARAMETRIC_MAX_EXPR_CHARS, PARAMETRIC_MAX_FRAMES,
    TAYLOR_ANIM_ORDEN_DEFAULT, TAYLOR_ANIM_ORDEN_MAX, TAYLOR_ANIM_ORDEN_MIN,
};
pub use player::{
    centroide_de, CreateAnim, FadeAnim, GrowFromCenterAnim, IndicateAnim, PlacedMobject, PlayItem,
    PlayedFrame, ScenePlayer, TrackerMap, TransformMatchingShapes, UpdateFromTracker, VMobject,
    ValueTracker, WaitAnim, WriteAnim, PLAYER_MAX_FRAMES, PLAYER_MAX_TOTAL_FRAMES,
};
pub use protocol::{
    downcast, estimate_chunk_bytes, frames_for_duration, indice_vecino_mas_cercano_con_tope, kinds,
    localize_worker_error, max_chunk_frames, normalize_concept, request_for_concept,
    sanitize_error_code, sanitize_template, taylor_anim_order, taylor_anim_order_from_params,
    template_for_concept, truncate_worker_message, AnimDuration, AnimJobId, AnimParams,
    AnimRequest, AnimResult, AnimationGroup, ExportFormat, PlanRemuestreo, PngDir, RenderProgress,
    Resolution, WireMessage, WorkerError, LONGFORM_CHUNK_MAX_BYTES, MAX_ERROR_CODE_LEN,
    MAX_MEDIA_PATH_CHARS, MAX_TIMELINE_DURATION_MS, MAX_WORKER_MESSAGE_LEN,
    PREVIEW_SHORT_MAX_FRAMES, TAYLOR_ANIM_ORDER_DEFAULT, TAYLOR_ANIM_ORDER_MAX,
    TAYLOR_ANIM_ORDER_MIN, VIDEO_LONGFORM_MAX_FRAMES,
};
pub use scene::{
    bezier_suaviza, frame_at_global, matching_shapes_frames, resample_arclen, sample_playlist,
    schedule_playlist, taylor_anim_order_from_params as scene_taylor_order, Animation, Camera,
    Clock, Mobject, MovingCamera, Ortho, PathFunc, PropertyTrack, RateFunc, Scene, SceneError,
    TrackKey, TransformAnim, MANIM_EXP_HALF_LIFE, MANIM_NOT_QUITE_PROPORTION, MANIM_PAUSE_RATIO,
    MANIM_RUNNING_PULL, MAX_GROUP_CHILDREN, MAX_GROUP_DEPTH, MAX_MOBJECT_POINTS, MAX_SCENE_LAYERS,
    MAX_TEX_SVG_BYTES, MAX_TRACK_DURATION_MS, MAX_TRACK_KEYS, SCENE_MORPH_MAX_SAMPLES,
};
