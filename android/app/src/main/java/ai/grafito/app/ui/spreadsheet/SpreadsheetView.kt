package ai.grafito.app.ui.spreadsheet

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.rememberScrollState
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import ai.grafito.app.viewmodel.GrafitoViewModel

@Composable
fun SpreadsheetView(
    viewModel: GrafitoViewModel,
    modifier: Modifier = Modifier,
) {
    var showEditor by remember { mutableStateOf(false) }
    var selectedCell by remember { mutableStateOf<Pair<Int, Int>?>(null) }
    var cellValue by remember { mutableStateOf("") }

    val rowCount = 50
    val colCount = 26

    Column(modifier = modifier.fillMaxSize()) {
        // Header row
        Row(
            modifier = Modifier
                .horizontalScroll(rememberScrollState())
                .background(MaterialTheme.colorScheme.surfaceVariant),
        ) {
            Box(
                modifier = Modifier
                    .width(50.dp)
                    .height(40.dp)
                    .border(1.dp, MaterialTheme.colorScheme.outline),
            )

            for (col in 0 until colCount) {
                Box(
                    modifier = Modifier
                        .width(80.dp)
                        .height(40.dp)
                        .border(1.dp, MaterialTheme.colorScheme.outline),
                    contentAlignment = Alignment.Center,
                ) {
                    Text(('A' + col).toString(), style = MaterialTheme.typography.labelMedium)
                }
            }
        }

        // Data rows
        LazyColumn(modifier = Modifier.fillMaxSize()) {
            itemsIndexed(List(rowCount) { it }) { rowIndex, _ ->
                Row(modifier = Modifier.horizontalScroll(rememberScrollState())) {
                    Box(
                        modifier = Modifier
                            .width(50.dp)
                            .height(40.dp)
                            .border(1.dp, MaterialTheme.colorScheme.outline)
                            .background(MaterialTheme.colorScheme.surfaceVariant),
                        contentAlignment = Alignment.Center,
                    ) {
                        Text((rowIndex + 1).toString(), style = MaterialTheme.typography.labelMedium)
                    }

                    for (col in 0 until colCount) {
                        val cellLabel = "${'A' + col}${rowIndex + 1}"
                        val isSelected = selectedCell == (rowIndex to col)
                        val varValue = viewModel.documentState.variables
                            .find { it.name == cellLabel }?.value?.toString() ?: ""

                        Box(
                            modifier = Modifier
                                .width(80.dp)
                                .height(40.dp)
                                .border(
                                    width = if (isSelected) 2.dp else 1.dp,
                                    color = if (isSelected)
                                        MaterialTheme.colorScheme.primary
                                    else
                                        MaterialTheme.colorScheme.outline,
                                )
                                .clickable {
                                    selectedCell = rowIndex to col
                                    cellValue = varValue
                                    showEditor = true
                                },
                            contentAlignment = Alignment.CenterStart,
                        ) {
                            Text(varValue, modifier = Modifier.padding(horizontal = 4.dp))
                        }
                    }
                }
            }
        }
    }

    if (showEditor && selectedCell != null) {
        val (row, col) = selectedCell!!
        val cellName = "${'A' + col}${row + 1}"

        AlertDialog(
            onDismissRequest = { showEditor = false },
            title = { Text("Edit $cellName") },
            text = {
                OutlinedTextField(
                    value = cellValue,
                    onValueChange = { cellValue = it },
                    label = { Text("Value") },
                    singleLine = true,
                )
            },
            confirmButton = {
                TextButton(onClick = {
                    viewModel.processCommand("$cellName = $cellValue")
                    showEditor = false
                }) { Text("Save") }
            },
            dismissButton = {
                TextButton(onClick = { showEditor = false }) { Text("Cancel") }
            },
        )
    }
}
