# Plan: Corrección de Bugs Críticos en UI Android

## Diagnóstico

Después de analizar el código y los reportes del usuario, identifiqué 6 problemas críticos:

### Problema 1: Barras nativas no respetadas
**Causa raíz**: El `BottomSheetScaffold` aplica `innerPadding` que incluye el TopAppBar, pero el `ToolBar` alineado al fondo con `Modifier.align(Alignment.BottomCenter)` se superpone con la navigation bar del sistema porque no tiene `navigationBarsPadding()`.

**Solución**: Agregar `Modifier.navigationBarsPadding()` al ToolBar para que respete la barra de navegación del sistema.

### Problema 2: Menú lateral sin expresiones algebraicas
**Causa raíz**: El drawer en `PhoneLayout.kt` solo tiene opciones estáticas ("Modo Claro" y "Buscar Comando"). No muestra los objetos del documento (puntos, líneas, círculos, etc.).

**Solución**: Agregar una lista dinámica de objetos del documento en el drawer, leyendo de `viewModel.documentState.objects`. Cada objeto debe mostrar su tipo, etiqueta y permitir selección/eliminación.

### Problema 3: Comandos no se renderizan
**Causa raíz**: Múltiples problemas potenciales:
1. El `CanvasSurfaceView` llama a `engine.updateScreenSize()` pero el `ViewTransform` del documento puede no estar actualizándose correctamente
2. El render loop puede estar corriendo pero `build_geometry()` puede estar devolviendo vectores vacíos
3. Los vértices pueden estar generándose en coordenadas incorrectas (ej: todos en 0,0)

**Solución**: 
1. Agregar logs en `CanvasRenderer.render_frame()` para verificar que se están generando vértices
2. Verificar que `Document.view().screen_size` se actualice correctamente
3. Agregar logs en `build_geometry()` para ver cuántos vértices genera

### Problema 4: Gestos no funcionan (pan/zoom)
**Causa raíz**: 
1. El `CanvasSurfaceView.onTouchEvent()` retorna `handled = true` cuando procesa un gesto, pero si retorna `false` el evento se propaga al padre
2. El `BottomSheetScaffold` puede estar interceptando eventos de scroll/pan para el BottomSheet
3. El `ToolBar` y `FAB` están superpuestos sobre el canvas y pueden estar bloqueando gestos en esas zonas

**Solución**:
1. Hacer que `onTouchEvent()` SIEMPRE retorne `true` para consumir todos los eventos táctiles en el canvas
2. Mover el ToolBar fuera del área del canvas (debajo del canvas, no superpuesto)
3. Verificar que los callbacks `onPanCallback` y `onZoomCallback` se estén invocando

### Problema 5: Modo 3D nunca renderiza
**Causa raíz**: El `CanvasRenderer` en Rust puede estar llamando siempre a `renderer.build_geometry()` (2D) en lugar de verificar el `viewMode` y llamar a `renderer.build_3d_geometry()` cuando esté en modo 3D.

**Solución**: 
1. Verificar que `CanvasRenderer.render_frame()` en `canvas.rs` verifique `engine.get_view_mode()`
2. Llamar a `build_3d_geometry()` cuando `viewMode == "3D"`
3. Asegurar que la matriz MVP para 3D se calcule correctamente

### Problema 6: Modo claro roto
**Causa raíz**: 
1. El `MainActivity.kt` puede estar forzando `darkTheme = true` hardcodeado
2. O el `GrafitoTheme` puede no estar leyendo `viewModel.canvasState.darkMode` correctamente

**Solución**:
1. Verificar que `MainActivity.kt` use `val darkTheme = viewModel.canvasState.darkMode` sin hardcodear
2. Verificar que `GrafitoTheme` reciba el parámetro `darkTheme` correctamente
3. Verificar que el toggle en el drawer invoque `viewModel.toggleDarkMode()`

---

## Plan de Implementación

### Fase 1: Corregir insets del sistema (30 min)
**Archivos a modificar**: `PhoneLayout.kt`, `TabletLayout.kt`

1. Agregar `Modifier.navigationBarsPadding()` al `ToolBar` en ambos layouts
2. Verificar que el `TopAppBar` respete la status bar (debería hacerlo automáticamente)
3. Probar en dispositivo para confirmar que el contenido no se superpone con las barras

### Fase 2: Corregir gestos táctiles (45 min)
**Archivos a modificar**: `CanvasSurfaceView.kt`, `PhoneLayout.kt`, `TabletLayout.kt`

1. Hacer que `onTouchEvent()` SIEMPRE retorne `true` para consumir todos los eventos
2. Mover el `ToolBar` fuera del área del canvas:
   - Cambiar el layout de `Box` (superposición) a `Column` (apilamiento vertical)
   - El canvas ocupa el peso restante, el ToolBar está debajo con altura fija
3. Mover el FAB fuera del ToolBar (arriba del ToolBar, alineado a la derecha)
4. Agregar logs en `onPanCallback` y `onZoomCallback` para verificar que se invoquen

### Fase 3: Verificar y corregir renderizado (60 min)
**Archivos a modificar**: `canvas.rs`, `bridge.rs`

1. Agregar logs en `CanvasRenderer.render_frame()`:
   - Log del tamaño de `vertices` y `indices` después de `build_geometry()`
   - Log de `viewMode` para verificar que se esté leyendo correctamente
   - Log de `screen_size` para verificar que se actualice
2. Verificar que `Document.view().screen_size` se actualice en `update_screen_size()`
3. Si los vértices están vacíos, agregar logs en `build_geometry()` para ver por qué
4. Si los vértices están en coordenadas incorrectas, verificar la transformación mundo→pantalla

### Fase 4: Corregir modo 3D (45 min)
**Archivos a modificar**: `canvas.rs`

1. Verificar que `render_frame()` verifique `engine.get_view_mode()`
2. Llamar a `renderer.build_3d_geometry()` cuando `viewMode == "3D"`
3. Calcular la matriz MVP para 3D:
   - `projection = camera.projection_matrix()`
   - `view = camera.view_matrix()`
   - `mvp = projection * view`
4. Agregar logs para verificar que se esté llamando a `build_3d_geometry()`

### Fase 5: Agregar lista de objetos al drawer (30 min)
**Archivos a modificar**: `PhoneLayout.kt`, `TabletLayout.kt`

1. Agregar un `LazyColumn` en el drawer que muestre `viewModel.documentState.objects`
2. Cada item debe mostrar:
   - Icono del tipo de objeto (punto, línea, círculo, etc.)
   - Etiqueta del objeto (ej: "A", "B", "C1")
   - Botón de visibilidad (ojo)
   - Botón de eliminación (papelera)
3. Al hacer clic en un objeto, seleccionarlo con `viewModel.selectObject(id)`
4. Agregar un botón "Limpiar todo" al final de la lista

### Fase 6: Corregir modo claro (30 min)
**Archivos a modificar**: `MainActivity.kt`, `GrafitoTheme.kt`

1. Verificar que `MainActivity.kt` use:
   ```kotlin
   val darkTheme = viewModel.canvasState.darkMode
   GrafitoTheme(darkTheme = darkTheme) { ... }
   ```
2. Verificar que `GrafitoTheme` reciba el parámetro correctamente
3. Verificar que el toggle en el drawer invoque `viewModel.toggleDarkMode()`
4. Probar el toggle y verificar que el tema cambie

---

## Orden de Implementación

1. **Fase 2: Gestos táctiles** (prioridad alta - sin gestos la app es inútil)
2. **Fase 1: Insets del sistema** (prioridad alta - problema visual crítico)
3. **Fase 3: Renderizado** (prioridad alta - sin renderizado no hay app)
4. **Fase 4: Modo 3D** (prioridad media - funcionalidad importante)
5. **Fase 6: Modo claro** (prioridad media - funcionalidad UI)
6. **Fase 5: Lista de objetos** (prioridad baja - mejora de UX)

**Tiempo total estimado**: 3-4 horas

---

## Verificación

Después de cada fase:
1. Compilar el proyecto: `cd android && ./gradlew assembleDebug`
2. Instalar la APK: `adb install -r app/build/outputs/apk/debug/app-debug.apk`
3. Probar la funcionalidad en el dispositivo
4. Verificar logs: `adb logcat | grep Grafito`

---

## Notas Técnicas

### Sobre SurfaceView y gestos
`SurfaceView` crea una ventana separada que se dibuja detrás de la ventana principal de la app. Los eventos táctiles llegan primero a la ventana principal (Compose) y solo se propagan al SurfaceView si no son consumidos. Por eso es crítico que `onTouchEvent()` retorne `true` para consumir todos los eventos.

### Sobre BottomSheetScaffold y gestos
`BottomSheetScaffold` intercepta eventos de scroll vertical para controlar el BottomSheet. Si el canvas está dentro del scaffold, los eventos de pan vertical pueden ser interceptados. La solución es hacer que el canvas consuma todos los eventos táctiles antes de que lleguen al scaffold.

### Sobre renderizado con wgpu
El render loop corre en un hilo separado a 60fps. Cada frame:
1. Adquiere la siguiente textura del surface
2. Llama a `build_geometry()` o `build_3d_geometry()` para generar vértices
3. Crea un command encoder
4. Inicia un render pass con la textura como target
5. Dibuja los vértices con el pipeline adecuado
6. Envía el command buffer a la GPU
7. Presenta la textura en el surface

Si `build_geometry()` devuelve vectores vacíos, no se dibuja nada (solo el clear color de fondo).
