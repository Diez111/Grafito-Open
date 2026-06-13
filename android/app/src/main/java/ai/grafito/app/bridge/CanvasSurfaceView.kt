package ai.grafito.app.bridge

import android.content.Context
import android.util.Log
import android.view.MotionEvent
import android.view.ScaleGestureDetector
import android.view.Surface
import android.view.SurfaceHolder
import android.view.SurfaceView
import uniffi.grafito_ffi.GrafitoEngine

class CanvasSurfaceView(
    context: Context,
    private val engine: GrafitoEngine,
) : SurfaceView(context), SurfaceHolder.Callback {

    var onTapCallback: ((Float, Float) -> Unit)? = null
    var onPanCallback: ((Float, Float) -> Unit)? = null
    var onZoomCallback: ((Float, Float, Float) -> Unit)? = null

    private var canvasRenderer: uniffi.grafito_ffi.CanvasRenderer? = null
    private var lastX = 0f
    private var lastY = 0f
    private val scaleDetector = ScaleGestureDetector(context, ScaleListener())

    companion object {
        private const val TAG = "CanvasSurfaceView"
        // Threshold aumentado de 10px a 40px para mejor detección en pantallas de alta densidad
        private const val TAP_THRESHOLD = 40f
    }

    init {
        holder.addCallback(this)
    }

    override fun surfaceCreated(holder: SurfaceHolder) {
        val w = holder.surfaceFrame.width()
        val h = holder.surfaceFrame.height()
        if (w <= 0 || h <= 0) {
            Log.w(TAG, "Surface created with invalid size: ${w}x${h}, waiting for layout")
            return
        }
        Log.d(TAG, "Surface created: ${w}x${h}")
        try {
            val ptr = getNativeWindowPtr(holder.surface)
            if (ptr == 0L) {
                Log.e(TAG, "getNativeWindowPtr returned 0")
                return
            }
            canvasRenderer = engine.createCanvasRenderer()
            canvasRenderer?.initWithSurface(ptr.toULong())
            canvasRenderer?.resize(w.toUInt(), h.toUInt())
            engine.updateScreenSize(w.toFloat(), h.toFloat())
            canvasRenderer?.startRenderLoop()
        } catch (e: Exception) {
            Log.e(TAG, "init error", e)
        }
    }

    override fun surfaceChanged(holder: SurfaceHolder, format: Int, width: Int, height: Int) {
        engine.updateScreenSize(width.toFloat(), height.toFloat())
        canvasRenderer?.resize(width.toUInt(), height.toUInt())
    }

    override fun surfaceDestroyed(holder: SurfaceHolder) {
        cleanup()
    }

    fun cleanup() {
        canvasRenderer?.cleanup()
        canvasRenderer = null
    }

    private var wasMultiTouch = false

    override fun onTouchEvent(event: MotionEvent): Boolean {
        scaleDetector.onTouchEvent(event)
        
        if (scaleDetector.isInProgress || event.pointerCount > 1) {
            wasMultiTouch = true
            return true
        }

        var handled = false
        when (event.actionMasked) {
            MotionEvent.ACTION_DOWN -> {
                wasMultiTouch = false
                lastX = event.x
                lastY = event.y
                handled = true
            }
            MotionEvent.ACTION_UP -> {
                val dx = event.x - lastX
                val dy = event.y - lastY
                if (!wasMultiTouch && Math.abs(dx) < TAP_THRESHOLD && Math.abs(dy) < TAP_THRESHOLD) {
                    onTapCallback?.invoke(event.x, event.y)
                    handled = true
                }
                wasMultiTouch = false
            }
            MotionEvent.ACTION_MOVE -> {
                if (wasMultiTouch) {
                    // Acabamos de salir de un multi-touch/zoom.
                    // Reseteamos lastX y lastY para evitar un salto brusco ("se fija y se traba")
                    lastX = event.x
                    lastY = event.y
                    wasMultiTouch = false
                } else if (event.pointerCount == 1) {
                    val dx = event.x - lastX
                    val dy = event.y - lastY
                    if (Math.abs(dx) > 1f || Math.abs(dy) > 1f) {
                        onPanCallback?.invoke(dx, dy)
                        handled = true
                    }
                    lastX = event.x
                    lastY = event.y
                }
            }
        }
        return true // SIEMPRE consumir eventos para evitar que BottomSheetScaffold los intercepte
    }

    private inner class ScaleListener : ScaleGestureDetector.SimpleOnScaleGestureListener() {
        override fun onScale(detector: ScaleGestureDetector): Boolean {
            val factor = detector.scaleFactor
            if (java.lang.Float.isNaN(factor) || java.lang.Float.isInfinite(factor) || factor <= 0.01f || factor >= 100.0f) {
                return true
            }
            onZoomCallback?.invoke(factor, detector.focusX, detector.focusY)
            return true
        }
    }

    private external fun getNativeWindowPtr(surface: Surface): Long
}
