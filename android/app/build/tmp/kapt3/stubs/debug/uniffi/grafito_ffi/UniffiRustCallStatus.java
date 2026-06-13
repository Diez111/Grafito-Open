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

@com.sun.jna.Structure.FieldOrder(value = {"code", "error_buf"})
@kotlin.Metadata(mv = {1, 9, 0}, k = 1, xi = 48, d1 = {"\u0000 \n\u0002\u0018\u0002\n\u0002\u0018\u0002\n\u0002\b\u0002\n\u0002\u0010\u0005\n\u0000\n\u0002\u0018\u0002\n\u0000\n\u0002\u0010\u000b\n\u0002\b\u0005\b\u0011\u0018\u0000 \f2\u00020\u0001:\u0002\u000b\fB\u0005\u00a2\u0006\u0002\u0010\u0002J\u0006\u0010\u0007\u001a\u00020\bJ\u0006\u0010\t\u001a\u00020\bJ\u0006\u0010\n\u001a\u00020\bR\u0012\u0010\u0003\u001a\u00020\u00048\u0006@\u0006X\u0087\u000e\u00a2\u0006\u0002\n\u0000R\u0012\u0010\u0005\u001a\u00020\u00068\u0006@\u0006X\u0087\u000e\u00a2\u0006\u0002\n\u0000\u00a8\u0006\r"}, d2 = {"Luniffi/grafito_ffi/UniffiRustCallStatus;", "Lcom/sun/jna/Structure;", "()V", "code", "", "error_buf", "Luniffi/grafito_ffi/RustBuffer$ByValue;", "isError", "", "isPanic", "isSuccess", "ByValue", "Companion", "app_debug"})
public class UniffiRustCallStatus extends com.sun.jna.Structure {
    @kotlin.jvm.JvmField()
    public byte code = (byte)0;
    @kotlin.jvm.JvmField()
    @org.jetbrains.annotations.NotNull()
    public uniffi.grafito_ffi.RustBuffer.ByValue error_buf;
    @org.jetbrains.annotations.NotNull()
    public static final uniffi.grafito_ffi.UniffiRustCallStatus.Companion Companion = null;
    
    public UniffiRustCallStatus() {
        super();
    }
    
    public final boolean isSuccess() {
        return false;
    }
    
    public final boolean isError() {
        return false;
    }
    
    public final boolean isPanic() {
        return false;
    }
    
    @kotlin.Metadata(mv = {1, 9, 0}, k = 1, xi = 48, d1 = {"\u0000\u0010\n\u0002\u0018\u0002\n\u0002\u0018\u0002\n\u0002\u0018\u0002\n\u0002\b\u0002\u0018\u00002\u00020\u00012\u00020\u0002B\u0005\u00a2\u0006\u0002\u0010\u0003\u00a8\u0006\u0004"}, d2 = {"Luniffi/grafito_ffi/UniffiRustCallStatus$ByValue;", "Luniffi/grafito_ffi/UniffiRustCallStatus;", "Lcom/sun/jna/Structure$ByValue;", "()V", "app_debug"})
    public static final class ByValue extends uniffi.grafito_ffi.UniffiRustCallStatus implements com.sun.jna.Structure.ByValue {
        
        public ByValue() {
            super();
        }
    }
    
    @kotlin.Metadata(mv = {1, 9, 0}, k = 1, xi = 48, d1 = {"\u0000\u001e\n\u0002\u0018\u0002\n\u0002\u0010\u0000\n\u0002\b\u0002\n\u0002\u0018\u0002\n\u0000\n\u0002\u0010\u0005\n\u0000\n\u0002\u0018\u0002\n\u0000\b\u0086\u0003\u0018\u00002\u00020\u0001B\u0007\b\u0002\u00a2\u0006\u0002\u0010\u0002J\u0016\u0010\u0003\u001a\u00020\u00042\u0006\u0010\u0005\u001a\u00020\u00062\u0006\u0010\u0007\u001a\u00020\b\u00a8\u0006\t"}, d2 = {"Luniffi/grafito_ffi/UniffiRustCallStatus$Companion;", "", "()V", "create", "Luniffi/grafito_ffi/UniffiRustCallStatus$ByValue;", "code", "", "errorBuf", "Luniffi/grafito_ffi/RustBuffer$ByValue;", "app_debug"})
    public static final class Companion {
        
        private Companion() {
            super();
        }
        
        @org.jetbrains.annotations.NotNull()
        public final uniffi.grafito_ffi.UniffiRustCallStatus.ByValue create(byte code, @org.jetbrains.annotations.NotNull()
        uniffi.grafito_ffi.RustBuffer.ByValue errorBuf) {
            return null;
        }
    }
}