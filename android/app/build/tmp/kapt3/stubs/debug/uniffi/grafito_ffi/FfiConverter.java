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
 * The FfiConverter interface handles converter types to and from the FFI
 *
 * All implementing objects should be public to support external types.  When a
 * type is external we need to import it's FfiConverter.
 *
 * @suppress
 */
@kotlin.Metadata(mv = {1, 9, 0}, k = 1, xi = 48, d1 = {"\u0000.\n\u0002\u0018\u0002\n\u0002\b\u0002\n\u0002\u0010\u0000\n\u0000\n\u0002\u0018\u0002\n\u0002\b\u0007\n\u0002\u0018\u0002\n\u0002\b\u0006\n\u0002\u0018\u0002\n\u0002\b\u0002\n\u0002\u0010\u0002\n\u0002\b\u0002\bf\u0018\u0000*\u0004\b\u0000\u0010\u0001*\u0004\b\u0001\u0010\u00022\u00020\u0003J\u001d\u0010\u0004\u001a\u00020\u00052\u0006\u0010\u0006\u001a\u00028\u0000H&\u00f8\u0001\u0000\u00f8\u0001\u0001\u00a2\u0006\u0004\b\u0007\u0010\bJ\u0015\u0010\t\u001a\u00028\u00002\u0006\u0010\u0006\u001a\u00028\u0001H&\u00a2\u0006\u0002\u0010\nJ\u0015\u0010\u000b\u001a\u00028\u00002\u0006\u0010\f\u001a\u00020\rH\u0016\u00a2\u0006\u0002\u0010\u000eJ\u0015\u0010\u000f\u001a\u00028\u00012\u0006\u0010\u0006\u001a\u00028\u0000H&\u00a2\u0006\u0002\u0010\nJ\u0015\u0010\u0010\u001a\u00020\r2\u0006\u0010\u0006\u001a\u00028\u0000H\u0016\u00a2\u0006\u0002\u0010\u0011J\u0015\u0010\u0012\u001a\u00028\u00002\u0006\u0010\u0013\u001a\u00020\u0014H&\u00a2\u0006\u0002\u0010\u0015J\u001d\u0010\u0016\u001a\u00020\u00172\u0006\u0010\u0006\u001a\u00028\u00002\u0006\u0010\u0013\u001a\u00020\u0014H&\u00a2\u0006\u0002\u0010\u0018\u0082\u0002\u000b\n\u0002\b!\n\u0005\b\u00a1\u001e0\u0001\u00a8\u0006\u0019"}, d2 = {"Luniffi/grafito_ffi/FfiConverter;", "KotlinType", "FfiType", "", "allocationSize", "Lkotlin/ULong;", "value", "allocationSize-I7RO_PI", "(Ljava/lang/Object;)J", "lift", "(Ljava/lang/Object;)Ljava/lang/Object;", "liftFromRustBuffer", "rbuf", "Luniffi/grafito_ffi/RustBuffer$ByValue;", "(Luniffi/grafito_ffi/RustBuffer$ByValue;)Ljava/lang/Object;", "lower", "lowerIntoRustBuffer", "(Ljava/lang/Object;)Luniffi/grafito_ffi/RustBuffer$ByValue;", "read", "buf", "Ljava/nio/ByteBuffer;", "(Ljava/nio/ByteBuffer;)Ljava/lang/Object;", "write", "", "(Ljava/lang/Object;Ljava/nio/ByteBuffer;)V", "app_debug"})
public abstract interface FfiConverter<KotlinType extends java.lang.Object, FfiType extends java.lang.Object> {
    
    public abstract KotlinType lift(FfiType value);
    
    public abstract FfiType lower(KotlinType value);
    
    public abstract KotlinType read(@org.jetbrains.annotations.NotNull()
    java.nio.ByteBuffer buf);
    
    public abstract void write(KotlinType value, @org.jetbrains.annotations.NotNull()
    java.nio.ByteBuffer buf);
    
    @org.jetbrains.annotations.NotNull()
    public abstract uniffi.grafito_ffi.RustBuffer.ByValue lowerIntoRustBuffer(KotlinType value);
    
    public abstract KotlinType liftFromRustBuffer(@org.jetbrains.annotations.NotNull()
    uniffi.grafito_ffi.RustBuffer.ByValue rbuf);
    
    /**
     * The FfiConverter interface handles converter types to and from the FFI
     *
     * All implementing objects should be public to support external types.  When a
     * type is external we need to import it's FfiConverter.
     *
     * @suppress
     */
    @kotlin.Metadata(mv = {1, 9, 0}, k = 3, xi = 48)
    public static final class DefaultImpls {
        
        @org.jetbrains.annotations.NotNull()
        public static <KotlinType extends java.lang.Object, FfiType extends java.lang.Object>uniffi.grafito_ffi.RustBuffer.ByValue lowerIntoRustBuffer(@org.jetbrains.annotations.NotNull()
        uniffi.grafito_ffi.FfiConverter<KotlinType, FfiType> $this, KotlinType value) {
            return null;
        }
        
        public static <KotlinType extends java.lang.Object, FfiType extends java.lang.Object>KotlinType liftFromRustBuffer(@org.jetbrains.annotations.NotNull()
        uniffi.grafito_ffi.FfiConverter<KotlinType, FfiType> $this, @org.jetbrains.annotations.NotNull()
        uniffi.grafito_ffi.RustBuffer.ByValue rbuf) {
            return null;
        }
    }
}