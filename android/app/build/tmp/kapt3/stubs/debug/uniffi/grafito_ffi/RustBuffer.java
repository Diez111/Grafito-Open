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
@com.sun.jna.Structure.FieldOrder(value = {"capacity", "len", "data"})
@kotlin.Metadata(mv = {1, 9, 0}, k = 1, xi = 48, d1 = {"\u0000(\n\u0002\u0018\u0002\n\u0002\u0018\u0002\n\u0002\b\u0002\n\u0002\u0010\t\n\u0000\n\u0002\u0018\u0002\n\u0002\b\u0002\n\u0002\u0018\u0002\n\u0000\n\u0002\u0010\u0002\n\u0002\b\u0006\b\u0017\u0018\u0000 \u00102\u00020\u0001:\u0003\u000e\u000f\u0010B\u0005\u00a2\u0006\u0002\u0010\u0002J\b\u0010\b\u001a\u0004\u0018\u00010\tJ\u0015\u0010\n\u001a\u00020\u000b2\u0006\u0010\f\u001a\u00020\u0000H\u0000\u00a2\u0006\u0002\b\rR\u0012\u0010\u0003\u001a\u00020\u00048\u0006@\u0006X\u0087\u000e\u00a2\u0006\u0002\n\u0000R\u0014\u0010\u0005\u001a\u0004\u0018\u00010\u00068\u0006@\u0006X\u0087\u000e\u00a2\u0006\u0002\n\u0000R\u0012\u0010\u0007\u001a\u00020\u00048\u0006@\u0006X\u0087\u000e\u00a2\u0006\u0002\n\u0000\u00a8\u0006\u0011"}, d2 = {"Luniffi/grafito_ffi/RustBuffer;", "Lcom/sun/jna/Structure;", "()V", "capacity", "", "data", "Lcom/sun/jna/Pointer;", "len", "asByteBuffer", "Ljava/nio/ByteBuffer;", "setValue", "", "other", "setValue$app_debug", "ByReference", "ByValue", "Companion", "app_debug"})
public class RustBuffer extends com.sun.jna.Structure {
    @kotlin.jvm.JvmField()
    public long capacity = 0L;
    @kotlin.jvm.JvmField()
    public long len = 0L;
    @kotlin.jvm.JvmField()
    @org.jetbrains.annotations.Nullable()
    public com.sun.jna.Pointer data;
    @org.jetbrains.annotations.NotNull()
    public static final uniffi.grafito_ffi.RustBuffer.Companion Companion = null;
    
    public RustBuffer() {
        super();
    }
    
    public final void setValue$app_debug(@org.jetbrains.annotations.NotNull()
    uniffi.grafito_ffi.RustBuffer other) {
    }
    
    @kotlin.Suppress(names = {"TooGenericExceptionThrown"})
    @org.jetbrains.annotations.Nullable()
    public final java.nio.ByteBuffer asByteBuffer() {
        return null;
    }
    
    @kotlin.Metadata(mv = {1, 9, 0}, k = 1, xi = 48, d1 = {"\u0000\u0010\n\u0002\u0018\u0002\n\u0002\u0018\u0002\n\u0002\u0018\u0002\n\u0002\b\u0002\u0018\u00002\u00020\u00012\u00020\u0002B\u0005\u00a2\u0006\u0002\u0010\u0003\u00a8\u0006\u0004"}, d2 = {"Luniffi/grafito_ffi/RustBuffer$ByReference;", "Luniffi/grafito_ffi/RustBuffer;", "Lcom/sun/jna/Structure$ByReference;", "()V", "app_debug"})
    public static final class ByReference extends uniffi.grafito_ffi.RustBuffer implements com.sun.jna.Structure.ByReference {
        
        public ByReference() {
            super();
        }
    }
    
    @kotlin.Metadata(mv = {1, 9, 0}, k = 1, xi = 48, d1 = {"\u0000\u0010\n\u0002\u0018\u0002\n\u0002\u0018\u0002\n\u0002\u0018\u0002\n\u0002\b\u0002\u0018\u00002\u00020\u00012\u00020\u0002B\u0005\u00a2\u0006\u0002\u0010\u0003\u00a8\u0006\u0004"}, d2 = {"Luniffi/grafito_ffi/RustBuffer$ByValue;", "Luniffi/grafito_ffi/RustBuffer;", "Lcom/sun/jna/Structure$ByValue;", "()V", "app_debug"})
    public static final class ByValue extends uniffi.grafito_ffi.RustBuffer implements com.sun.jna.Structure.ByValue {
        
        public ByValue() {
            super();
        }
    }
    
    @kotlin.Metadata(mv = {1, 9, 0}, k = 1, xi = 48, d1 = {"\u0000*\n\u0002\u0018\u0002\n\u0002\u0010\u0000\n\u0002\b\u0002\n\u0002\u0018\u0002\n\u0000\n\u0002\u0018\u0002\n\u0002\b\u0006\n\u0002\u0018\u0002\n\u0002\b\u0003\n\u0002\u0010\u0002\n\u0002\b\u0003\b\u0086\u0003\u0018\u00002\u00020\u0001B\u0007\b\u0002\u00a2\u0006\u0002\u0010\u0002J\u001c\u0010\u0003\u001a\u00020\u00042\b\b\u0002\u0010\u0005\u001a\u00020\u0006H\u0000\u00f8\u0001\u0000\u00a2\u0006\u0004\b\u0007\u0010\bJ,\u0010\t\u001a\u00020\u00042\u0006\u0010\n\u001a\u00020\u00062\u0006\u0010\u000b\u001a\u00020\u00062\b\u0010\f\u001a\u0004\u0018\u00010\rH\u0000\u00f8\u0001\u0000\u00a2\u0006\u0004\b\u000e\u0010\u000fJ\u0015\u0010\u0010\u001a\u00020\u00112\u0006\u0010\u0012\u001a\u00020\u0004H\u0000\u00a2\u0006\u0002\b\u0013\u0082\u0002\u0007\n\u0005\b\u00a1\u001e0\u0001\u00a8\u0006\u0014"}, d2 = {"Luniffi/grafito_ffi/RustBuffer$Companion;", "", "()V", "alloc", "Luniffi/grafito_ffi/RustBuffer$ByValue;", "size", "Lkotlin/ULong;", "alloc-VKZWuLQ$app_debug", "(J)Luniffi/grafito_ffi/RustBuffer$ByValue;", "create", "capacity", "len", "data", "Lcom/sun/jna/Pointer;", "create-twO9MuI$app_debug", "(JJLcom/sun/jna/Pointer;)Luniffi/grafito_ffi/RustBuffer$ByValue;", "free", "", "buf", "free$app_debug", "app_debug"})
    public static final class Companion {
        
        private Companion() {
            super();
        }
        
        public final void free$app_debug(@org.jetbrains.annotations.NotNull()
        uniffi.grafito_ffi.RustBuffer.ByValue buf) {
        }
    }
}