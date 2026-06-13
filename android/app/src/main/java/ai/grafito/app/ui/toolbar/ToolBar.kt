package ai.grafito.app.ui.toolbar

import androidx.compose.animation.animateColorAsState
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyRow
import androidx.compose.foundation.lazy.items
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.*
import androidx.compose.material3.*
import androidx.compose.material3.MaterialTheme.colorScheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.unit.dp
import uniffi.grafito_ffi.ToolDto

private data class ToolDef(val dto: ToolDto, val icon: ImageVector, val label: String, val category: String)

private val tools2D = listOf(
    ToolDef(ToolDto.SELECT, Icons.Default.NearMe, "Select", "2D"),
    ToolDef(ToolDto.POINT, Icons.Default.FiberManualRecord, "Point", "2D"),
    ToolDef(ToolDto.LINE, Icons.Default.Timeline, "Line", "2D"),
    ToolDef(ToolDto.CIRCLE, Icons.Default.RadioButtonUnchecked, "Circle", "2D"),
    ToolDef(ToolDto.POLYGON, Icons.Default.Hexagon, "Polygon", "2D"),
    ToolDef(ToolDto.FUNCTION, Icons.Default.Functions, "F(x)", "2D"),
)

private val tools3D = listOf(
    ToolDef(ToolDto.POINT3_D, Icons.Default.ViewInAr, "3D Pt", "3D"),
    ToolDef(ToolDto.SPHERE3_D, Icons.Default.SportsBasketball, "Sphere", "3D"),
    ToolDef(ToolDto.CUBE3_D, Icons.Default.ViewInAr, "Cube", "3D"),
)

private val toolsAdvanced = listOf(
    ToolDef(ToolDto.ATTRACTOR, Icons.Default.Hub, "Attract", "Adv"),
    ToolDef(ToolDto.FRACTAL, Icons.Default.BlurOn, "Fractal", "Adv"),
    ToolDef(ToolDto.HISTOGRAM, Icons.Default.BarChart, "Hist", "Adv"),
    ToolDef(ToolDto.SCATTER_PLOT, Icons.Default.ScatterPlot, "Scatter", "Adv"),
)

@Composable
fun ToolBar(
    currentTool: ToolDto,
    darkMode: Boolean,
    viewMode: String,
    onToolSelected: (ToolDto) -> Unit,
    onClear: () -> Unit,
    modifier: Modifier = Modifier,
) {
    // Filtrar herramientas según viewMode
    val availableTools = when (viewMode) {
        "2D" -> tools2D
        "3D" -> tools3D + toolsAdvanced
        else -> tools2D + tools3D
    }

    Surface(
        modifier = modifier,
        tonalElevation = 2.dp,
        color = colorScheme.surface,
    ) {
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .padding(horizontal = 8.dp, vertical = 4.dp),
        ) {
            // Tool chips row — horizontally scrollable
            LazyRow(
                horizontalArrangement = Arrangement.spacedBy(6.dp),
                modifier = Modifier.fillMaxWidth(),
            ) {
                items(availableTools) { tool ->
                    ToolChip(
                        tool = tool.dto,
                        icon = tool.icon,
                        label = tool.label,
                        isSelected = tool.dto == currentTool,
                        onClick = { onToolSelected(tool.dto) },
                    )
                }
            }


        }
    }
}

@Composable
private fun ToolChip(
    tool: ToolDto,
    icon: ImageVector,
    label: String,
    isSelected: Boolean,
    onClick: () -> Unit,
) {
    val bg by animateColorAsState(
        if (isSelected) colorScheme.primaryContainer else colorScheme.surfaceVariant,
        label = "chip_bg",
    )
    val fg by animateColorAsState(
        if (isSelected) colorScheme.onPrimaryContainer else colorScheme.onSurfaceVariant,
        label = "chip_fg",
    )

    Surface(
        onClick = onClick,
        shape = MaterialTheme.shapes.small,
        color = bg,
        tonalElevation = if (isSelected) 2.dp else 0.dp,
    ) {
        Row(
            modifier = Modifier.padding(horizontal = 10.dp, vertical = 6.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Icon(icon, label, tint = fg, modifier = Modifier.size(18.dp))
            Spacer(Modifier.width(4.dp))
            Text(label, style = MaterialTheme.typography.labelMedium, color = fg)
        }
    }
}

@Composable
private fun SmallIconButton(
    icon: ImageVector,
    desc: String,
    onClick: () -> Unit,
    tint: androidx.compose.ui.graphics.Color = colorScheme.onSurfaceVariant,
) {
    IconButton(onClick = onClick, modifier = Modifier.size(36.dp)) {
        Icon(icon, desc, tint = tint, modifier = Modifier.size(20.dp))
    }
}
