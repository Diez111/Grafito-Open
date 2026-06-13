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
 * UniffiRustCallStatusErrorHandler implementation for times when we don't expect a CALL_ERROR
 *
 * @suppress
 */
@kotlin.Metadata(mv = {1, 9, 0}, k = 1, xi = 48, d1 = {"\u0000\u0016\n\u0002\u0018\u0002\n\u0002\u0018\u0002\n\u0002\u0018\u0002\n\u0002\b\u0003\n\u0002\u0018\u0002\n\u0000\b\u00c6\u0002\u0018\u00002\b\u0012\u0004\u0012\u00020\u00020\u0001B\u0007\b\u0002\u00a2\u0006\u0002\u0010\u0003J\u0010\u0010\u0004\u001a\u00020\u00022\u0006\u0010\u0005\u001a\u00020\u0006H\u0016\u00a8\u0006\u0007"}, d2 = {"Luniffi/grafito_ffi/UniffiNullRustCallStatusErrorHandler;", "Luniffi/grafito_ffi/UniffiRustCallStatusErrorHandler;", "Luniffi/grafito_ffi/InternalException;", "()V", "lift", "error_buf", "Luniffi/grafito_ffi/RustBuffer$ByValue;", "app_debug"})
public final class UniffiNullRustCallStatusErrorHandler implements uniffi.grafito_ffi.UniffiRustCallStatusErrorHandler<uniffi.grafito_ffi.InternalException> {
    @org.jetbrains.annotations.NotNull()
    public static final uniffi.grafito_ffi.UniffiNullRustCallStatusErrorHandler INSTANCE = null;
    
    private UniffiNullRustCallStatusErrorHandler() {
        super();
    }
    
    @java.lang.Override()
    @org.jetbrains.annotations.NotNull()
    public uniffi.grafito_ffi.InternalException lift(@org.jetbrains.annotations.NotNull()
    uniffi.grafito_ffi.RustBuffer.ByValue error_buf) {
        return null;
    }
}