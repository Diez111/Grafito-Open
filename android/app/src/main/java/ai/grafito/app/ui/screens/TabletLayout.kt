package ai.grafito.app.ui.screens

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import ai.grafito.app.ui.algebra.AlgebraPanel
import ai.grafito.app.ui.canvas.GrafitoCanvas
import ai.grafito.app.ui.commandpalette.CommandPaletteDialog
import ai.grafito.app.ui.components.GrafitoSnackbar
import ai.grafito.app.ui.mathkeyboard.MathKeyboard
import ai.grafito.app.ui.properties.PropertiesSheet
import ai.grafito.app.ui.toolbar.ToolBar
import ai.grafito.app.viewmodel.GrafitoViewModel

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun TabletLayout(viewModel: GrafitoViewModel) {
    var showCmdPalette by remember { mutableStateOf(false) }
    var showMathKb by remember { mutableStateOf(false) }
    var cmdInput by remember { mutableStateOf("") }
    val snackbarHostState = remember { SnackbarHostState() }

    LaunchedEffect(viewModel.toastMessage) {
        viewModel.toastMessage?.let { msg ->
            snackbarHostState.showSnackbar(msg, withDismissAction = true, duration = SnackbarDuration.Short)
            viewModel.clearToast()
        }
    }

    Scaffold(
        snackbarHost = { GrafitoSnackbar(message = null, snackbarHostState = snackbarHostState) },
    ) { padding ->
        Box(Modifier.fillMaxSize().padding(padding)) {
            Row(Modifier.fillMaxSize()) {
                Surface(Modifier.fillMaxHeight().weight(0.25f), tonalElevation = 1.dp) {
                    PropertiesSheet(viewModel)
                }
                Box(Modifier.fillMaxHeight().weight(0.5f)) {
                    GrafitoCanvas(viewModel, Modifier.fillMaxSize())
                    
                    // Floating Top Bar Overlay
                    Row(
                        modifier = Modifier
                            .fillMaxWidth()
                            .statusBarsPadding()
                            .padding(top = 16.dp, start = 16.dp, end = 16.dp),
                        horizontalArrangement = Arrangement.SpaceBetween,
                        verticalAlignment = Alignment.CenterVertically
                    ) {
                        Surface(
                            shape = RoundedCornerShape(50),
                            color = MaterialTheme.colorScheme.surface.copy(alpha = 0.8f),
                            tonalElevation = 4.dp
                        ) {
                            Row(verticalAlignment = Alignment.CenterVertically, modifier = Modifier.padding(horizontal = 4.dp)) {
                                IconButton(onClick = { viewModel.toggleDarkMode() }) {
                                    Icon(if (viewModel.canvasState.darkMode) Icons.Default.LightMode else Icons.Default.DarkMode, "Tema")
                                }
                                IconButton(onClick = { showCmdPalette = true }) {
                                    Icon(Icons.Default.Search, "Buscar")
                                }
                            }
                        }

                        Surface(
                            shape = RoundedCornerShape(50),
                            color = MaterialTheme.colorScheme.surface.copy(alpha = 0.8f),
                            tonalElevation = 4.dp
                        ) {
                            Row(verticalAlignment = Alignment.CenterVertically, modifier = Modifier.padding(horizontal = 4.dp)) {
                                TextButton(onClick = {
                                    viewModel.setViewMode(if (viewModel.documentState.viewMode == "2D") "3D" else "2D")
                                }) {
                                    Text(viewModel.documentState.viewMode, style = MaterialTheme.typography.labelLarge, color = MaterialTheme.colorScheme.primary)
                                }
                                IconButton(onClick = { viewModel.undo() }, enabled = viewModel.documentState.undoAvailable) {
                                    Icon(Icons.Default.Undo, "Deshacer")
                                }
                                IconButton(onClick = { viewModel.redo() }, enabled = viewModel.documentState.redoAvailable) {
                                    Icon(Icons.Default.Redo, "Rehacer")
                                }
                            }
                        }
                    }

                    // Toolbar at bottom of Canvas
                    ToolBar(
                        currentTool = viewModel.canvasState.currentTool,
                        darkMode = viewModel.canvasState.darkMode,
                        viewMode = viewModel.documentState.viewMode,
                        onToolSelected = { viewModel.setTool(it) },
                        onClear = { viewModel.clearAll() },
                        modifier = Modifier.align(Alignment.BottomCenter).fillMaxWidth().navigationBarsPadding()
                    )
                }
                Surface(Modifier.fillMaxHeight().weight(0.25f), tonalElevation = 1.dp) {
                    AlgebraPanel(
                        viewModel = viewModel,
                        cmdInput = cmdInput,
                        onCmdInputChange = { cmdInput = it }
                    )
                }
            }

            // Keyboard overlay (center or right)
            AnimatedVisibility(
                visible = showMathKb,
                modifier = Modifier.align(Alignment.BottomCenter).navigationBarsPadding()
            ) {
                MathKeyboard(
                    cmdInput = cmdInput,
                    onCmdInputChange = { cmdInput = it },
                    onInsert = { cmdInput += it },
                    onSubmit = {
                        viewModel.processCommand(cmdInput)
                        cmdInput = ""
                        showMathKb = false
                    },
                    onDelete = { if (cmdInput.isNotEmpty()) cmdInput = cmdInput.dropLast(1) },
                    onDismiss = { showMathKb = false }
                )
            }


        }
    }

    if (showCmdPalette) {
        CommandPaletteDialog(
            viewModel = viewModel,
            onSelect = { cmdInput = it },
            onDismiss = { showCmdPalette = false },
        )
    }
}
