package ai.grafito.app.ui.properties

import androidx.compose.foundation.layout.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import ai.grafito.app.viewmodel.GrafitoViewModel
import ai.grafito.app.viewmodel.PropertyUiItem

@Composable
fun PropertiesSheet(
    viewModel: GrafitoViewModel,
    modifier: Modifier = Modifier,
) {
    val selected = viewModel.documentState.objects
        .find { it.id == viewModel.documentState.selectedId }

    if (selected == null) {
        Box(modifier = modifier.fillMaxSize().padding(16.dp)) {
            Text(
                text = "No object selected",
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
        return
    }

    Column(
        modifier = modifier
            .fillMaxSize()
            .padding(16.dp),
    ) {
        Text(selected.type, style = MaterialTheme.typography.titleMedium)
        Spacer(Modifier.height(8.dp))
        Text(selected.label, style = MaterialTheme.typography.bodyLarge)
        Spacer(Modifier.height(16.dp))

        selected.properties.forEach { property ->
            PropertyRow(property)
        }

        if (selected.summary.isNotBlank()) {
            Spacer(Modifier.height(16.dp))
            Card(modifier = Modifier.fillMaxWidth()) {
                Text(
                    selected.summary,
                    modifier = Modifier.padding(12.dp),
                    style = MaterialTheme.typography.bodySmall,
                )
            }
        }
    }
}

@Composable
private fun PropertyRow(property: PropertyUiItem) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .padding(vertical = 4.dp),
        horizontalArrangement = Arrangement.SpaceBetween,
    ) {
        Text(
            property.name,
            style = MaterialTheme.typography.bodyMedium,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
        Text(property.value, style = MaterialTheme.typography.bodyMedium)
    }
}
