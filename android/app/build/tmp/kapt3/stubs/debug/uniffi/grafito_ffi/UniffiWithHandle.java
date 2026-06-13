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
 * Placeholder object used to signal that we're constructing an interface with a FFI handle.
 *
 * This is the first argument for interface constructors that input a raw handle. It exists is that
 * so we can avoid signature conflicts when an interface has a regular constructor than inputs a
 * Long.
 *
 * @suppress
 */
@kotlin.Metadata(mv = {1, 9, 0}, k = 1, xi = 48, d1 = {"\u0000\f\n\u0002\u0018\u0002\n\u0002\u0010\u0000\n\u0002\b\u0002\b\u00c6\u0002\u0018\u00002\u00020\u0001B\u0007\b\u0002\u00a2\u0006\u0002\u0010\u0002\u00a8\u0006\u0003"}, d2 = {"Luniffi/grafito_ffi/UniffiWithHandle;", "", "()V", "app_debug"})
public final class UniffiWithHandle {
    @org.jetbrains.annotations.NotNull()
    public static final uniffi.grafito_ffi.UniffiWithHandle INSTANCE = null;
    
    private UniffiWithHandle() {
        super();
    }
}