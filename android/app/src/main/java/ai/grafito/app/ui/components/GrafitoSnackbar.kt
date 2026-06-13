package ai.grafito.app.ui.components

import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Snackbar
import androidx.compose.material3.SnackbarHost
import androidx.compose.material3.SnackbarHostState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp

@Composable
fun GrafitoSnackbar(
    message: String?,
    snackbarHostState: SnackbarHostState,
    modifier: Modifier = Modifier,
) {
    LaunchedEffect(message) {
        if (!message.isNullOrBlank()) {
            snackbarHostState.showSnackbar(message = message, withDismissAction = true)
        }
    }

    SnackbarHost(
        hostState = snackbarHostState,
        modifier = modifier.padding(8.dp),
    )
}
