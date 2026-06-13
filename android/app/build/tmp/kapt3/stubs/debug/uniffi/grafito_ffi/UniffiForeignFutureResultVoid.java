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

@com.sun.jna.Structure.FieldOrder(value = {"callStatus"})
@kotlin.Metadata(mv = {1, 9, 0}, k = 1, xi = 48, d1 = {"\u0000\u001a\n\u0002\u0018\u0002\n\u0002\u0018\u0002\n\u0000\n\u0002\u0018\u0002\n\u0002\b\u0002\n\u0002\u0010\u0002\n\u0002\b\u0004\b\u0011\u0018\u00002\u00020\u0001:\u0001\tB\u000f\u0012\b\b\u0002\u0010\u0002\u001a\u00020\u0003\u00a2\u0006\u0002\u0010\u0004J\u0015\u0010\u0005\u001a\u00020\u00062\u0006\u0010\u0007\u001a\u00020\u0000H\u0000\u00a2\u0006\u0002\b\bR\u0012\u0010\u0002\u001a\u00020\u00038\u0000@\u0000X\u0081\u000e\u00a2\u0006\u0002\n\u0000\u00a8\u0006\n"}, d2 = {"Luniffi/grafito_ffi/UniffiForeignFutureResultVoid;", "Lcom/sun/jna/Structure;", "callStatus", "Luniffi/grafito_ffi/UniffiRustCallStatus$ByValue;", "(Luniffi/grafito_ffi/UniffiRustCallStatus$ByValue;)V", "uniffiSetValue", "", "other", "uniffiSetValue$app_debug", "UniffiByValue", "app_debug"})
public class UniffiForeignFutureResultVoid extends com.sun.jna.Structure {
    @kotlin.jvm.JvmField()
    @org.jetbrains.annotations.NotNull()
    public uniffi.grafito_ffi.UniffiRustCallStatus.ByValue callStatus;
    
    public UniffiForeignFutureResultVoid(@org.jetbrains.annotations.NotNull()
    uniffi.grafito_ffi.UniffiRustCallStatus.ByValue callStatus) {
        super();
    }
    
    public final void uniffiSetValue$app_debug(@org.jetbrains.annotations.NotNull()
    uniffi.grafito_ffi.UniffiForeignFutureResultVoid other) {
    }
    
    public UniffiForeignFutureResultVoid() {
        super();
    }
    
    @kotlin.Metadata(mv = {1, 9, 0}, k = 1, xi = 48, d1 = {"\u0000\u0016\n\u0002\u0018\u0002\n\u0002\u0018\u0002\n\u0002\u0018\u0002\n\u0000\n\u0002\u0018\u0002\n\u0002\b\u0002\u0018\u00002\u00020\u00012\u00020\u0002B\u000f\u0012\b\b\u0002\u0010\u0003\u001a\u00020\u0004\u00a2\u0006\u0002\u0010\u0005\u00a8\u0006\u0006"}, d2 = {"Luniffi/grafito_ffi/UniffiForeignFutureResultVoid$UniffiByValue;", "Luniffi/grafito_ffi/UniffiForeignFutureResultVoid;", "Lcom/sun/jna/Structure$ByValue;", "callStatus", "Luniffi/grafito_ffi/UniffiRustCallStatus$ByValue;", "(Luniffi/grafito_ffi/UniffiRustCallStatus$ByValue;)V", "app_debug"})
    public static final class UniffiByValue extends uniffi.grafito_ffi.UniffiForeignFutureResultVoid implements com.sun.jna.Structure.ByValue {
        
        public UniffiByValue(@org.jetbrains.annotations.NotNull()
        uniffi.grafito_ffi.UniffiRustCallStatus.ByValue callStatus) {
            super(null);
        }
        
        public UniffiByValue() {
            super(null);
        }
    }
}