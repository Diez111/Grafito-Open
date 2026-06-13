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

@kotlin.Metadata(mv = {1, 9, 0}, k = 1, xi = 48, d1 = {"\u0000\u001e\n\u0002\u0018\u0002\n\u0002\u0018\u0002\n\u0002\u0018\u0002\n\u0002\b\u0005\n\u0002\u0018\u0002\n\u0002\u0018\u0002\n\u0002\u0018\u0002\n\u0000\b6\u0018\u0000 \u00042\u00060\u0001j\u0002`\u0002:\u0004\u0004\u0005\u0006\u0007B\u0007\b\u0004\u00a2\u0006\u0002\u0010\u0003\u0082\u0001\u0003\b\t\n\u00a8\u0006\u000b"}, d2 = {"Luniffi/grafito_ffi/CanvasException;", "Ljava/lang/Exception;", "Lkotlin/Exception;", "()V", "ErrorHandler", "NotInitialized", "RenderException", "SurfaceException", "Luniffi/grafito_ffi/CanvasException$NotInitialized;", "Luniffi/grafito_ffi/CanvasException$RenderException;", "Luniffi/grafito_ffi/CanvasException$SurfaceException;", "app_debug"})
public abstract class CanvasException extends java.lang.Exception {
    @org.jetbrains.annotations.NotNull()
    public static final uniffi.grafito_ffi.CanvasException.ErrorHandler ErrorHandler = null;
    
    private CanvasException() {
        super();
    }
    
    @kotlin.Metadata(mv = {1, 9, 0}, k = 1, xi = 48, d1 = {"\u0000\u0016\n\u0002\u0018\u0002\n\u0002\u0018\u0002\n\u0002\u0018\u0002\n\u0002\b\u0003\n\u0002\u0018\u0002\n\u0000\b\u0086\u0003\u0018\u00002\b\u0012\u0004\u0012\u00020\u00020\u0001B\u0007\b\u0002\u00a2\u0006\u0002\u0010\u0003J\u0010\u0010\u0004\u001a\u00020\u00022\u0006\u0010\u0005\u001a\u00020\u0006H\u0016\u00a8\u0006\u0007"}, d2 = {"Luniffi/grafito_ffi/CanvasException$ErrorHandler;", "Luniffi/grafito_ffi/UniffiRustCallStatusErrorHandler;", "Luniffi/grafito_ffi/CanvasException;", "()V", "lift", "error_buf", "Luniffi/grafito_ffi/RustBuffer$ByValue;", "app_debug"})
    public static final class ErrorHandler implements uniffi.grafito_ffi.UniffiRustCallStatusErrorHandler<uniffi.grafito_ffi.CanvasException> {
        
        private ErrorHandler() {
            super();
        }
        
        @java.lang.Override()
        @org.jetbrains.annotations.NotNull()
        public uniffi.grafito_ffi.CanvasException lift(@org.jetbrains.annotations.NotNull()
        uniffi.grafito_ffi.RustBuffer.ByValue error_buf) {
            return null;
        }
    }
    
    @kotlin.Metadata(mv = {1, 9, 0}, k = 1, xi = 48, d1 = {"\u0000\u0014\n\u0002\u0018\u0002\n\u0002\u0018\u0002\n\u0002\b\u0002\n\u0002\u0010\u000e\n\u0002\b\u0003\u0018\u00002\u00020\u0001B\u0005\u00a2\u0006\u0002\u0010\u0002R\u0014\u0010\u0003\u001a\u00020\u00048VX\u0096\u0004\u00a2\u0006\u0006\u001a\u0004\b\u0005\u0010\u0006\u00a8\u0006\u0007"}, d2 = {"Luniffi/grafito_ffi/CanvasException$NotInitialized;", "Luniffi/grafito_ffi/CanvasException;", "()V", "message", "", "getMessage", "()Ljava/lang/String;", "app_debug"})
    public static final class NotInitialized extends uniffi.grafito_ffi.CanvasException {
        
        public NotInitialized() {
        }
        
        @java.lang.Override()
        @org.jetbrains.annotations.NotNull()
        public java.lang.String getMessage() {
            return null;
        }
    }
    
    @kotlin.Metadata(mv = {1, 9, 0}, k = 1, xi = 48, d1 = {"\u0000\u0012\n\u0002\u0018\u0002\n\u0002\u0018\u0002\n\u0000\n\u0002\u0010\u000e\n\u0002\b\u0006\u0018\u00002\u00020\u0001B\r\u0012\u0006\u0010\u0002\u001a\u00020\u0003\u00a2\u0006\u0002\u0010\u0004R\u0014\u0010\u0005\u001a\u00020\u00038VX\u0096\u0004\u00a2\u0006\u0006\u001a\u0004\b\u0006\u0010\u0007R\u0011\u0010\u0002\u001a\u00020\u0003\u00a2\u0006\b\n\u0000\u001a\u0004\b\b\u0010\u0007\u00a8\u0006\t"}, d2 = {"Luniffi/grafito_ffi/CanvasException$RenderException;", "Luniffi/grafito_ffi/CanvasException;", "v1", "", "(Ljava/lang/String;)V", "message", "getMessage", "()Ljava/lang/String;", "getV1", "app_debug"})
    public static final class RenderException extends uniffi.grafito_ffi.CanvasException {
        @org.jetbrains.annotations.NotNull()
        private final java.lang.String v1 = null;
        
        public RenderException(@org.jetbrains.annotations.NotNull()
        java.lang.String v1) {
        }
        
        @org.jetbrains.annotations.NotNull()
        public final java.lang.String getV1() {
            return null;
        }
        
        @java.lang.Override()
        @org.jetbrains.annotations.NotNull()
        public java.lang.String getMessage() {
            return null;
        }
    }
    
    @kotlin.Metadata(mv = {1, 9, 0}, k = 1, xi = 48, d1 = {"\u0000\u0012\n\u0002\u0018\u0002\n\u0002\u0018\u0002\n\u0000\n\u0002\u0010\u000e\n\u0002\b\u0006\u0018\u00002\u00020\u0001B\r\u0012\u0006\u0010\u0002\u001a\u00020\u0003\u00a2\u0006\u0002\u0010\u0004R\u0014\u0010\u0005\u001a\u00020\u00038VX\u0096\u0004\u00a2\u0006\u0006\u001a\u0004\b\u0006\u0010\u0007R\u0011\u0010\u0002\u001a\u00020\u0003\u00a2\u0006\b\n\u0000\u001a\u0004\b\b\u0010\u0007\u00a8\u0006\t"}, d2 = {"Luniffi/grafito_ffi/CanvasException$SurfaceException;", "Luniffi/grafito_ffi/CanvasException;", "v1", "", "(Ljava/lang/String;)V", "message", "getMessage", "()Ljava/lang/String;", "getV1", "app_debug"})
    public static final class SurfaceException extends uniffi.grafito_ffi.CanvasException {
        @org.jetbrains.annotations.NotNull()
        private final java.lang.String v1 = null;
        
        public SurfaceException(@org.jetbrains.annotations.NotNull()
        java.lang.String v1) {
        }
        
        @org.jetbrains.annotations.NotNull()
        public final java.lang.String getV1() {
            return null;
        }
        
        @java.lang.Override()
        @org.jetbrains.annotations.NotNull()
        public java.lang.String getMessage() {
            return null;
        }
    }
}