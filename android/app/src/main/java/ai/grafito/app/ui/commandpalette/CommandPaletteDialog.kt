package ai.grafito.app.ui.commandpalette

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Close
import androidx.compose.material.icons.filled.Search
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import ai.grafito.app.viewmodel.CommandPaletteItem
import ai.grafito.app.viewmodel.GrafitoViewModel

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun CommandPaletteDialog(
    viewModel: GrafitoViewModel,
    onSelect: (String) -> Unit,
    onDismiss: () -> Unit,
    modifier: Modifier = Modifier,
) {
    var query by remember { mutableStateOf("") }
    var results by remember { mutableStateOf<List<CommandPaletteItem>>(emptyList()) }

    LaunchedEffect(query) {
        results = if (query.isNotBlank()) viewModel.searchCommands(query) else emptyList()
    }

    Surface(
        modifier = modifier.fillMaxSize(),
        color = MaterialTheme.colorScheme.surface,
    ) {
        Column {
            Row(
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(16.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                OutlinedTextField(
                    value = query,
                    onValueChange = { query = it },
                    modifier = Modifier.weight(1f),
                    placeholder = { Text("Search commands…") },
                    leadingIcon = { Icon(Icons.Default.Search, "Search") },
                    singleLine = true,
                )
                Spacer(Modifier.width(8.dp))
                IconButton(onClick = onDismiss) {
                    Icon(Icons.Default.Close, "Close")
                }
            }

            LazyColumn(
                modifier = Modifier.fillMaxSize(),
                verticalArrangement = Arrangement.spacedBy(4.dp),
            ) {
                if (query.isBlank()) {
                    item {
                        Text(
                            "Ejemplos",
                            style = MaterialTheme.typography.titleMedium,
                            color = MaterialTheme.colorScheme.primary,
                            modifier = Modifier.padding(16.dp)
                        )
                    }
                    val examples = if (viewModel.documentState.viewMode == "3D") {
                        listOf(
                            CommandPaletteItem("Atractor de Lorenz", "3D", "lorenz(10, 28, 8/3)"),
                            CommandPaletteItem("Cubo 3D", "3D", "cube(0,0,0, 2)"),
                            CommandPaletteItem("Curva Paramétrica", "3D", "curve(sin(t), cos(t), t, 0, 10)")
                        )
                    } else {
                        listOf(
                            CommandPaletteItem("Seno", "2D", "f(x) = sin(x)"),
                            CommandPaletteItem("Derivada", "2D", "derivative(x^2 * sin(x))"),
                            CommandPaletteItem("Integral", "2D", "integral(x^2, 0, 1)"),
                            CommandPaletteItem("Fourier", "2D", "fourier(sin(x) + cos(2x))")
                        )
                    }
                    items(examples) { item ->
                        CommandItem(
                            item = item,
                            onClick = {
                                onSelect(item.syntaxHint)
                                onDismiss()
                            },
                        )
                    }
                } else {
                    items(results) { item ->
                        CommandItem(
                            item = item,
                            onClick = {
                                onSelect(item.syntaxHint)
                                onDismiss()
                            },
                        )
                    }
                }
            }
        }
    }
}

@Composable
private fun CommandItem(
    item: CommandPaletteItem,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
) {
    Card(
        modifier = modifier
            .fillMaxWidth()
            .padding(horizontal = 16.dp)
            .clickable(onClick = onClick),
    ) {
        Column(modifier = Modifier.padding(16.dp)) {
            Row(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.SpaceBetween,
            ) {
                Text(item.name, style = MaterialTheme.typography.titleSmall)
                Text(
                    item.category,
                    style = MaterialTheme.typography.labelSmall,
                    color = MaterialTheme.colorScheme.primary,
                )
            }
            Spacer(Modifier.height(4.dp))
            Text(
                item.syntaxHint,
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
    }
}
