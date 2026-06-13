package ai.grafito.app.bridge;

import android.content.Context;
import android.util.Log;
import android.view.MotionEvent;
import android.view.ScaleGestureDetector;
import android.view.Surface;
import android.view.SurfaceHolder;
import android.view.SurfaceView;
import uniffi.grafito_ffi.GrafitoEngine;

@kotlin.Metadata(mv = {1, 9, 0}, k = 1, xi = 48, d1 = {"\u0000n\n\u0002\u0018\u0002\n\u0002\u0018\u0002\n\u0002\u0018\u0002\n\u0000\n\u0002\u0018\u0002\n\u0000\n\u0002\u0018\u0002\n\u0002\b\u0002\n\u0002\u0018\u0002\n\u0000\n\u0002\u0010\u0007\n\u0002\b\u0002\n\u0002\u0018\u0002\n\u0002\u0010\u0002\n\u0002\b\b\n\u0002\u0018\u0002\n\u0002\b\u0005\n\u0002\u0018\u0002\n\u0000\n\u0002\u0010\u000b\n\u0000\n\u0002\u0010\t\n\u0000\n\u0002\u0018\u0002\n\u0002\b\u0002\n\u0002\u0018\u0002\n\u0002\b\u0002\n\u0002\u0018\u0002\n\u0000\n\u0002\u0010\b\n\u0002\b\u0007\u0018\u0000 12\u00020\u00012\u00020\u0002:\u000212B\u0015\u0012\u0006\u0010\u0003\u001a\u00020\u0004\u0012\u0006\u0010\u0005\u001a\u00020\u0006\u00a2\u0006\u0002\u0010\u0007J\u0011\u0010!\u001a\u00020\"2\u0006\u0010#\u001a\u00020$H\u0082 J\u0010\u0010%\u001a\u00020 2\u0006\u0010&\u001a\u00020\'H\u0016J(\u0010(\u001a\u00020\u000f2\u0006\u0010)\u001a\u00020*2\u0006\u0010+\u001a\u00020,2\u0006\u0010-\u001a\u00020,2\u0006\u0010.\u001a\u00020,H\u0016J\u0010\u0010/\u001a\u00020\u000f2\u0006\u0010)\u001a\u00020*H\u0016J\u0010\u00100\u001a\u00020\u000f2\u0006\u0010)\u001a\u00020*H\u0016R\u0010\u0010\b\u001a\u0004\u0018\u00010\tX\u0082\u000e\u00a2\u0006\u0002\n\u0000R\u000e\u0010\u0005\u001a\u00020\u0006X\u0082\u0004\u00a2\u0006\u0002\n\u0000R\u000e\u0010\n\u001a\u00020\u000bX\u0082\u000e\u00a2\u0006\u0002\n\u0000R\u000e\u0010\f\u001a\u00020\u000bX\u0082\u000e\u00a2\u0006\u0002\n\u0000R.\u0010\r\u001a\u0016\u0012\u0004\u0012\u00020\u000b\u0012\u0004\u0012\u00020\u000b\u0012\u0004\u0012\u00020\u000f\u0018\u00010\u000eX\u0086\u000e\u00a2\u0006\u000e\n\u0000\u001a\u0004\b\u0010\u0010\u0011\"\u0004\b\u0012\u0010\u0013R.\u0010\u0014\u001a\u0016\u0012\u0004\u0012\u00020\u000b\u0012\u0004\u0012\u00020\u000b\u0012\u0004\u0012\u00020\u000f\u0018\u00010\u000eX\u0086\u000e\u00a2\u0006\u000e\n\u0000\u001a\u0004\b\u0015\u0010\u0011\"\u0004\b\u0016\u0010\u0013R4\u0010\u0017\u001a\u001c\u0012\u0004\u0012\u00020\u000b\u0012\u0004\u0012\u00020\u000b\u0012\u0004\u0012\u00020\u000b\u0012\u0004\u0012\u00020\u000f\u0018\u00010\u0018X\u0086\u000e\u00a2\u0006\u000e\n\u0000\u001a\u0004\b\u0019\u0010\u001a\"\u0004\b\u001b\u0010\u001cR\u000e\u0010\u001d\u001a\u00020\u001eX\u0082\u0004\u00a2\u0006\u0002\n\u0000R\u000e\u0010\u001f\u001a\u00020 X\u0082\u000e\u00a2\u0006\u0002\n\u0000\u00a8\u00063"}, d2 = {"Lai/grafito/app/bridge/CanvasSurfaceView;", "Landroid/view/SurfaceView;", "Landroid/view/SurfaceHolder$Callback;", "context", "Landroid/content/Context;", "engine", "Luniffi/grafito_ffi/GrafitoEngine;", "(Landroid/content/Context;Luniffi/grafito_ffi/GrafitoEngine;)V", "canvasRenderer", "Luniffi/grafito_ffi/CanvasRenderer;", "lastX", "", "lastY", "onPanCallback", "Lkotlin/Function2;", "", "getOnPanCallback", "()Lkotlin/jvm/functions/Function2;", "setOnPanCallback", "(Lkotlin/jvm/functions/Function2;)V", "onTapCallback", "getOnTapCallback", "setOnTapCallback", "onZoomCallback", "Lkotlin/Function3;", "getOnZoomCallback", "()Lkotlin/jvm/functions/Function3;", "setOnZoomCallback", "(Lkotlin/jvm/functions/Function3;)V", "scaleDetector", "Landroid/view/ScaleGestureDetector;", "wasMultiTouch", "", "getNativeWindowPtr", "", "surface", "Landroid/view/Surface;", "onTouchEvent", "event", "Landroid/view/MotionEvent;", "surfaceChanged", "holder", "Landroid/view/SurfaceHolder;", "format", "", "width", "height", "surfaceCreated", "surfaceDestroyed", "Companion", "ScaleListener", "app_debug"})
public final class CanvasSurfaceView extends android.view.SurfaceView implements android.view.SurfaceHolder.Callback {
    @org.jetbrains.annotations.NotNull()
    private final uniffi.grafito_ffi.GrafitoEngine engine = null;
    @org.jetbrains.annotations.Nullable()
    private kotlin.jvm.functions.Function2<? super java.lang.Float, ? super java.lang.Float, kotlin.Unit> onTapCallback;
    @org.jetbrains.annotations.Nullable()
    private kotlin.jvm.functions.Function2<? super java.lang.Float, ? super java.lang.Float, kotlin.Unit> onPanCallback;
    @org.jetbrains.annotations.Nullable()
    private kotlin.jvm.functions.Function3<? super java.lang.Float, ? super java.lang.Float, ? super java.lang.Float, kotlin.Unit> onZoomCallback;
    @org.jetbrains.annotations.Nullable()
    private uniffi.grafito_ffi.CanvasRenderer canvasRenderer;
    private float lastX = 0.0F;
    private float lastY = 0.0F;
    @org.jetbrains.annotations.NotNull()
    private final android.view.ScaleGestureDetector scaleDetector = null;
    @org.jetbrains.annotations.NotNull()
    private static final java.lang.String TAG = "CanvasSurfaceView";
    private static final float TAP_THRESHOLD = 40.0F;
    private boolean wasMultiTouch = false;
    @org.jetbrains.annotations.NotNull()
    public static final ai.grafito.app.bridge.CanvasSurfaceView.Companion Companion = null;
    
    public CanvasSurfaceView(@org.jetbrains.annotations.NotNull()
    android.content.Context context, @org.jetbrains.annotations.NotNull()
    uniffi.grafito_ffi.GrafitoEngine engine) {
        super(null);
    }
    
    @org.jetbrains.annotations.Nullable()
    public final kotlin.jvm.functions.Function2<java.lang.Float, java.lang.Float, kotlin.Unit> getOnTapCallback() {
        return null;
    }
    
    public final void setOnTapCallback(@org.jetbrains.annotations.Nullable()
    kotlin.jvm.functions.Function2<? super java.lang.Float, ? super java.lang.Float, kotlin.Unit> p0) {
    }
    
    @org.jetbrains.annotations.Nullable()
    public final kotlin.jvm.functions.Function2<java.lang.Float, java.lang.Float, kotlin.Unit> getOnPanCallback() {
        return null;
    }
    
    public final void setOnPanCallback(@org.jetbrains.annotations.Nullable()
    kotlin.jvm.functions.Function2<? super java.lang.Float, ? super java.lang.Float, kotlin.Unit> p0) {
    }
    
    @org.jetbrains.annotations.Nullable()
    public final kotlin.jvm.functions.Function3<java.lang.Float, java.lang.Float, java.lang.Float, kotlin.Unit> getOnZoomCallback() {
        return null;
    }
    
    public final void setOnZoomCallback(@org.jetbrains.annotations.Nullable()
    kotlin.jvm.functions.Function3<? super java.lang.Float, ? super java.lang.Float, ? super java.lang.Float, kotlin.Unit> p0) {
    }
    
    @java.lang.Override()
    public void surfaceCreated(@org.jetbrains.annotations.NotNull()
    android.view.SurfaceHolder holder) {
    }
    
    @java.lang.Override()
    public void surfaceChanged(@org.jetbrains.annotations.NotNull()
    android.view.SurfaceHolder holder, int format, int width, int height) {
    }
    
    @java.lang.Override()
    public void surfaceDestroyed(@org.jetbrains.annotations.NotNull()
    android.view.SurfaceHolder holder) {
    }
    
    @java.lang.Override()
    public boolean onTouchEvent(@org.jetbrains.annotations.NotNull()
    android.view.MotionEvent event) {
        return false;
    }
    
    private final native long getNativeWindowPtr(android.view.Surface surface) {
        return 0L;
    }
    
    @kotlin.Metadata(mv = {1, 9, 0}, k = 1, xi = 48, d1 = {"\u0000\u0018\n\u0002\u0018\u0002\n\u0002\u0010\u0000\n\u0002\b\u0002\n\u0002\u0010\u000e\n\u0000\n\u0002\u0010\u0007\n\u0000\b\u0086\u0003\u0018\u00002\u00020\u0001B\u0007\b\u0002\u00a2\u0006\u0002\u0010\u0002R\u000e\u0010\u0003\u001a\u00020\u0004X\u0082T\u00a2\u0006\u0002\n\u0000R\u000e\u0010\u0005\u001a\u00020\u0006X\u0082T\u00a2\u0006\u0002\n\u0000\u00a8\u0006\u0007"}, d2 = {"Lai/grafito/app/bridge/CanvasSurfaceView$Companion;", "", "()V", "TAG", "", "TAP_THRESHOLD", "", "app_debug"})
    public static final class Companion {
        
        private Companion() {
            super();
        }
    }
    
    @kotlin.Metadata(mv = {1, 9, 0}, k = 1, xi = 48, d1 = {"\u0000\u0018\n\u0002\u0018\u0002\n\u0002\u0018\u0002\n\u0002\b\u0002\n\u0002\u0010\u000b\n\u0000\n\u0002\u0018\u0002\n\u0000\b\u0082\u0004\u0018\u00002\u00020\u0001B\u0005\u00a2\u0006\u0002\u0010\u0002J\u0010\u0010\u0003\u001a\u00020\u00042\u0006\u0010\u0005\u001a\u00020\u0006H\u0016\u00a8\u0006\u0007"}, d2 = {"Lai/grafito/app/bridge/CanvasSurfaceView$ScaleListener;", "Landroid/view/ScaleGestureDetector$SimpleOnScaleGestureListener;", "(Lai/grafito/app/bridge/CanvasSurfaceView;)V", "onScale", "", "detector", "Landroid/view/ScaleGestureDetector;", "app_debug"})
    final class ScaleListener extends android.view.ScaleGestureDetector.SimpleOnScaleGestureListener {
        
        public ScaleListener() {
            super();
        }
        
        @java.lang.Override()
        public boolean onScale(@org.jetbrains.annotations.NotNull()
        android.view.ScaleGestureDetector detector) {
            return false;
        }
    }
}