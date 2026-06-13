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
 * FfiConverter that uses `RustBuffer` as the FfiType
 *
 * @suppress
 */
@kotlin.Metadata(mv = {1, 9, 0}, k = 1, xi = 48, d1 = {"\u0000\u0012\n\u0002\u0018\u0002\n\u0000\n\u0002\u0018\u0002\n\u0002\u0018\u0002\n\u0002\b\u0006\bf\u0018\u0000*\u0004\b\u0000\u0010\u00012\u000e\u0012\u0004\u0012\u0002H\u0001\u0012\u0004\u0012\u00020\u00030\u0002J\u0015\u0010\u0004\u001a\u00028\u00002\u0006\u0010\u0005\u001a\u00020\u0003H\u0016\u00a2\u0006\u0002\u0010\u0006J\u0015\u0010\u0007\u001a\u00020\u00032\u0006\u0010\u0005\u001a\u00028\u0000H\u0016\u00a2\u0006\u0002\u0010\b\u00a8\u0006\t"}, d2 = {"Luniffi/grafito_ffi/FfiConverterRustBuffer;", "KotlinType", "Luniffi/grafito_ffi/FfiConverter;", "Luniffi/grafito_ffi/RustBuffer$ByValue;", "lift", "value", "(Luniffi/grafito_ffi/RustBuffer$ByValue;)Ljava/lang/Object;", "lower", "(Ljava/lang/Object;)Luniffi/grafito_ffi/RustBuffer$ByValue;", "app_debug"})
public abstract interface FfiConverterRustBuffer<KotlinType extends java.lang.Object> extends uniffi.grafito_ffi.FfiConverter<KotlinType, uniffi.grafito_ffi.RustBuffer.ByValue> {
    
    @java.lang.Override()
    public abstract KotlinType lift(@org.jetbrains.annotations.NotNull()
    uniffi.grafito_ffi.RustBuffer.ByValue value);
    
    @java.lang.Override()
    @org.jetbrains.annotations.NotNull()
    public abstract uniffi.grafito_ffi.RustBuffer.ByValue lower(KotlinType value);
    
    /**
     * FfiConverter that uses `RustBuffer` as the FfiType
     *
     * @suppress
     */
    @kotlin.Metadata(mv = {1, 9, 0}, k = 3, xi = 48)
    public static final class DefaultImpls {
        
        /**
         * FfiConverter that uses `RustBuffer` as the FfiType
         *
         * @suppress
         */
        public static <KotlinType extends java.lang.Object>KotlinType liftFromRustBuffer(@org.jetbrains.annotations.NotNull()
        uniffi.grafito_ffi.FfiConverterRustBuffer<KotlinType> $this, @org.jetbrains.annotations.NotNull()
        uniffi.grafito_ffi.RustBuffer.ByValue rbuf) {
            return null;
        }
        
        /**
         * FfiConverter that uses `RustBuffer` as the FfiType
         *
         * @suppress
         */
        @org.jetbrains.annotations.NotNull()
        public static <KotlinType extends java.lang.Object>uniffi.grafito_ffi.RustBuffer.ByValue lowerIntoRustBuffer(@org.jetbrains.annotations.NotNull()
        uniffi.grafito_ffi.FfiConverterRustBuffer<KotlinType> $this, KotlinType value) {
            return null;
        }
        
        public static <KotlinType extends java.lang.Object>KotlinType lift(@org.jetbrains.annotations.NotNull()
        uniffi.grafito_ffi.FfiConverterRustBuffer<KotlinType> $this, @org.jetbrains.annotations.NotNull()
        uniffi.grafito_ffi.RustBuffer.ByValue value) {
            return null;
        }
        
        @org.jetbrains.annotations.NotNull()
        public static <KotlinType extends java.lang.Object>uniffi.grafito_ffi.RustBuffer.ByValue lower(@org.jetbrains.annotations.NotNull()
        uniffi.grafito_ffi.FfiConverterRustBuffer<KotlinType> $this, KotlinType value) {
            return null;
        }
    }
}