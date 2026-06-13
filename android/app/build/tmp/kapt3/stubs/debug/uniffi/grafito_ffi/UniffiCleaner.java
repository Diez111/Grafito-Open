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
 * The cleaner interface for Object finalization code to run.
 * This is the entry point to any implementation that we're using.
 *
 * The cleaner registers objects and returns cleanables, so now we are
 * defining a `UniffiCleaner` with a `UniffiClenaer.Cleanable` to abstract the
 * different implmentations available at compile time.
 *
 * @suppress
 */
@kotlin.Metadata(mv = {1, 9, 0}, k = 1, xi = 48, d1 = {"\u0000\u001a\n\u0002\u0018\u0002\n\u0002\u0010\u0000\n\u0000\n\u0002\u0018\u0002\n\u0002\b\u0002\n\u0002\u0018\u0002\n\u0002\b\u0003\bf\u0018\u0000 \b2\u00020\u0001:\u0002\u0007\bJ\u0018\u0010\u0002\u001a\u00020\u00032\u0006\u0010\u0004\u001a\u00020\u00012\u0006\u0010\u0005\u001a\u00020\u0006H&\u00a8\u0006\t"}, d2 = {"Luniffi/grafito_ffi/UniffiCleaner;", "", "register", "Luniffi/grafito_ffi/UniffiCleaner$Cleanable;", "value", "cleanUpTask", "Ljava/lang/Runnable;", "Cleanable", "Companion", "app_debug"})
public abstract interface UniffiCleaner {
    @org.jetbrains.annotations.NotNull()
    public static final uniffi.grafito_ffi.UniffiCleaner.Companion Companion = null;
    
    @org.jetbrains.annotations.NotNull()
    public abstract uniffi.grafito_ffi.UniffiCleaner.Cleanable register(@org.jetbrains.annotations.NotNull()
    java.lang.Object value, @org.jetbrains.annotations.NotNull()
    java.lang.Runnable cleanUpTask);
    
    @kotlin.Metadata(mv = {1, 9, 0}, k = 1, xi = 48, d1 = {"\u0000\u0010\n\u0002\u0018\u0002\n\u0002\u0010\u0000\n\u0000\n\u0002\u0010\u0002\n\u0000\bf\u0018\u00002\u00020\u0001J\b\u0010\u0002\u001a\u00020\u0003H&\u00a8\u0006\u0004"}, d2 = {"Luniffi/grafito_ffi/UniffiCleaner$Cleanable;", "", "clean", "", "app_debug"})
    public static abstract interface Cleanable {
        
        public abstract void clean();
    }
    
    @kotlin.Metadata(mv = {1, 9, 0}, k = 1, xi = 48, d1 = {"\u0000\f\n\u0002\u0018\u0002\n\u0002\u0010\u0000\n\u0002\b\u0002\b\u0086\u0003\u0018\u00002\u00020\u0001B\u0007\b\u0002\u00a2\u0006\u0002\u0010\u0002\u00a8\u0006\u0003"}, d2 = {"Luniffi/grafito_ffi/UniffiCleaner$Companion;", "", "()V", "app_debug"})
    public static final class Companion {
        
        private Companion() {
            super();
        }
    }
}