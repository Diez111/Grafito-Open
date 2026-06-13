package ai.grafito.app.ui.algebra

import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.clickable
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Delete
import androidx.compose.material.icons.filled.Visibility
import androidx.compose.material.icons.filled.VisibilityOff
import androidx.compose.material.icons.filled.Close
import androidx.compose.material.icons.filled.Keyboard
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import androidx.compose.ui.focus.onFocusChanged
import ai.grafito.app.viewmodel.GrafitoViewModel
import ai.grafito.app.viewmodel.ObjectUiItem

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun AlgebraPanel(
    viewModel: GrafitoViewModel,
    cmdInput: String,
    onCmdInputChange: (String) -> Unit,
    showMathKb: Boolean = false,
    onToggleMathKb: () -> Unit = {},
    modifier: Modifier = Modifier,
) {
    Column(
        modifier = modifier
            .fillMaxSize()
            .padding(8.dp),
    ) {
        // Sleek Input field
        TextField(
            value = cmdInput,
            onValueChange = onCmdInputChange,
            modifier = Modifier
                .fillMaxWidth()
                .padding(horizontal = 4.dp)
                .onFocusChanged { state ->
                    if (state.isFocused && !showMathKb) {
                        onToggleMathKb()
                    }
                },
            placeholder = { Text("Comando o Expresión...") },
            singleLine = true,
            shape = androidx.compose.foundation.shape.RoundedCornerShape(24.dp),
            colors = TextFieldDefaults.colors(
                focusedIndicatorColor = androidx.compose.ui.graphics.Color.Transparent,
                unfocusedIndicatorColor = androidx.compose.ui.graphics.Color.Transparent,
                disabledIndicatorColor = androidx.compose.ui.graphics.Color.Transparent,
                focusedContainerColor = MaterialTheme.colorScheme.surfaceVariant.copy(alpha = 0.5f),
                unfocusedContainerColor = MaterialTheme.colorScheme.surfaceVariant.copy(alpha = 0.5f)
            ),
            trailingIcon = {
                IconButton(onClick = {
                    if (cmdInput.isNotBlank()) {
                        viewModel.processCommand(cmdInput)
                        onCmdInputChange("")
                    }
                }) {
                    Text("↵", style = MaterialTheme.typography.titleMedium, color = MaterialTheme.colorScheme.primary)
                }
            },
        )

        Spacer(Modifier.height(12.dp))

        // Object list filtered by view mode
        val is3DType = { type: String ->
            type.contains("3D", ignoreCase = true) || 
            listOf("Sphere", "Cube", "Cylinder", "Cone", "Pyramid", "Torus", "Moebius", "Surface", "Attractor").any { type.contains(it, ignoreCase = true) }
        }
        val filteredObjects = viewModel.documentState.objects.filter { obj ->
            val is3D = is3DType(obj.type)
            if (viewModel.documentState.viewMode == "3D") is3D else !is3D
        }

        LazyColumn(
            modifier = Modifier.weight(1f),
            verticalArrangement = Arrangement.spacedBy(4.dp),
        ) {
            if (filteredObjects.isEmpty() && cmdInput.isBlank()) {
                item {
                    Column(
                        modifier = Modifier.fillMaxWidth().padding(16.dp),
                        horizontalAlignment = Alignment.CenterHorizontally
                    ) {
                        Text(
                            text = "Ejemplos para Probar",
                            style = MaterialTheme.typography.titleMedium,
                            color = MaterialTheme.colorScheme.primary,
                            modifier = Modifier.padding(bottom = 12.dp)
                        )
                        val examples = if (viewModel.documentState.viewMode == "3D") {
                            listOf(
                                "Atractor de Lorenz" to "Lorenz[10, 28, 8/3]",
                                "Cubo 3D" to "Cube3D[(0,0,0), 2]",
                                "Curva Paramétrica" to "ParametricCurve3D[sin(t), cos(t), t, 0, 10]"
                            )
                        } else {
                            listOf(
                                "Seno" to "f(x) = sin(x)",
                                "Derivada" to "Derivative[x^2 * sin(x), x]",
                                "Integral" to "Integral[x^2, x, 0, 1]",
                                "Fourier (Taylor)" to "Taylor[sin(x) + cos(2*x), x, 0, 5]"
                            )
                        }
                        examples.forEach { (name, cmd) ->
                            Card(
                                onClick = { viewModel.processCommand(cmd) },
                                modifier = Modifier.fillMaxWidth().padding(vertical = 4.dp),
                                colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surfaceVariant.copy(alpha = 0.5f))
                            ) {
                                Column(modifier = Modifier.padding(12.dp).fillMaxWidth(), horizontalAlignment = Alignment.CenterHorizontally) {
                                    Text(name, style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.primary)
                                    Text(cmd, style = MaterialTheme.typography.bodySmall)
                                }
                            }
                        }
                    }
                }
            } else {
                items(filteredObjects, key = { it.id }) { obj ->
                    ObjectItem(
                        obj = obj,
                        isSelected = obj.id == viewModel.documentState.selectedId,
                        onClick = { viewModel.selectObject(obj.id) },
                        onToggleVisibility = { viewModel.toggleVisibility(obj.id) },
                        onDelete = { viewModel.deleteObject(obj.id) },
                    )
                }
            }
        }

        // Variables
        if (viewModel.documentState.variables.isNotEmpty()) {
            Spacer(Modifier.height(12.dp))
            Text(
                text = "Variables",
                style = MaterialTheme.typography.titleSmall,
                modifier = Modifier.padding(bottom = 8.dp),
            )

            viewModel.documentState.variables.forEach { v ->
                VariableSlider(
                    name = v.name,
                    value = v.value,
                    min = v.min,
                    max = v.max,
                    onValueChange = { viewModel.setVariable(v.name, it) },
                    modifier = Modifier.fillMaxWidth(),
                )
            }
        }
    }
}

@Composable
fun ObjectItem(
    obj: ObjectUiItem,
    isSelected: Boolean,
    onClick: () -> Unit,
    onToggleVisibility: () -> Unit,
    onDelete: () -> Unit,
) {
    Card(
        modifier = Modifier.fillMaxWidth(),
        colors = CardDefaults.cardColors(
            containerColor = if (isSelected) MaterialTheme.colorScheme.primaryContainer
            else MaterialTheme.colorScheme.surface,
        ),
    ) {
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .clickable { onClick() }
                .padding(horizontal = 12.dp, vertical = 8.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            IconButton(onClick = onToggleVisibility) {
                Icon(
                    if (obj.visible) Icons.Default.Visibility else Icons.Default.VisibilityOff,
                    obj.label,
                )
            }
            Column(modifier = Modifier.weight(1f)) {
                Text(obj.label, style = MaterialTheme.typography.bodyMedium)
                Text(obj.type, style = MaterialTheme.typography.labelSmall)
            }
            IconButton(onClick = onDelete) {
                Icon(Icons.Default.Delete, "Delete", tint = MaterialTheme.colorScheme.error)
            }
        }
    }
}

@Composable
private fun VariableSlider(
    name: String,
    value: Double,
    min: Double,
    max: Double,
    onValueChange: (Double) -> Unit,
    modifier: Modifier = Modifier,
) {
    var sliderValue by remember(value) { mutableFloatStateOf(value.toFloat()) }

    Column(modifier = modifier.padding(vertical = 4.dp)) {
        Row(
            modifier = Modifier.fillMaxWidth(),
            horizontalArrangement = Arrangement.SpaceBetween,
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text(name, style = MaterialTheme.typography.bodyMedium)
            Text("%.2f".format(sliderValue), style = MaterialTheme.typography.bodySmall)
        }
        Slider(
            value = sliderValue,
            onValueChange = { sliderValue = it },
            onValueChangeFinished = { onValueChange(sliderValue.toDouble()) },
            valueRange = min.toFloat()..max.toFloat(),
        )
    }
}
