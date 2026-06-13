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
@kotlin.Metadata(mv = {1, 9, 0}, k = 1, xi = 48, d1 = {"\u0000&\n\u0002\u0018\u0002\n\u0002\u0018\u0002\n\u0002\u0010\u0007\n\u0002\b\u0002\n\u0002\u0018\u0002\n\u0002\b\b\n\u0002\u0018\u0002\n\u0002\b\u0002\n\u0002\u0010\u0002\n\u0000\b\u00c6\u0002\u0018\u00002\u000e\u0012\u0004\u0012\u00020\u0002\u0012\u0004\u0012\u00020\u00020\u0001B\u0007\b\u0002\u00a2\u0006\u0002\u0010\u0003J\u001d\u0010\u0004\u001a\u00020\u00052\u0006\u0010\u0006\u001a\u00020\u0002H\u0016\u00f8\u0001\u0000\u00f8\u0001\u0001\u00a2\u0006\u0004\b\u0007\u0010\bJ\u0015\u0010\t\u001a\u00020\u00022\u0006\u0010\u0006\u001a\u00020\u0002H\u0016\u00a2\u0006\u0002\u0010\nJ\u0015\u0010\u000b\u001a\u00020\u00022\u0006\u0010\u0006\u001a\u00020\u0002H\u0016\u00a2\u0006\u0002\u0010\nJ\u0015\u0010\f\u001a\u00020\u00022\u0006\u0010\r\u001a\u00020\u000eH\u0016\u00a2\u0006\u0002\u0010\u000fJ\u0018\u0010\u0010\u001a\u00020\u00112\u0006\u0010\u0006\u001a\u00020\u00022\u0006\u0010\r\u001a\u00020\u000eH\u0016\u0082\u0002\u000b\n\u0002\b!\n\u0005\b\u00a1\u001e0\u0001\u00a8\u0006\u0012"}, d2 = {"Luniffi/grafito_ffi/FfiConverterFloat;", "Luniffi/grafito_ffi/FfiConverter;", "", "()V", "allocationSize", "Lkotlin/ULong;", "value", "allocationSize-I7RO_PI", "(F)J", "lift", "(F)Ljava/lang/Float;", "lower", "read", "buf", "Ljava/nio/ByteBuffer;", "(Ljava/nio/ByteBuffer;)Ljava/lang/Float;", "write", "", "app_debug"})
public final class FfiConverterFloat implements uniffi.grafito_ffi.FfiConverter<java.lang.Float, java.lang.Float> {
    @org.jetbrains.annotations.NotNull()
    public static final uniffi.grafito_ffi.FfiConverterFloat INSTANCE = null;
    
    private FfiConverterFloat() {
        super();
    }
    
    @java.lang.Override()
    @org.jetbrains.annotations.NotNull()
    public java.lang.Float lift(float value) {
        return null;
    }
    
    @java.lang.Override()
    @org.jetbrains.annotations.NotNull()
    public java.lang.Float read(@org.jetbrains.annotations.NotNull()
    java.nio.ByteBuffer buf) {
        return null;
    }
    
    @java.lang.Override()
    @org.jetbrains.annotations.NotNull()
    public java.lang.Float lower(float value) {
        return null;
    }
    
    @java.lang.Override()
    public void write(float value, @org.jetbrains.annotations.NotNull()
    java.nio.ByteBuffer buf) {
    }
    
    @java.lang.Override()
    @org.jetbrains.annotations.NotNull()
    public java.lang.Float liftFromRustBuffer(@org.jetbrains.annotations.NotNull()
    uniffi.grafito_ffi.RustBuffer.ByValue rbuf) {
        return null;
    }
    
    @java.lang.Override()
    @org.jetbrains.annotations.NotNull()
    public uniffi.grafito_ffi.RustBuffer.ByValue lowerIntoRustBuffer(float value) {
        return null;
    }
}