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
@kotlin.Metadata(mv = {1, 9, 0}, k = 1, xi = 48, d1 = {"\u0000$\n\u0002\u0018\u0002\n\u0002\u0018\u0002\n\u0002\u0010\u000e\n\u0002\b\u0002\n\u0002\u0018\u0002\n\u0002\b\u0005\n\u0002\u0018\u0002\n\u0000\n\u0002\u0010\u0002\n\u0000\b\u00c6\u0002\u0018\u00002\n\u0012\u0006\u0012\u0004\u0018\u00010\u00020\u0001B\u0007\b\u0002\u00a2\u0006\u0002\u0010\u0003J\u001f\u0010\u0004\u001a\u00020\u00052\b\u0010\u0006\u001a\u0004\u0018\u00010\u0002H\u0016\u00f8\u0001\u0000\u00f8\u0001\u0001\u00a2\u0006\u0004\b\u0007\u0010\bJ\u0012\u0010\t\u001a\u0004\u0018\u00010\u00022\u0006\u0010\n\u001a\u00020\u000bH\u0016J\u001a\u0010\f\u001a\u00020\r2\b\u0010\u0006\u001a\u0004\u0018\u00010\u00022\u0006\u0010\n\u001a\u00020\u000bH\u0016\u0082\u0002\u000b\n\u0002\b!\n\u0005\b\u00a1\u001e0\u0001\u00a8\u0006\u000e"}, d2 = {"Luniffi/grafito_ffi/FfiConverterOptionalString;", "Luniffi/grafito_ffi/FfiConverterRustBuffer;", "", "()V", "allocationSize", "Lkotlin/ULong;", "value", "allocationSize-I7RO_PI", "(Ljava/lang/String;)J", "read", "buf", "Ljava/nio/ByteBuffer;", "write", "", "app_debug"})
public final class FfiConverterOptionalString implements uniffi.grafito_ffi.FfiConverterRustBuffer<java.lang.String> {
    @org.jetbrains.annotations.NotNull()
    public static final uniffi.grafito_ffi.FfiConverterOptionalString INSTANCE = null;
    
    private FfiConverterOptionalString() {
        super();
    }
    
    @java.lang.Override()
    @org.jetbrains.annotations.Nullable()
    public java.lang.String read(@org.jetbrains.annotations.NotNull()
    java.nio.ByteBuffer buf) {
        return null;
    }
    
    @java.lang.Override()
    public void write(@org.jetbrains.annotations.Nullable()
    java.lang.String value, @org.jetbrains.annotations.NotNull()
    java.nio.ByteBuffer buf) {
    }
    
    @java.lang.Override()
    @org.jetbrains.annotations.Nullable()
    public java.lang.String lift(@org.jetbrains.annotations.NotNull()
    uniffi.grafito_ffi.RustBuffer.ByValue value) {
        return null;
    }
    
    @java.lang.Override()
    @org.jetbrains.annotations.Nullable()
    public java.lang.String liftFromRustBuffer(@org.jetbrains.annotations.NotNull()
    uniffi.grafito_ffi.RustBuffer.ByValue rbuf) {
        return null;
    }
    
    @java.lang.Override()
    @org.jetbrains.annotations.NotNull()
    public uniffi.grafito_ffi.RustBuffer.ByValue lower(@org.jetbrains.annotations.Nullable()
    java.lang.String value) {
        return null;
    }
    
    @java.lang.Override()
    @org.jetbrains.annotations.NotNull()
    public uniffi.grafito_ffi.RustBuffer.ByValue lowerIntoRustBuffer(@org.jetbrains.annotations.Nullable()
    java.lang.String value) {
        return null;
    }
}