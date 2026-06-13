package ai.grafito.app.viewmodel

import androidx.compose.ui.graphics.Color
import uniffi.grafito_ffi.ToolDto

// ── Documento ─────────────────────────────────────────────────────

data class DocumentUiState(
    val objects: List<ObjectUiItem> = emptyList(),
    val variables: List<VariableUiItem> = emptyList(),
    val selectedId: String? = null,
    val viewMode: String = "2D",
    val undoAvailable: Boolean = false,
    val redoAvailable: Boolean = false,
)

data class ObjectUiItem(
    val id: String,
    val label: String,
    val type: String,
    val visible: Boolean,
    val color: Color,
    val summary: String,
    val properties: List<PropertyUiItem> = emptyList(),
)

data class VariableUiItem(
    val name: String,
    val value: Double,
    val min: Double,
    val max: Double,
)

data class PropertyUiItem(
    val name: String,
    val value: String,
    val editable: Boolean,
)

// ── Canvas / UI ───────────────────────────────────────────────────

data class CanvasUiState(
    val currentTool: ToolDto = ToolDto.SELECT,
    val darkMode: Boolean = true,
    val viewMode: String = "2D",
)

// ── Búsqueda ──────────────────────────────────────────────────────

data class CommandPaletteItem(
    val name: String,
    val category: String,
    val syntaxHint: String,
)

// ── Spreadsheet ───────────────────────────────────────────────────

data class SpreadsheetUiState(
    val rows: Int = 10,
    val cols: Int = 10,
    val cells: Map<Pair<Int, Int>, CellUiItem> = emptyMap(),
)

data class CellUiItem(
    val row: Int,
    val col: Int,
    val value: String,
    val evaluated: Double? = null,
)
