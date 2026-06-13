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
fun FunctionTab(
    onInsert: (String) -> Unit,
    onSubmit: () -> Unit,
    onDelete: () -> Unit,
    modifier: Modifier = Modifier
) {
    val functions = listOf(
        "sin", "cos", "tan", "log", "ln", "exp",
        "asin", "acos", "atan", "abs", "floor", "ceil",
        "sinh", "cosh", "tanh", "sign", "min", "max"
    )

    Column(
        modifier = modifier
            .fillMaxWidth()
            .padding(8.dp)
    ) {
        for (row in functions.chunked(6)) {
            Row(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.spacedBy(4.dp)
            ) {
                row.forEach { func ->
                    OutlinedButton(
                        onClick = { onInsert("$func(") },
                        modifier = Modifier.weight(1f)
                    ) {
                        Text(func, style = MaterialTheme.typography.bodySmall)
                    }
                }
            }
            Spacer(modifier = Modifier.height(4.dp))
        }

        // Fila inferior con Submit y Delete
        Spacer(modifier = Modifier.height(4.dp))
        Row(
            modifier = Modifier.fillMaxWidth(),
            horizontalArrangement = Arrangement.spacedBy(4.dp)
        ) {
            OutlinedButton(
                onClick = onDelete,
                modifier = Modifier.weight(1f)
            ) {
                Icon(Icons.Default.Backspace, contentDescription = "Delete")
            }
            Button(
                onClick = onSubmit,
                modifier = Modifier.weight(1f)
            ) {
                Icon(Icons.Default.Check, contentDescription = "Submit")
            }
        }
    }
}
