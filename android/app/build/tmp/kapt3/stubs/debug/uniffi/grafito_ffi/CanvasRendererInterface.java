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

@kotlin.Metadata(mv = {1, 9, 0}, k = 1, xi = 48, d1 = {"\u0000 \n\u0002\u0018\u0002\n\u0002\u0010\u0000\n\u0000\n\u0002\u0010\u0002\n\u0000\n\u0002\u0018\u0002\n\u0002\b\u0006\n\u0002\u0018\u0002\n\u0002\b\f\bf\u0018\u0000 \u00172\u00020\u0001:\u0001\u0017J\b\u0010\u0002\u001a\u00020\u0003H&J\u0015\u0010\u0004\u001a\u00020\u0005H&\u00f8\u0001\u0000\u00f8\u0001\u0001\u00a2\u0006\u0004\b\u0006\u0010\u0007J\u0015\u0010\b\u001a\u00020\u0005H&\u00f8\u0001\u0000\u00f8\u0001\u0001\u00a2\u0006\u0004\b\t\u0010\u0007J\u001a\u0010\n\u001a\u00020\u00032\u0006\u0010\u000b\u001a\u00020\fH&\u00f8\u0001\u0001\u00a2\u0006\u0004\b\r\u0010\u000eJ\b\u0010\u000f\u001a\u00020\u0003H&J\"\u0010\u0010\u001a\u00020\u00032\u0006\u0010\u0011\u001a\u00020\u00052\u0006\u0010\u0012\u001a\u00020\u0005H&\u00f8\u0001\u0001\u00a2\u0006\u0004\b\u0013\u0010\u0014J\b\u0010\u0015\u001a\u00020\u0003H&J\b\u0010\u0016\u001a\u00020\u0003H&\u0082\u0002\u000b\n\u0002\b!\n\u0005\b\u00a1\u001e0\u0001\u00a8\u0006\u0018"}, d2 = {"Luniffi/grafito_ffi/CanvasRendererInterface;", "", "cleanup", "", "getHeight", "Lkotlin/UInt;", "getHeight-pVg5ArA", "()I", "getWidth", "getWidth-pVg5ArA", "initWithSurface", "surfacePtr", "Lkotlin/ULong;", "initWithSurface-VKZWuLQ", "(J)V", "renderFrame", "resize", "width", "height", "resize-feOb9K0", "(II)V", "startRenderLoop", "stopRenderLoop", "Companion", "app_debug"})
public abstract interface CanvasRendererInterface {
    @org.jetbrains.annotations.NotNull()
    public static final uniffi.grafito_ffi.CanvasRendererInterface.Companion Companion = null;
    
    public abstract void cleanup();
    
    public abstract void renderFrame();
    
    public abstract void startRenderLoop();
    
    public abstract void stopRenderLoop();
    
    @kotlin.Metadata(mv = {1, 9, 0}, k = 1, xi = 48, d1 = {"\u0000\f\n\u0002\u0018\u0002\n\u0002\u0010\u0000\n\u0002\b\u0002\b\u0086\u0003\u0018\u00002\u00020\u0001B\u0007\b\u0002\u00a2\u0006\u0002\u0010\u0002\u00a8\u0006\u0003"}, d2 = {"Luniffi/grafito_ffi/CanvasRendererInterface$Companion;", "", "()V", "app_debug"})
    public static final class Companion {
        
        private Companion() {
            super();
        }
    }
}