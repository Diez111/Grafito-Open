package ai.grafito.app.viewmodel

import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.graphics.Color
import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import ai.grafito.app.bridge.UniFFIBridge
import uniffi.grafito_ffi.*
import dagger.hilt.android.lifecycle.HiltViewModel
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import javax.inject.Inject

@HiltViewModel
class GrafitoViewModel @Inject constructor(
    private val bridge: UniFFIBridge,
) : ViewModel() {

    var documentState by mutableStateOf(DocumentUiState())
        private set

    var canvasState by mutableStateOf(CanvasUiState(darkMode = false))
        private set

    var toastMessage by mutableStateOf<String?>(null)
        private set

    fun clearToast() {
        toastMessage = null
    }

    init { refreshSnapshot() }

    fun initDarkMode(dark: Boolean) {
        canvasState = canvasState.copy(darkMode = dark)
        bridge.setDarkMode(dark)
    }

    fun getEngine(): GrafitoEngine = bridge.engine

    fun processCommand(input: String) {
        viewModelScope.launch(Dispatchers.Default) {
            val result = bridge.processCommand(input)
            if (!result.success && result.message != null) {
                toastMessage = result.message
            }
            refreshSnapshot()
        }
    }

    fun selectObject(id: String?) {
        viewModelScope.launch(Dispatchers.Default) {
            bridge.selectObject(id)
            refreshSnapshot()
        }
    }

    fun deleteObject(id: String) {
        viewModelScope.launch(Dispatchers.Default) {
            bridge.deleteObject(id)
            refreshSnapshot()
        }
    }

    fun toggleVisibility(id: String) {
        viewModelScope.launch(Dispatchers.Default) {
            bridge.toggleVisibility(id)
            refreshSnapshot()
        }
    }

    fun canvasTap(x: Float, y: Float) {
        viewModelScope.launch(Dispatchers.Default) {
            bridge.canvasTap(x, y)
            refreshSnapshot()
        }
    }

    fun canvasPan(dx: Float, dy: Float) {
        viewModelScope.launch(Dispatchers.Default) {
            bridge.canvasPan(dx, dy)
            refreshSnapshot()
        }
    }

    fun canvasZoom(factor: Float, centerX: Float, centerY: Float) {
        viewModelScope.launch(Dispatchers.Default) {
            bridge.canvasZoom(factor, centerX, centerY)
            refreshSnapshot()
        }
    }

    fun setTool(tool: ToolDto) {
        canvasState = canvasState.copy(currentTool = tool)
        viewModelScope.launch(Dispatchers.Default) { bridge.setTool(tool) }
    }

    fun setViewMode(mode: String) {
        viewModelScope.launch(Dispatchers.Default) {
            bridge.setViewMode(mode)
            refreshSnapshot()
        }
    }

    fun toggleDarkMode() {
        canvasState = canvasState.copy(darkMode = !canvasState.darkMode)
        viewModelScope.launch(Dispatchers.Default) {
            bridge.setDarkMode(canvasState.darkMode)
        }
    }

    fun undo() {
        viewModelScope.launch(Dispatchers.Default) {
            bridge.undo()
            refreshSnapshot()
        }
    }

    fun redo() {
        viewModelScope.launch(Dispatchers.Default) {
            bridge.redo()
            refreshSnapshot()
        }
    }

    fun clearAll() {
        viewModelScope.launch(Dispatchers.Default) {
            bridge.clear()
            refreshSnapshot()
        }
    }

    fun setVariable(name: String, value: Double) {
        viewModelScope.launch(Dispatchers.Default) {
            bridge.setVariable(name, value)
            refreshSnapshot()
        }
    }

    fun searchCommands(query: String): List<CommandPaletteItem> {
        return bridge.searchCommands(query).map {
            CommandPaletteItem(it.name, it.category, it.syntaxHint)
        }
    }

    private fun refreshSnapshot() {
        val snap = bridge.getSnapshot()
        documentState = DocumentUiState(
            objects = snap.objects.map { it.toUi() },
            variables = snap.variables.map { it.toUi() },
            selectedId = snap.selectedId,
            viewMode = snap.viewMode,
            undoAvailable = snap.undoAvailable,
            redoAvailable = snap.redoAvailable,
        )
        canvasState = canvasState.copy(
            darkMode = bridge.isDarkMode(),
            viewMode = snap.viewMode,
        )
    }

    override fun onCleared() {
        super.onCleared()
        bridge.dispose()
    }
}

private fun ObjectDto.toUi() = ObjectUiItem(
    id = id, label = label, type = objectType, visible = visible,
    color = Color(color.r, color.g, color.b, color.a), summary = summary,
    properties = properties.map { PropertyUiItem(it.name, it.value, it.editable) },
)

private fun VariableDto.toUi() = VariableUiItem(
    name = name, value = value, min = min, max = max,
)
