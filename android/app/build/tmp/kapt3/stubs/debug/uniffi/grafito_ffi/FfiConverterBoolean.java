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
@kotlin.Metadata(mv = {1, 9, 0}, k = 1, xi = 48, d1 = {"\u0000*\n\u0002\u0018\u0002\n\u0002\u0018\u0002\n\u0002\u0010\u000b\n\u0002\u0010\u0005\n\u0002\b\u0002\n\u0002\u0018\u0002\n\u0002\b\t\n\u0002\u0018\u0002\n\u0002\b\u0002\n\u0002\u0010\u0002\n\u0000\b\u00c6\u0002\u0018\u00002\u000e\u0012\u0004\u0012\u00020\u0002\u0012\u0004\u0012\u00020\u00030\u0001B\u0007\b\u0002\u00a2\u0006\u0002\u0010\u0004J\u001d\u0010\u0005\u001a\u00020\u00062\u0006\u0010\u0007\u001a\u00020\u0002H\u0016\u00f8\u0001\u0000\u00f8\u0001\u0001\u00a2\u0006\u0004\b\b\u0010\tJ\u0015\u0010\n\u001a\u00020\u00022\u0006\u0010\u0007\u001a\u00020\u0003H\u0016\u00a2\u0006\u0002\u0010\u000bJ\u0015\u0010\f\u001a\u00020\u00032\u0006\u0010\u0007\u001a\u00020\u0002H\u0016\u00a2\u0006\u0002\u0010\rJ\u0015\u0010\u000e\u001a\u00020\u00022\u0006\u0010\u000f\u001a\u00020\u0010H\u0016\u00a2\u0006\u0002\u0010\u0011J\u0018\u0010\u0012\u001a\u00020\u00132\u0006\u0010\u0007\u001a\u00020\u00022\u0006\u0010\u000f\u001a\u00020\u0010H\u0016\u0082\u0002\u000b\n\u0002\b!\n\u0005\b\u00a1\u001e0\u0001\u00a8\u0006\u0014"}, d2 = {"Luniffi/grafito_ffi/FfiConverterBoolean;", "Luniffi/grafito_ffi/FfiConverter;", "", "", "()V", "allocationSize", "Lkotlin/ULong;", "value", "allocationSize-I7RO_PI", "(Z)J", "lift", "(B)Ljava/lang/Boolean;", "lower", "(Z)Ljava/lang/Byte;", "read", "buf", "Ljava/nio/ByteBuffer;", "(Ljava/nio/ByteBuffer;)Ljava/lang/Boolean;", "write", "", "app_debug"})
public final class FfiConverterBoolean implements uniffi.grafito_ffi.FfiConverter<java.lang.Boolean, java.lang.Byte> {
    @org.jetbrains.annotations.NotNull()
    public static final uniffi.grafito_ffi.FfiConverterBoolean INSTANCE = null;
    
    private FfiConverterBoolean() {
        super();
    }
    
    @java.lang.Override()
    @org.jetbrains.annotations.NotNull()
    public java.lang.Boolean lift(byte value) {
        return null;
    }
    
    @java.lang.Override()
    @org.jetbrains.annotations.NotNull()
    public java.lang.Boolean read(@org.jetbrains.annotations.NotNull()
    java.nio.ByteBuffer buf) {
        return null;
    }
    
    @java.lang.Override()
    @org.jetbrains.annotations.NotNull()
    public java.lang.Byte lower(boolean value) {
        return null;
    }
    
    @java.lang.Override()
    public void write(boolean value, @org.jetbrains.annotations.NotNull()
    java.nio.ByteBuffer buf) {
    }
    
    @java.lang.Override()
    @org.jetbrains.annotations.NotNull()
    public java.lang.Boolean liftFromRustBuffer(@org.jetbrains.annotations.NotNull()
    uniffi.grafito_ffi.RustBuffer.ByValue rbuf) {
        return null;
    }
    
    @java.lang.Override()
    @org.jetbrains.annotations.NotNull()
    public uniffi.grafito_ffi.RustBuffer.ByValue lowerIntoRustBuffer(boolean value) {
        return null;
    }
}