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

/**
 * @suppress
 */
@kotlin.Metadata(mv = {1, 9, 0}, k = 1, xi = 48, d1 = {"\u0000(\n\u0002\u0018\u0002\n\u0002\u0018\u0002\n\u0002\u0010\u0006\n\u0002\b\u0002\n\u0002\u0018\u0002\n\u0002\b\u0005\n\u0002\u0018\u0002\n\u0002\b\u0002\n\u0002\u0010\u0002\n\u0002\b\u0002\b\u00c6\u0002\u0018\u00002\n\u0012\u0006\u0012\u0004\u0018\u00010\u00020\u0001B\u0007\b\u0002\u00a2\u0006\u0002\u0010\u0003J\u001f\u0010\u0004\u001a\u00020\u00052\b\u0010\u0006\u001a\u0004\u0018\u00010\u0002H\u0016\u00f8\u0001\u0000\u00f8\u0001\u0001\u00a2\u0006\u0004\b\u0007\u0010\bJ\u0017\u0010\t\u001a\u0004\u0018\u00010\u00022\u0006\u0010\n\u001a\u00020\u000bH\u0016\u00a2\u0006\u0002\u0010\fJ\u001f\u0010\r\u001a\u00020\u000e2\b\u0010\u0006\u001a\u0004\u0018\u00010\u00022\u0006\u0010\n\u001a\u00020\u000bH\u0016\u00a2\u0006\u0002\u0010\u000f\u0082\u0002\u000b\n\u0002\b!\n\u0005\b\u00a1\u001e0\u0001\u00a8\u0006\u0010"}, d2 = {"Luniffi/grafito_ffi/FfiConverterOptionalDouble;", "Luniffi/grafito_ffi/FfiConverterRustBuffer;", "", "()V", "allocationSize", "Lkotlin/ULong;", "value", "allocationSize-I7RO_PI", "(Ljava/lang/Double;)J", "read", "buf", "Ljava/nio/ByteBuffer;", "(Ljava/nio/ByteBuffer;)Ljava/lang/Double;", "write", "", "(Ljava/lang/Double;Ljava/nio/ByteBuffer;)V", "app_debug"})
public final class FfiConverterOptionalDouble implements uniffi.grafito_ffi.FfiConverterRustBuffer<java.lang.Double> {
    @org.jetbrains.annotations.NotNull()
    public static final uniffi.grafito_ffi.FfiConverterOptionalDouble INSTANCE = null;
    
    private FfiConverterOptionalDouble() {
        super();
    }
    
    @java.lang.Override()
    @org.jetbrains.annotations.Nullable()
    public java.lang.Double read(@org.jetbrains.annotations.NotNull()
    java.nio.ByteBuffer buf) {
        return null;
    }
    
    @java.lang.Override()
    public void write(@org.jetbrains.annotations.Nullable()
    java.lang.Double value, @org.jetbrains.annotations.NotNull()
    java.nio.ByteBuffer buf) {
    }
    
    @java.lang.Override()
    @org.jetbrains.annotations.Nullable()
    public java.lang.Double lift(@org.jetbrains.annotations.NotNull()
    uniffi.grafito_ffi.RustBuffer.ByValue value) {
        return null;
    }
    
    @java.lang.Override()
    @org.jetbrains.annotations.Nullable()
    public java.lang.Double liftFromRustBuffer(@org.jetbrains.annotations.NotNull()
    uniffi.grafito_ffi.RustBuffer.ByValue rbuf) {
        return null;
    }
    
    @java.lang.Override()
    @org.jetbrains.annotations.NotNull()
    public uniffi.grafito_ffi.RustBuffer.ByValue lower(@org.jetbrains.annotations.Nullable()
    java.lang.Double value) {
        return null;
    }
    
    @java.lang.Override()
    @org.jetbrains.annotations.NotNull()
    public uniffi.grafito_ffi.RustBuffer.ByValue lowerIntoRustBuffer(@org.jetbrains.annotations.Nullable()
    java.lang.Double value) {
        return null;
    }
}