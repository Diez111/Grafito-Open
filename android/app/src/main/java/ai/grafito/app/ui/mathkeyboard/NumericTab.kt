package ai.grafito.app.ui.mathkeyboard

import androidx.compose.foundation.layout.*
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Backspace
import androidx.compose.material.icons.filled.Check
import androidx.compose.material3.*
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp

@Composable
fun NumericTab(
    onInsert: (String) -> Unit,
    onSubmit: () -> Unit,
    onDelete: () -> Unit,
    modifier: Modifier = Modifier
) {
    val buttons = listOf(
        "7", "8", "9", "/", "(", ")",
        "4", "5", "6", "*", "π", "e",
        "1", "2", "3", "-", "√", "^",
        "0", ".", "=", "+", "⌫", "↵"
    )

    Column(
        modifier = modifier
            .fillMaxWidth()
            .padding(8.dp)
    ) {
        for (row in buttons.chunked(6)) {
            Row(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.spacedBy(4.dp)
            ) {
                row.forEach { button ->
                    when (button) {
                        "⌫" -> {
                            OutlinedButton(
                                onClick = onDelete,
                                modifier = Modifier.weight(1f)
                            ) {
                                Icon(Icons.Default.Backspace, contentDescription = "Delete")
                            }
                        }
                        "↵" -> {
                            Button(
                                onClick = onSubmit,
                                modifier = Modifier.weight(1f)
                            ) {
                                Icon(Icons.Default.Check, contentDescription = "Submit")
                            }
                        }
                        else -> {
                            OutlinedButton(
                                onClick = { onInsert(button) },
                                modifier = Modifier.weight(1f)
                            ) {
                                Text(button)
                            }
                        }
                    }
                }
            }
            Spacer(modifier = Modifier.height(4.dp))
        }
    }
}
