package uniffi.grafito_ffi;

import com.sun.jna.Library;
import com.sun.jna.IntegerType;
import com.sun.jna.Native;
import com.sun.jna.Pointer;
import com.sun.jna.Structure;
import com.sun.jna.Callback;
import com.sun.jna.ptr.*;
import java.nio.ByteBuffer;
import java.nio.ByteOrder;
import java.nio.CharBuffer;
import java.nio.charset.CodingErrorAction;
import java.util.concurrent.atomic.AtomicLong;
import java.util.concurrent.ConcurrentHashMap;
import java.util.concurrent.atomic.AtomicBoolean;

@kotlin.Metadata(mv = {1, 9, 0}, k = 1, xi = 48, d1 = {"\u0000h\n\u0002\u0018\u0002\n\u0002\u0018\u0002\n\u0002\u0018\u0002\n\u0002\u0018\u0002\n\u0000\n\u0002\u0018\u0002\n\u0000\n\u0002\u0010\t\n\u0002\b\u0002\n\u0002\u0018\u0002\n\u0002\b\u0002\n\u0002\u0018\u0002\n\u0000\n\u0002\u0018\u0002\n\u0002\b\u0003\n\u0002\u0018\u0002\n\u0000\n\u0002\u0018\u0002\n\u0002\b\u0005\n\u0002\u0018\u0002\n\u0002\b\u0003\n\u0002\u0018\u0002\n\u0002\u0018\u0002\n\u0002\b\u0004\n\u0002\u0010\u0002\n\u0002\b\t\n\u0002\u0018\u0002\n\u0002\b\f\b\u0016\u0018\u0000 92\u00020\u00012\u00020\u00022\u00020\u0003:\u00029:B\u0017\b\u0016\u0012\u0006\u0010\u0004\u001a\u00020\u0005\u0012\u0006\u0010\u0006\u001a\u00020\u0007\u00a2\u0006\u0002\u0010\bB\u000f\b\u0016\u0012\u0006\u0010\t\u001a\u00020\n\u00a2\u0006\u0002\u0010\u000bB\u001f\b\u0016\u0012\u0006\u0010\f\u001a\u00020\r\u0012\u0006\u0010\u000e\u001a\u00020\u000f\u0012\u0006\u0010\u0010\u001a\u00020\u000f\u00a2\u0006\u0002\u0010\u0011J<\u0010\u001c\u001a\u0002H\u001d\"\u0004\b\u0000\u0010\u001d2!\u0010\u001e\u001a\u001d\u0012\u0013\u0012\u00110\u0007\u00a2\u0006\f\b \u0012\b\b!\u0012\u0004\b\b(\u0006\u0012\u0004\u0012\u0002H\u001d0\u001fH\u0080\b\u00f8\u0001\u0000\u00a2\u0006\u0004\b\"\u0010#J\b\u0010$\u001a\u00020%H\u0016J\b\u0010&\u001a\u00020%H\u0016J\b\u0010\'\u001a\u00020%H\u0016J\u0015\u0010(\u001a\u00020\u000fH\u0016\u00f8\u0001\u0001\u00f8\u0001\u0002\u00a2\u0006\u0004\b)\u0010*J\u0015\u0010+\u001a\u00020\u000fH\u0016\u00f8\u0001\u0001\u00f8\u0001\u0002\u00a2\u0006\u0004\b,\u0010*J\u001a\u0010-\u001a\u00020%2\u0006\u0010.\u001a\u00020/H\u0016\u00f8\u0001\u0002\u00a2\u0006\u0004\b0\u00101J\b\u00102\u001a\u00020%H\u0016J\"\u00103\u001a\u00020%2\u0006\u0010\u000e\u001a\u00020\u000f2\u0006\u0010\u0010\u001a\u00020\u000fH\u0016\u00f8\u0001\u0002\u00a2\u0006\u0004\b4\u00105J\b\u00106\u001a\u00020%H\u0016J\b\u00107\u001a\u00020%H\u0016J\u0006\u00108\u001a\u00020\u0007R\u000e\u0010\u0012\u001a\u00020\u0013X\u0082\u0004\u00a2\u0006\u0002\n\u0000R\u0016\u0010\u0014\u001a\u0004\u0018\u00010\u0015X\u0084\u0004\u00a2\u0006\b\n\u0000\u001a\u0004\b\u0016\u0010\u0017R\u0014\u0010\u0006\u001a\u00020\u0007X\u0084\u0004\u00a2\u0006\b\n\u0000\u001a\u0004\b\u0018\u0010\u0019R\u000e\u0010\u001a\u001a\u00020\u001bX\u0082\u0004\u00a2\u0006\u0002\n\u0000\u0082\u0002\u0012\n\u0005\b\u009920\u0001\n\u0002\b!\n\u0005\b\u00a1\u001e0\u0001\u00a8\u0006;"}, d2 = {"Luniffi/grafito_ffi/CanvasRenderer;", "Luniffi/grafito_ffi/Disposable;", "Ljava/lang/AutoCloseable;", "Luniffi/grafito_ffi/CanvasRendererInterface;", "withHandle", "Luniffi/grafito_ffi/UniffiWithHandle;", "handle", "", "(Luniffi/grafito_ffi/UniffiWithHandle;J)V", "noHandle", "Luniffi/grafito_ffi/NoHandle;", "(Luniffi/grafito_ffi/NoHandle;)V", "engine", "Luniffi/grafito_ffi/GrafitoEngine;", "width", "Lkotlin/UInt;", "height", "(Luniffi/grafito_ffi/GrafitoEngine;IILkotlin/jvm/internal/DefaultConstructorMarker;)V", "callCounter", "Ljava/util/concurrent/atomic/AtomicLong;", "cleanable", "Luniffi/grafito_ffi/UniffiCleaner$Cleanable;", "getCleanable", "()Luniffi/grafito_ffi/UniffiCleaner$Cleanable;", "getHandle", "()J", "wasDestroyed", "Ljava/util/concurrent/atomic/AtomicBoolean;", "callWithHandle", "R", "block", "Lkotlin/Function1;", "Lkotlin/ParameterName;", "name", "callWithHandle$app_debug", "(Lkotlin/jvm/functions/Function1;)Ljava/lang/Object;", "cleanup", "", "close", "destroy", "getHeight", "getHeight-pVg5ArA", "()I", "getWidth", "getWidth-pVg5ArA", "initWithSurface", "surfacePtr", "Lkotlin/ULong;", "initWithSurface-VKZWuLQ", "(J)V", "renderFrame", "resize", "resize-feOb9K0", "(II)V", "startRenderLoop", "stopRenderLoop", "uniffiCloneHandle", "Companion", "UniffiCleanAction", "app_debug"})
public class CanvasRenderer implements uniffi.grafito_ffi.Disposable, java.lang.AutoCloseable, uniffi.grafito_ffi.CanvasRendererInterface {
    private final long handle = 0L;
    @org.jetbrains.annotations.Nullable()
    private final uniffi.grafito_ffi.UniffiCleaner.Cleanable cleanable = null;
    @org.jetbrains.annotations.NotNull()
    private final java.util.concurrent.atomic.AtomicBoolean wasDestroyed = null;
    @org.jetbrains.annotations.NotNull()
    private final java.util.concurrent.atomic.AtomicLong callCounter = null;
    @org.jetbrains.annotations.NotNull()
    public static final uniffi.grafito_ffi.CanvasRenderer.Companion Companion = null;
    
    /**
     * @suppress
     */
    @kotlin.Suppress(names = {"UNUSED_PARAMETER"})
    public CanvasRenderer(@org.jetbrains.annotations.NotNull()
    uniffi.grafito_ffi.UniffiWithHandle withHandle, long handle) {
        super();
    }
    
    /**
     * @suppress
     *
     * This constructor can be used to instantiate a fake object. Only used for tests. Any
     * attempt to actually use an object constructed this way will fail as there is no
     * connected Rust object.
     */
    @kotlin.Suppress(names = {"UNUSED_PARAMETER"})
    public CanvasRenderer(@org.jetbrains.annotations.NotNull()
    uniffi.grafito_ffi.NoHandle noHandle) {
        super();
    }
    
    private CanvasRenderer(uniffi.grafito_ffi.GrafitoEngine engine, int width, int height) {
        super();
    }
    
    protected final long getHandle() {
        return 0L;
    }
    
    @org.jetbrains.annotations.Nullable()
    protected final uniffi.grafito_ffi.UniffiCleaner.Cleanable getCleanable() {
        return null;
    }
    
    @java.lang.Override()
    public void destroy() {
    }
    
    @java.lang.Override()
    @kotlin.jvm.Synchronized()
    public synchronized void close() {
    }
    
    public final <R extends java.lang.Object>R callWithHandle$app_debug(@org.jetbrains.annotations.NotNull()
    kotlin.jvm.functions.Function1<? super java.lang.Long, ? extends R> block) {
        return null;
    }
    
    /**
     * @suppress
     */
    public final long uniffiCloneHandle() {
        return 0L;
    }
    
    @java.lang.Override()
    public void cleanup() {
    }
    
    @java.lang.Override()
    @kotlin.jvm.Throws(exceptionClasses = {uniffi.grafito_ffi.CanvasException.class})
    public void renderFrame() throws uniffi.grafito_ffi.CanvasException {
    }
    
    @java.lang.Override()
    public void startRenderLoop() {
    }
    
    @java.lang.Override()
    public void stopRenderLoop() {
    }
    
    /**
     * @suppress
     */
    @kotlin.Metadata(mv = {1, 9, 0}, k = 1, xi = 48, d1 = {"\u0000\f\n\u0002\u0018\u0002\n\u0002\u0010\u0000\n\u0002\b\u0002\b\u0086\u0003\u0018\u00002\u00020\u0001B\u0007\b\u0002\u00a2\u0006\u0002\u0010\u0002\u00a8\u0006\u0003"}, d2 = {"Luniffi/grafito_ffi/CanvasRenderer$Companion;", "", "()V", "app_debug"})
    public static final class Companion {
        
        private Companion() {
            super();
        }
    }
    
    @kotlin.Metadata(mv = {1, 9, 0}, k = 1, xi = 48, d1 = {"\u0000\u0018\n\u0002\u0018\u0002\n\u0002\u0018\u0002\n\u0000\n\u0002\u0010\t\n\u0002\b\u0002\n\u0002\u0010\u0002\n\u0000\b\u0002\u0018\u00002\u00020\u0001B\r\u0012\u0006\u0010\u0002\u001a\u00020\u0003\u00a2\u0006\u0002\u0010\u0004J\b\u0010\u0005\u001a\u00020\u0006H\u0016R\u000e\u0010\u0002\u001a\u00020\u0003X\u0082\u0004\u00a2\u0006\u0002\n\u0000\u00a8\u0006\u0007"}, d2 = {"Luniffi/grafito_ffi/CanvasRenderer$UniffiCleanAction;", "Ljava/lang/Runnable;", "handle", "", "(J)V", "run", "", "app_debug"})
    static final class UniffiCleanAction implements java.lang.Runnable {
        private final long handle = 0L;
        
        public UniffiCleanAction(long handle) {
            super();
        }
        
        @java.lang.Override()
        public void run() {
        }
    }
}