# Grafito Android — Plan de UI/UX Nativa

## Decisiones finales

| Decisión | Valor |
|---|---|
| UI Framework | Jetpack Compose + Material Design 3 |
| Canvas | SurfaceView + wgpu (grafito-render, sin egui) |
| FFI | UniFFI proc-macros (bindings Kotlin auto-generados) |
| API Level | 30 (Android 11+) — Vulkan garantizado |
| Transición | Reemplazo completo (eliminar eframe/egui en Android) |
| Distribución | APK directo (todas las arquitecturas) |
| Persistencia | Auto-save en internal storage |
| Dispositivos | Teléfono (portrait) + Tablet (landscape) |
| Prioridad | Paridad completa con desktop |

---

## Arquitectura

```
┌─── Android (Kotlin) ──────────────────────────────┐
│                                                     │
│  Jetpack Compose (MD3)        SurfaceView           │
│  ├── PhoneLayout              └── wgpu render       │
│  │   ├── TopAppBar (2D/3D)         ├── 2D objects   │
│  │   ├── GrafitoCanvas             ├── 3D objects   │
│  │   ├── ToolBar (scroll)          ├── Grid + Axes  │
│  │   ├── AlgebraBottomSheet        └── Gestures     │
│  │   └── MathKeyboard (slide)                       │
│  │                                                  │
│  └── TabletLayout                                   │
│      ├── AlgebraPanel (side)                        │
│      ├── GrafitoCanvas (center)                     │
│      ├── PropertiesPanel (side)                     │
│      └── ToolBar (top)                              │
│                                                     │
│  GrafitoViewModel ── StateFlow<DocState> ── Compose │
│        │                                            │
│        └──── UniFFI (auto-generated Kotlin)         │
└──────────────────┬──────────────────────────────────┘
                   │ FFI boundary
┌──────────────────┴──────────────────────────────────┐
│  Rust: grafito-ffi (nuevo crate)                     │
│                                                      │
│  GrafitoEngine {                                     │
│    document: Arc<Mutex<Document>>                    │
│    camera: Arc<Mutex<Camera3D>>                      │
│    state: Arc<Mutex<AppState>>                       │
│  }                                                   │
│                                                      │
│  CanvasRenderer {                                    │
│    wgpu Instance/Device/Queue/Surface                │
│    grafito_render::Renderer                          │
│  }                                                   │
│                                                      │
│  DTO converters: GeoObject → ObjectDto (34 tipos)    │
│                                                      │
│  Depends on:                                         │
│    grafito-core (Document, GeoObject)                │
│    grafito-render (Renderer, wgpu pipeline)          │
│    grafito-geometry (CAS, stats, fractals)           │
└──────────────────────────────────────────────────────┘
```

---

## Fase 1: Fundación

**Objetivo:** Crate `grafito-ffi` + UniFFI + Compose scaffolding funcional.

### 1.1 Crear crate `grafito-ffi`

```
crates/grafito-ffi/
├── Cargo.toml          # uniffi 0.28, grafito-core, grafito-geometry, grafito-render
├── build.rs            # uniffi::build_scaffolding()
└── src/
    ├── lib.rs          # uniffi::setup_scaffolding!()
    ├── dto.rs          # ObjectDto, DocumentSnapshot, CommandResult, VariableDto, etc.
    ├── bridge.rs       # GrafitoEngine: new, get_snapshot, process_command, undo/redo
    ├── canvas.rs       # CanvasRenderer: from_native_window, resize, render_frame
    ├── converters.rs   # GeoObject → ObjectDto para 34 variantes
    └── persist.rs      # save_document / load_document (auto-save JSON)
```

**DTOs principales:**

```rust
#[derive(uniffi::Record)]
pub struct ObjectDto {
    pub id: String,
    pub label: String,
    pub object_type: String,       // "Point", "Circle", etc.
    pub visible: bool,
    pub color: ColorDto,
    pub properties: Vec<PropertyDto>,
    pub summary: String,           // "Circle: (1,2) r=3"
}

#[derive(uniffi::Record)]
pub struct DocumentSnapshot {
    pub objects: Vec<ObjectDto>,
    pub variables: Vec<VariableDto>,
    pub selected_id: Option<String>,
    pub view_mode: String,         // "2D" | "3D"
    pub undo_available: bool,
    pub redo_available: bool,
}

#[derive(uniffi::Record)]
pub struct CommandResult {
    pub success: bool,
    pub message: Option<String>,
    pub new_object_id: Option<String>,
}
```

**GrafitoEngine API (UniFFI):**

```rust
#[derive(uniffi::Object)]
pub struct GrafitoEngine { ... }

#[uniffi::export]
impl GrafitoEngine {
    #[uniffi::constructor]
    pub fn new(screen_width: f32, screen_height: f32) -> Self;

    // Document
    pub fn get_snapshot(&self) -> DocumentSnapshot;
    pub fn process_command(&self, input: String) -> CommandResult;
    pub fn delete_object(&self, id: String) -> bool;
    pub fn toggle_visibility(&self, id: String) -> bool;
    pub fn set_object_label(&self, id: String, label: String) -> bool;
    pub fn set_object_color(&self, id: String, r: f32, g: f32, b: f32) -> bool;
    pub fn set_variable(&self, name: String, value: f64);
    pub fn undo(&self) -> bool;
    pub fn redo(&self) -> bool;
    pub fn clear(&self);
    pub fn select_object(&self, id: Option<String>);
    pub fn pick_object_at(&self, screen_x: f32, screen_y: f32) -> Option<String>;

    // View
    pub fn set_view_mode(&self, mode: String);
    pub fn set_tool(&self, tool: ToolDto);
    pub fn get_tool(&self) -> ToolDto;
    pub fn set_dark_mode(&self, dark: bool);
    pub fn is_dark_mode(&self) -> bool;

    // Canvas
    pub fn canvas_pan(&self, dx: f32, dy: f32);
    pub fn canvas_zoom(&self, factor: f32, center_x: f32, center_y: f32);
    pub fn canvas_tap(&self, x: f32, y: f32) -> CommandResult;
    pub fn zoom_to_fit(&self);

    // 3D Camera
    pub fn camera_orbit(&self, delta_azimuth: f32, delta_elevation: f32);
    pub fn camera_dolly(&self, delta: f32);

    // CAS
    pub fn evaluate_cas(&self, command: String, args: Vec<String>) -> CasResult;

    // Spreadsheet
    pub fn get_spreadsheet(&self) -> SpreadsheetDto;
    pub fn set_cell(&self, row: u32, col: u32, value: String);

    // Command palette
    pub fn search_commands(&self, query: String) -> Vec<PaletteCommandDto>;

    // Export
    pub fn export_svg(&self) -> String;
    pub fn export_png_base64(&self, width: u32, height: u32) -> String;
    pub fn export_tikz(&self) -> String;

    // Persistence
    pub fn save_to_file(&self, path: String) -> bool;
    pub fn load_from_file(&self, path: String) -> bool;
}
```

### 1.2 Converters para 34 GeoObject → ObjectDto

Cada variante de `GeoObject` se convierte a un `ObjectDto` plano con:
- `object_type`: nombre del tipo como string
- `summary`: descripción legible ("Circle: center (1,2), r=3")
- `properties`: lista de PropertyDto con nombre/valor/editable

### 1.3 Build system

- Actualizar `build.gradle.kts`: Kotlin 2.0, Compose BOM, MD3, Hilt
- Custom Gradle task: `cargo ndk` + `uniffi-bindgen` → genera Kotlin bindings
- Mover `minSdk` a 30 en `build.gradle.kts`

### 1.4 Android scaffolding

- `MainActivity.kt`: ComponentActivity + `setContent { GrafitoApp() }`
- `GrafitoTheme.kt`: MD3 ColorScheme mapeado del tema Grafito (dark/light)
- `GrafitoViewModel.kt`: StateFlow<DocumentUiState> + llamadas FFI en Dispatchers.Default
- `GrafitoApplication.kt`: Hilt application class

**Entregable:** App Compose vacía que carga el .so, ejecuta un comando via UniFFI, y muestra el resultado en un Text composable.

---

## Fase 2: Canvas wgpu Nativo

**Objetivo:** SurfaceView con wgpu renderizando el canvas matemático.

### 2.1 CanvasRenderer en Rust

```rust
#[derive(uniffi::Object)]
pub struct CanvasRenderer { ... }

#[uniffi::export]
impl CanvasRenderer {
    #[uniffi::constructor]
    pub fn from_native_window(native_window_ptr: u64, width: u32, height: u32) -> Result<Self, String>;
    pub fn resize(&mut self, width: u32, height: u32);
    pub fn render_frame(&self, engine: &GrafitoEngine, dark_mode: bool) -> Result<(), String>;
}
```

- Backend: Vulkan (garantizado en API 30)
- Crear wgpu Surface desde `ANativeWindow` pointer
- `render_frame()` llama `Renderer::build_geometry()` / `build_3d_geometry()` y presenta

### 2.2 SurfaceView en Kotlin

```
bridge/
├── CanvasSurfaceView.kt    # SurfaceView + SurfaceHolder.Callback + Choreographer loop
├── NativeWindowHelper.kt   # JNI mínimo (~10 líneas) para obtener ANativeWindow ptr
└── GrafitoBridge.kt        # Lifecycle management del engine + renderer
```

- Render loop via `Choreographer.postFrameCallback` (vsync 60fps)
- `surfaceCreated` → crear CanvasRenderer
- `surfaceDestroyed` → destruir renderer (Document persiste en ViewModel)
- `surfaceChanged` → resize

### 2.3 Compose wrapper

```
canvas/
├── GrafitoCanvas.kt    # AndroidView wrapping SurfaceView
└── CanvasGestures.kt   # Gestos: tap, drag, pinch-to-zoom, fling
```

- `detectTapGestures`: tap → `canvas_tap()`, double-tap → zoom in
- `detectTransformGestures`: pan → `canvas_pan()`, pinch → `canvas_zoom()`
- 3D: drag → `camera_orbit()`, pinch → `camera_dolly()`

### 2.4 Auto-save

- `GrafitoEngine.save_to_file(path)` serializa Document a JSON
- `GrafitoEngine.load_from_file(path)` restaura desde JSON
- ViewModel llama `save_to_file()` en `onPause()` y `load_from_file()` en init
- Ruta: `context.filesDir/grafito_document.json`

**Entregable:** Canvas wgpu renderizando objetos 2D/3D en SurfaceView nativo con gestos funcionales y auto-save.

---

## Fase 3: UI Principal — Compose + MD3

**Objetivo:** Todos los paneles y controles de la app.

### 3.1 Layouts adaptativos

```kotlin
@Composable
fun GrafitoScaffold(viewModel: GrafitoViewModel) {
    val windowSizeClass = calculateWindowSizeClass()
    when (windowSizeClass.widthSizeClass) {
        WindowWidthSizeClass.Compact -> PhoneLayout(viewModel)
        WindowWidthSizeClass.Medium,
        WindowWidthSizeClass.Expanded -> TabletLayout(viewModel)
    }
}
```

**PhoneLayout (portrait, <600dp):**
```
┌────────────────────────┐
│ [2D|3D]     [🔍] [⋮]  │  TopAppBar
├────────────────────────┤
│                        │
│     Canvas (wgpu)      │  ~60% pantalla
│                        │
├────────────────────────┤
│ [↖][•][─][○][△][f(x)] │  ToolBar (scroll horizontal)
├────────────────────────┤
│  Algebra BottomSheet   │  Expandible: input + lista + props
│  ─── drag handle ───   │
│  > [input field]       │
│  A: Point (0,0)   👁 🗑│
│  c: Circle r=3    👁 🗑│
└────────────────────────┘
  Math Keyboard (slide-up bajo demanda)
```

**TabletLayout (landscape, ≥600dp):**
```
┌──────────────────────────────────────────┐
│ [2D|3D] [↖•─○△f] [🔍] [↩↪] [⋮]        │
├──────────┬─────────────────┬─────────────┤
│ Algebra  │                 │ Properties  │
│ [input]  │   Canvas        │ Type: Point │
│ A: Pt    │   (wgpu)        │ Pos: (0,0)  │
│ c: Cir   │                 │ Color: ■    │
│ f: fn    │                 │ ☑ Visible   │
├──────────┤                 ├─────────────┤
│Variables │                 │ CAS Result  │
│ a = 2.0  │                 │ ∫sin = -cos │
│ [slider] │                 │             │
└──────────┴─────────────────┴─────────────┘
│ [123] [f(x)] [ABC] [3D]   Math Keyboard  │
└──────────────────────────────────────────┘
```

### 3.2 Componentes

| Componente | Archivo | Descripción |
|---|---|---|
| AlgebraPanel | `algebra/AlgebraPanel.kt` | LazyColumn de ObjectItem + OutlinedTextField |
| ObjectItem | `algebra/ObjectItem.kt` | Row: color dot + label + summary + visibility + delete |
| CommandInput | `algebra/CommandInput.kt` | TextField con submit on Enter |
| ToolBar | `toolbar/ToolBar.kt` | LazyRow de ToolButton, separadores por categoría |
| ToolButton | `toolbar/ToolButton.kt` | FilterChip o IconButton con icono vectorial |
| PropertiesSheet | `properties/PropertiesSheet.kt` | ModalBottomSheet con propiedades del objeto |
| MeasurementCard | `properties/MeasurementCard.kt` | Card con medida (área, longitud, etc.) |
| CommandPalette | `commandpalette/CommandPaletteDialog.kt` | FullScreenDialog + SearchBar + LazyColumn |
| MathKeyboard | `mathkeyboard/MathKeyboard.kt` | TabRow(4) + grids de botones |
| NumericTab | `mathkeyboard/NumericTab.kt` | Grid: 0-9, operadores, π, e, √, ^ |
| FunctionTab | `mathkeyboard/FunctionTab.kt` | sin, cos, tan, log, ln, exp, abs |
| AlphaTab | `mathkeyboard/AlphaTab.kt` | a-z, subíndices |
| ThreeDTab | `mathkeyboard/ThreeDTab.kt` | x², y², z², Surface, Sphere, etc. |
| ColorPicker | `components/ColorPickerSheet.kt` | HSV picker + 5 favoritos + MD3 sliders |
| Spreadsheet | `spreadsheet/SpreadsheetScreen.kt` | LazyVerticalGrid editable |
| CasPanel | `cas/CasPanel.kt` | Input + resultado formateado |

### 3.3 ToolBar con iconos vectoriales

Cada herramienta tiene un icono dibujado con `Canvas` composable (equivalente a los painter API del desktop):
- Select: cursor arrow
- Point: dot
- Line: diagonal line
- Circle: circle
- Polygon: pentagon
- Function: wave (sin curve)
- 3D objects: wireframe cube, sphere

**Entregable:** UI completa funcional en teléfono y tablet con todos los paneles.

---

## Fase 4: Features Avanzadas

**Objetivo:** Paridad funcional completa con desktop.

| Feature | Implementación |
|---|---|
| Undo/Redo | IconButton en TopAppBar + shake-to-undo (SensorManager) |
| Variables con sliders | MD3 Slider conectado a `set_variable()` |
| Spreadsheet | LazyVerticalGrid con CellEditor dialog |
| CAS | Input con syntax highlighting + CasResult formateado |
| Export | SVG/PNG via `ACTION_SEND` share intent, TikZ via clipboard |
| Atractores/Fractales | LinearProgressIndicator durante computación (rayon background) |
| Dark/Light mode | `dynamicDarkColorScheme()` + toggle en TopAppBar |
| Animaciones | AnimatedVisibility para paneles, Crossfade para tabs |
| Accessibility | contentDescription en todos los botones, TalkBack |

**Entregable:** Paridad funcional completa.

---

## Fase 5: Polish y Performance

| Tarea | Descripción |
|---|---|
| Snapshot coalescing | Debounce `refreshSnapshot()` a max 30Hz para UI updates |
| Memory management | Verificar Arc<Mutex<Document>> lifecycle con Android |
| Rotation/multi-window | CanvasRenderer re-creatable, ViewModel sobrevive |
| Thread config | `rayon::num_threads(4)` para no saturar CPU móvil |
| APK size | `strip=true`, `lto="fat"`, evaluar quitar x86/x86_64 |
| Testing | Unit tests para converters, integration tests para bridge |
| Edge cases | Low memory, background/foreground, split-screen |

---

## Estructura de archivos Kotlin

```
android/app/src/main/java/ai/grafito/app/
├── MainActivity.kt
├── GrafitoApplication.kt
├── bridge/
│   ├── GrafitoBridge.kt
│   ├── CanvasSurfaceView.kt
│   └── NativeWindowHelper.kt
├── viewmodel/
│   ├── GrafitoViewModel.kt
│   └── DocumentUiState.kt
├── ui/
│   ├── theme/
│   │   └── GrafitoTheme.kt
│   ├── screens/
│   │   ├── PhoneLayout.kt
│   │   └── TabletLayout.kt
│   ├── canvas/
│   │   ├── GrafitoCanvas.kt
│   │   └── CanvasGestures.kt
│   ├── algebra/
│   │   ├── AlgebraPanel.kt
│   │   ├── ObjectItem.kt
│   │   └── CommandInput.kt
│   ├── toolbar/
│   │   ├── ToolBar.kt
│   │   └── ToolButton.kt
│   ├── properties/
│   │   ├── PropertiesSheet.kt
│   │   └── MeasurementCard.kt
│   ├── mathkeyboard/
│   │   ├── MathKeyboard.kt
│   │   ├── NumericTab.kt
│   │   ├── FunctionTab.kt
│   │   ├── AlphaTab.kt
│   │   └── ThreeDTab.kt
│   ├── commandpalette/
│   │   └── CommandPaletteDialog.kt
│   ├── spreadsheet/
│   │   ├── SpreadsheetScreen.kt
│   │   └── CellEditor.kt
│   ├── cas/
│   │   └── CasPanel.kt
│   └── components/
│       ├── ColorPickerSheet.kt
│       └── ToastSnackbar.kt
└── di/
    └── GrafitoModule.kt
```

## Estructura de archivos Rust (nuevo crate)

```
crates/grafito-ffi/
├── Cargo.toml
├── build.rs
└── src/
    ├── lib.rs
    ├── dto.rs
    ├── bridge.rs
    ├── canvas.rs
    ├── converters.rs
    └── persist.rs
```
