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

@kotlin.Metadata(mv = {1, 9, 0}, k = 1, xi = 48, d1 = {"\u0000\u0018\n\u0002\u0018\u0002\n\u0002\u0018\u0002\n\u0000\n\u0002\u0018\u0002\n\u0002\b\u0002\n\u0002\u0010\u0002\n\u0000\b\u0002\u0018\u00002\u00020\u0001B\r\u0012\u0006\u0010\u0002\u001a\u00020\u0003\u00a2\u0006\u0002\u0010\u0004J\b\u0010\u0005\u001a\u00020\u0006H\u0016R\u000e\u0010\u0002\u001a\u00020\u0003X\u0082\u0004\u00a2\u0006\u0002\n\u0000\u00a8\u0006\u0007"}, d2 = {"Luniffi/grafito_ffi/UniffiJnaCleanable;", "Luniffi/grafito_ffi/UniffiCleaner$Cleanable;", "cleanable", "Lcom/sun/jna/internal/Cleaner$Cleanable;", "(Lcom/sun/jna/internal/Cleaner$Cleanable;)V", "clean", "", "app_debug"})
final class UniffiJnaCleanable implements uniffi.grafito_ffi.UniffiCleaner.Cleanable {
    @org.jetbrains.annotations.NotNull()
    private final com.sun.jna.internal.Cleaner.Cleanable cleanable = null;
    
    public UniffiJnaCleanable(@org.jetbrains.annotations.NotNull()
    com.sun.jna.internal.Cleaner.Cleanable cleanable) {
        super();
    }
    
    @java.lang.Override()
    public void clean() {
    }
}