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
fun ThreeDTab(
    onInsert: (String) -> Unit,
    onSubmit: () -> Unit,
    onDelete: () -> Unit,
    modifier: Modifier = Modifier
) {
    val commands = listOf(
        "Point3D", "Sphere3D", "Cube3D", "Cone3D", "Cylinder3D", "Torus3D",
        "Surface3D", "Attractor3D", "VectorField3D", "HyperSurface4D"
    )

    Column(
        modifier = modifier
            .fillMaxWidth()
            .padding(8.dp)
    ) {
        for (row in commands.chunked(3)) {
            Row(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.spacedBy(4.dp)
            ) {
                row.forEach { cmd ->
                    OutlinedButton(
                        onClick = { onInsert("$cmd[") },
                        modifier = Modifier.weight(1f)
                    ) {
                        Text(cmd, style = MaterialTheme.typography.bodySmall)
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
