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
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.unit.dp
import ai.grafito.app.ui.algebra.AlgebraPanel
import ai.grafito.app.ui.canvas.GrafitoCanvas
import ai.grafito.app.ui.commandpalette.CommandPaletteDialog
import ai.grafito.app.ui.components.GrafitoSnackbar
import ai.grafito.app.ui.mathkeyboard.MathKeyboard
import ai.grafito.app.ui.toolbar.ToolBar
import ai.grafito.app.viewmodel.GrafitoViewModel
import kotlinx.coroutines.launch

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun PhoneLayout(viewModel: GrafitoViewModel) {
    var showCmdPalette by remember { mutableStateOf(false) }
    var showMathKb by remember { mutableStateOf(false) }
    var cmdInput by remember { mutableStateOf("") }
    val snackbarHostState = remember { SnackbarHostState() }
    val scaffoldState = rememberBottomSheetScaffoldState(
        bottomSheetState = rememberStandardBottomSheetState(
            initialValue = SheetValue.PartiallyExpanded,
            skipHiddenState = true
        )
    )

    LaunchedEffect(viewModel.toastMessage) {
        viewModel.toastMessage?.let { msg ->
            snackbarHostState.showSnackbar(msg, withDismissAction = true, duration = SnackbarDuration.Short)
            viewModel.clearToast()
        }
    }

        BottomSheetScaffold(
        scaffoldState = scaffoldState,
        sheetContent = {
            val navPadding = WindowInsets.navigationBars.asPaddingValues().calculateBottomPadding()
            Column(
                modifier = Modifier
                    .fillMaxWidth()
                    .fillMaxHeight(0.45f)
                    .padding(bottom = navPadding)
            ) {
                // Peek Content: Toolbar + Cmd
                Row(
                    modifier = Modifier.fillMaxWidth().padding(horizontal = 4.dp, vertical = 0.dp),
                    verticalAlignment = Alignment.CenterVertically
                ) {
                    ToolBar(
                        currentTool = viewModel.canvasState.currentTool,
                        darkMode = viewModel.canvasState.darkMode,
                        viewMode = viewModel.documentState.viewMode,
                        onToolSelected = { viewModel.setTool(it) },
                        onClear = { viewModel.clearAll() },
                        modifier = Modifier.weight(1f)
                    )
                }
                
                // Expanded Content: Algebra Panel
                AlgebraPanel(
                    viewModel = viewModel,
                    cmdInput = cmdInput,
                    onCmdInputChange = { cmdInput = it },
                    showMathKb = showMathKb,
                    onToggleMathKb = { showMathKb = !showMathKb },
                    modifier = Modifier.weight(1f)
                )
            }
        },
        sheetPeekHeight = 130.dp + WindowInsets.navigationBars.asPaddingValues().calculateBottomPadding(),
        sheetShape = RoundedCornerShape(topStart = 24.dp, topEnd = 24.dp),
        sheetContainerColor = MaterialTheme.colorScheme.surface,
        sheetShadowElevation = 16.dp,
        snackbarHost = { GrafitoSnackbar(message = null, snackbarHostState = snackbarHostState) },
    ) { innerPadding ->
        Box(modifier = Modifier.fillMaxSize()) {
            // Full screen canvas
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

            // Keyboard overlay at the bottom
            AnimatedVisibility(
                visible = showMathKb,
                modifier = Modifier.align(Alignment.BottomCenter).navigationBarsPadding().imePadding()
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
