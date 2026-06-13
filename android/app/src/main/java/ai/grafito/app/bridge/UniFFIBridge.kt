package ai.grafito.app.bridge

import uniffi.grafito_ffi.*

/**
 * Wrapper limpio sobre los bindings UniFFI.
 * Expone tanto el GrafitoEngine como el CanvasRenderer.
 */
class UniFFIBridge(
    screenWidth: Float = 1080f,
    screenHeight: Float = 1920f,
) {
    val engine: GrafitoEngine = GrafitoEngine(screenWidth, screenHeight)

    /** Crea un CanvasRenderer vinculado a este engine */
    fun createCanvasRenderer(): CanvasRenderer =
        engine.createCanvasRenderer()

    // ── Comandos ──────────────────────────────────────────────────

    fun processCommand(input: String): CommandResult =
        engine.processCommand(input)

    fun getSnapshot(): DocumentSnapshot =
        engine.getSnapshot()

    fun selectObject(id: String?) =
        engine.selectObject(id)

    fun toggleVisibility(id: String): Boolean =
        engine.toggleVisibility(id)

    fun deleteObject(id: String): Boolean =
        engine.deleteObject(id)

    // ── Canvas ────────────────────────────────────────────────────

    fun canvasTap(x: Float, y: Float): CommandResult =
        engine.canvasTap(x, y)

    fun canvasPan(dx: Float, dy: Float) =
        engine.canvasPan(dx, dy)

    fun canvasZoom(factor: Float, centerX: Float, centerY: Float) =
        engine.canvasZoom(factor, centerX, centerY)

    // ── Herramientas y UI ─────────────────────────────────────────

    fun setTool(tool: ToolDto) = engine.setTool(tool)
    fun getTool(): ToolDto = engine.getTool()
    fun setViewMode(mode: String) = engine.setViewMode(mode)
    fun setDarkMode(dark: Boolean) = engine.setDarkMode(dark)
    fun isDarkMode(): Boolean = engine.isDarkMode()

    // ── Undo / Redo ───────────────────────────────────────────────

    fun undo(): Boolean = engine.undo()
    fun redo(): Boolean = engine.redo()
    fun clear() = engine.clear()

    // ── Variables + Spreadsheet ───────────────────────────────────

    fun setVariable(name: String, value: Double) =
        engine.setVariable(name, value)

    fun getSpreadsheet(): SpreadsheetDto = engine.getSpreadsheet()
    fun setCell(row: UInt, col: UInt, value: String) =
        engine.setCell(row, col, value)

    // ── Búsqueda ──────────────────────────────────────────────────

    fun searchCommands(query: String): List<PaletteCommandDto> =
        engine.searchCommands(query)

    // ── Archivos ──────────────────────────────────────────────────

    fun saveToFile(path: String): Boolean = engine.saveToFile(path)
    fun loadFromFile(path: String): Boolean = engine.loadFromFile(path)

    // ── Cleanup ───────────────────────────────────────────────────

    fun dispose() = engine.close()
}
