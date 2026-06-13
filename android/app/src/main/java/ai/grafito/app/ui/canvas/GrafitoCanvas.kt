package ai.grafito.app.ui.canvas

import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.viewinterop.AndroidView
import ai.grafito.app.bridge.CanvasSurfaceView
import ai.grafito.app.viewmodel.GrafitoViewModel

@Composable
fun GrafitoCanvas(
    viewModel: GrafitoViewModel,
    modifier: Modifier = Modifier,
) {
    val context = LocalContext.current
    val engine = viewModel.getEngine()

    AndroidView(
        modifier = modifier.fillMaxSize(),
        factory = {
            CanvasSurfaceView(context, engine).also { sv ->
                sv.onTapCallback = { x, y -> viewModel.canvasTap(x, y) }
                sv.onPanCallback = { dx, dy -> viewModel.canvasPan(dx, dy) }
                sv.onZoomCallback = { factor, cx, cy -> viewModel.canvasZoom(factor, cx, cy) }
            }
        },
        update = {},
        onRelease = { (it as CanvasSurfaceView).cleanup() },
    )
}
