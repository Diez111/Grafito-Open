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

@kotlin.Metadata(mv = {1, 9, 0}, k = 1, xi = 48, d1 = {"\u0000&\n\u0002\u0018\u0002\n\u0002\u0018\u0002\n\u0002\b\u0002\n\u0002\u0018\u0002\n\u0002\b\u0004\n\u0002\u0018\u0002\n\u0000\n\u0002\u0010\u0000\n\u0000\n\u0002\u0018\u0002\n\u0000\b\u0002\u0018\u00002\u00020\u0001B\u0005\u00a2\u0006\u0002\u0010\u0002J\u0018\u0010\b\u001a\u00020\t2\u0006\u0010\n\u001a\u00020\u000b2\u0006\u0010\f\u001a\u00020\rH\u0016R\u0019\u0010\u0003\u001a\n \u0005*\u0004\u0018\u00010\u00040\u0004\u00a2\u0006\b\n\u0000\u001a\u0004\b\u0006\u0010\u0007\u00a8\u0006\u000e"}, d2 = {"Luniffi/grafito_ffi/JavaLangRefCleaner;", "Luniffi/grafito_ffi/UniffiCleaner;", "()V", "cleaner", "Ljava/lang/ref/Cleaner;", "kotlin.jvm.PlatformType", "getCleaner", "()Ljava/lang/ref/Cleaner;", "register", "Luniffi/grafito_ffi/UniffiCleaner$Cleanable;", "value", "", "cleanUpTask", "Ljava/lang/Runnable;", "app_debug"})
final class JavaLangRefCleaner implements uniffi.grafito_ffi.UniffiCleaner {
    private final java.lang.ref.Cleaner cleaner = null;
    
    public JavaLangRefCleaner() {
        super();
    }
    
    public final java.lang.ref.Cleaner getCleaner() {
        return null;
    }
    
    @java.lang.Override()
    @org.jetbrains.annotations.NotNull()
    public uniffi.grafito_ffi.UniffiCleaner.Cleanable register(@org.jetbrains.annotations.NotNull()
    java.lang.Object value, @org.jetbrains.annotations.NotNull()
    java.lang.Runnable cleanUpTask) {
        return null;
    }
}