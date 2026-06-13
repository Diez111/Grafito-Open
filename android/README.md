# Grafito Android - Jetpack Compose + Material Design 3

Port nativo de Grafito para Android usando Jetpack Compose y Material Design 3, manteniendo toda la lógica matemática en Rust.

## Arquitectura

```
┌─────────────────────────────────────────────────────────────┐
│                    Android (Kotlin)                          │
│                                                              │
│  ┌──────────────┐  ┌──────────────┐  ┌────────────────────┐ │
│  │  Compose UI  │  │ GrafitoCanvas│  │  GrafitoViewModel  │ │
│  │  (MD3)       │  │ (SurfaceView │  │  (StateFlow<Doc>)  │ │
│  │  - Algebra   │  │  + wgpu)     │  │                    │ │
│  │  - Toolbar   │  │              │  │  - documentState   │ │
│  │  - CmdPal.   │  │              │  │  - selectedObj     │ │
│  │  - MathKbd   │  │              │  │  - viewMode        │ │
│  │  - Spreadsheet│ │              │  │  - toolState       │ │
│  └──────┬───────┘  └──────┬───────┘  └────────┬───────────┘ │
│         │                 │                    │              │
│         └─────────────────┼────────────────────┘              │
│                           │                                   │
│                  ┌────────┴────────┐                          │
│                  │ GrafitoBridge   │ (JNI wrapper)            │
│                  └────────┬────────┘                          │
└───────────────────────────┼───────────────────────────────────┘
                            │ JNI boundary
┌───────────────────────────┼───────────────────────────────────┐
│                    Rust   │                                    │
│                  ┌────────┴────────┐                          │
│                  │  grafito-ffi    │ (nuevo crate)            │
│                  │                 │                          │
│                  │  GrafitoEngine  │ ← Arc<Mutex<Document>>   │
│                  │  CanvasRenderer │ ← Renderer + wgpu::Device│
│                  │  CommandProc    │ ← process_input()        │
│                  │  DtoConverters  │ ← GeoObject → ObjectDto  │
│                  └──┬──────┬────┬──┘                          │
│                     │      │    │                              │
│              ┌──────┘  ┌───┘    └──────┐                      │
│              ▼         ▼               ▼                      │
│         grafito-core  grafito-render  grafito-geometry        │
└───────────────────────────────────────────────────────────────┘
```

## Estructura de Archivos

### Rust (crate `grafito-ffi`)
```
crates/grafito-ffi/
├── Cargo.toml
└── src/
    ├── lib.rs              # UniFFI scaffolding
    ├── dto.rs              # Data Transfer Objects (ObjectDto, CommandResult, etc.)
    ├── bridge.rs           # GrafitoEngine - API principal expuesta via JNI
    ├── canvas.rs           # CanvasRenderer - renderizado wgpu a ANativeWindow
    ├── converters.rs       # Conversión GeoObject → ObjectDto (34 tipos)
    ├── persist.rs          # Auto-save/load del documento
    └── command_processor.rs # Procesamiento de comandos matemáticos
```

### Android (Kotlin + Compose)
```
android/app/src/main/java/ai/grafito/app/
├── MainActivity.kt             # Entry point con Compose
├── GrafitoApplication.kt       # Hilt Application
│
├── bridge/
│   ├── GrafitoBridge.kt        # Wrapper JNI
│   ├── CanvasSurfaceView.kt    # SurfaceView para wgpu
│   └── NativeWindowHelper.kt   # Helper para ANativeWindow
│
├── viewmodel/
│   ├── GrafitoViewModel.kt     # ViewModel principal
│   └── DocumentUiState.kt      # Data classes de estado
│
├── ui/
│   ├── theme/
│   │   └── GrafitoTheme.kt     # Tema Material 3 (light/dark)
│   │
│   ├── screens/
│   │   ├── PhoneLayout.kt      # Layout adaptativo para phones (<600dp)
│   │   └── TabletLayout.kt     # Layout adaptativo para tablets (≥600dp)
│   │
│   ├── canvas/
│   │   └── GrafitoCanvas.kt    # Composable wrapper para SurfaceView
│   │
│   ├── algebra/
│   │   ├── AlgebraPanel.kt     # Panel de álgebra con lista de objetos
│   │   ├── ObjectItem.kt       # Item individual de objeto
│   │   └── CommandInput.kt     # Input para comandos
│   │
│   ├── toolbar/
│   │   ├── ToolBar.kt          # Barra de herramientas
│   │   └── ToolButton.kt       # Botón de herramienta individual
│   │
│   ├── properties/
│   │   ├── PropertiesSheet.kt  # BottomSheet de propiedades
│   │   └── MeasurementCard.kt  # Card de mediciones
│   │
│   ├── mathkeyboard/
│   │   ├── MathKeyboard.kt     # Teclado matemático principal
│   │   ├── NumericTab.kt       # Tab numérico (123)
│   │   ├── FunctionTab.kt      # Tab de funciones (f(x))
│   │   ├── AlphaTab.kt         # Tab alfabético (ABC)
│   │   └── ThreeDTab.kt        # Tab 3D
│   │
│   ├── commandpalette/
│   │   └── CommandPaletteDialog.kt  # Paleta de comandos (Ctrl+K style)
│   │
│   ├── spreadsheet/
│   │   └── SpreadsheetView.kt  # Vista de hoja de cálculo
│   │
│   └── components/
│       ├── ColorPickerDialog.kt # Selector de color
│       └── GrafitoSnackbar.kt   # Notificaciones toast
│
└── di/
    └── AppModule.kt            # Módulo Hilt para DI
```

## Features Implementadas

### ✅ Core
- **34 tipos de objetos geométricos**: Point, Line, Circle, Polygon, Function, Ellipse, Parabola, Hyperbola, Point3D, Sphere3D, Cube3D, etc.
- **Sistema de comandos**: `Circle[(1,2), 3]`, `Function[sin(x)]`, `Sphere3D[(0,0,0), 2]`
- **Undo/Redo**: Stack de hasta 50 estados
- **Auto-save**: Persistencia automática del documento

### ✅ UI/UX
- **Material Design 3**: Tema completo con colores dinámicos
- **Layout adaptativo**: 
  - **Phone (<600dp)**: Canvas 60% + Algebra 40% con bottom navigation
  - **Tablet (≥600dp)**: 3 paneles (Algebra 25% | Canvas 50% | Properties 25%)
- **Teclado matemático**: 4 tabs (123, f(x), ABC, 3D)
- **Paleta de comandos**: Búsqueda rápida estilo Ctrl+K
- **Dark/Light mode**: Toggle en toolbar

### ✅ Canvas
- **Renderizado wgpu**: GPU-accelerated via Vulkan
- **Gestos nativos**: 
  - Tap para seleccionar/crear
  - Pinch para zoom
  - Drag para pan
  - Double-tap para reset
- **Grid y axes**: Renderizado automático

### ✅ Paneles
- **Algebra**: Lista de objetos con visibilidad, eliminación, selección
- **Properties**: Propiedades del objeto seleccionado (coordenadas, dimensiones, color)
- **Variables**: Sliders para variables globales (a, b, c...)
- **Spreadsheet**: Hoja de cálculo con celdas A1-Z50

## Build Instructions

### Prerequisites
- Android Studio Hedgehog o superior
- Android SDK 34 (API 34)
- NDK 25.x o superior
- Rust con targets Android:
  ```bash
  rustup target add aarch64-linux-android armv7-linux-androideabi x86_64-linux-android
  ```

### Compilar Rust para Android
```bash
cd crates/grafito-ffi
cargo build --target aarch64-linux-android --release
```

### Generar bindings JNI (próximamente)
```bash
# Esto generará los headers C y el código Kotlin automáticamente
cargo run --bin uniffi-bindgen generate \
  --library target/aarch64-linux-android/release/libgrafito_ffi.so \
  --language kotlin \
  --out-dir android/app/src/main/java/ai/grafito/app/bindings
```

### Compilar APK
```bash
cd android
./gradlew assembleDebug
```

## Configuración

### `build.gradle` (app)
```gradle
android {
    compileSdk 34
    
    defaultConfig {
        minSdk 30
        targetSdk 34
    }
    
    buildFeatures {
        compose true
    }
}

dependencies {
    implementation 'androidx.compose.material3:material3:1.2.0'
    implementation 'com.google.dagger:hilt-android:2.50'
    kapt 'com.google.dagger:hilt-compiler:2.50'
}
```

### `AndroidManifest.xml`
```xml
<application
    android:name=".GrafitoApplication"
    android:label="Grafito">
    <activity android:name=".MainActivity">
        <intent-filter>
            <action android:name="android.intent.action.MAIN" />
            <category android:name="android.intent.category.LAUNCHER" />
        </intent-filter>
    </activity>
</application>
```

## Próximos Pasos

1. **Integración JNI**: Conectar GrafitoBridge.kt con las funciones nativas de Rust
2. **Testing**: Unit tests para GrafitoEngine y CanvasRenderer
3. **Performance**: Optimizar render loop para 60fps
4. **Features avanzadas**:
   - Export a PNG/SVG/PDF
   - Compartir documentos
   - Cloud sync
   - Colaboración en tiempo real

## Licencia

MIT - Ver LICENSE.md
