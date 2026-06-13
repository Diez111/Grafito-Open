package ai.grafito.app.ui.mathkeyboard

import androidx.compose.foundation.layout.*
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Close
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun MathKeyboard(
    cmdInput: String,
    onCmdInputChange: (String) -> Unit,
    onInsert: (String) -> Unit,
    onSubmit: () -> Unit,
    onDelete: () -> Unit,
    onDismiss: () -> Unit,
    modifier: Modifier = Modifier,
) {
    var selectedTab by remember { mutableIntStateOf(0) }

    Surface(
        modifier = modifier.fillMaxWidth(),
        tonalElevation = 8.dp,
    ) {
        Column {
            // Display del texto actual + botón cerrar
            Row(
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(horizontal = 16.dp, vertical = 8.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Text(
                    text = if (cmdInput.isEmpty()) "…" else cmdInput,
                    style = MaterialTheme.typography.bodyLarge,
                    modifier = Modifier.weight(1f),
                    maxLines = 1,
                )
                IconButton(onClick = onDismiss) {
                    Icon(Icons.Default.Close, "Cerrar")
                }
            }

            // Tabs
            TabRow(selectedTabIndex = selectedTab) {
                Tab(selected = selectedTab == 0, onClick = { selectedTab = 0 }, text = { Text("123") })
                Tab(selected = selectedTab == 1, onClick = { selectedTab = 1 }, text = { Text("f(x)") })
                Tab(selected = selectedTab == 2, onClick = { selectedTab = 2 }, text = { Text("ABC") })
                Tab(selected = selectedTab == 3, onClick = { selectedTab = 3 }, text = { Text("3D") })
            }

            // Contenido de la tab con onDelete y onSubmit en todas
            when (selectedTab) {
                0 -> NumericTab(onInsert, onSubmit, onDelete)
                1 -> FunctionTab(onInsert, onSubmit, onDelete)
                2 -> AlphaTab(onInsert, onSubmit, onDelete)
                3 -> ThreeDTab(onInsert, onSubmit, onDelete)
            }
        }
    }
}
