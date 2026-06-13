package ai.grafito.app.ui.components

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.grid.GridCells
import androidx.compose.foundation.lazy.grid.LazyVerticalGrid
import androidx.compose.foundation.lazy.grid.items
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.toArgb
import androidx.compose.ui.unit.dp

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun ColorPickerDialog(
    currentColor: Color,
    onColorSelected: (Color) -> Unit,
    onDismiss: () -> Unit
) {
    var selectedColor by remember { mutableStateOf(currentColor) }

    // Predefined color palette
    val colors = listOf(
        Color(0xFFE53935), // Red
        Color(0xFFD81B60), // Pink
        Color(0xFF8E24AA), // Purple
        Color(0xFF5E35B1), // Deep Purple
        Color(0xFF3949AB), // Indigo
        Color(0xFF1E88E5), // Blue
        Color(0xFF039BE5), // Light Blue
        Color(0xFF00ACC1), // Cyan
        Color(0xFF00897B), // Teal
        Color(0xFF43A047), // Green
        Color(0xFF7CB342), // Light Green
        Color(0xFFC0CA33), // Lime
        Color(0xFFFDD835), // Yellow
        Color(0xFFFFB300), // Amber
        Color(0xFFFB8C00), // Orange
        Color(0xFFF4511E), // Deep Orange
        Color(0xFF6D4C41), // Brown
        Color(0xFF757575), // Grey
        Color(0xFF546E7A), // Blue Grey
        Color(0xFF000000), // Black
    )

    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text("Select Color") },
        text = {
            Column {
                // Color preview
                Box(
                    modifier = Modifier
                        .fillMaxWidth()
                        .height(60.dp)
                        .clip(MaterialTheme.shapes.medium)
                        .background(selectedColor)
                )

                Spacer(modifier = Modifier.height(16.dp))

                // Color grid
                LazyVerticalGrid(
                    columns = GridCells.Fixed(5),
                    modifier = Modifier.height(200.dp),
                    horizontalArrangement = Arrangement.spacedBy(8.dp),
                    verticalArrangement = Arrangement.spacedBy(8.dp)
                ) {
                    items(colors) { color ->
                        Box(
                            modifier = Modifier
                                .size(48.dp)
                                .clip(CircleShape)
                                .background(color)
                                .clickable { selectedColor = color }
                                .then(
                                    if (selectedColor == color) {
                                        Modifier.border(
                                            width = 3.dp,
                                            color = MaterialTheme.colorScheme.primary,
                                            shape = CircleShape
                                        )
                                    } else {
                                        Modifier
                                    }
                                )
                        )
                    }
                }
            }
        },
        confirmButton = {
            TextButton(onClick = { onColorSelected(selectedColor) }) {
                Text("Select")
            }
        },
        dismissButton = {
            TextButton(onClick = onDismiss) {
                Text("Cancel")
            }
        }
    )
}
