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

@kotlin.Metadata(mv = {1, 9, 0}, k = 1, xi = 48, d1 = {"\u0000\u0018\n\u0002\u0018\u0002\n\u0002\u0018\u0002\n\u0000\n\u0002\u0018\u0002\n\u0002\b\u0004\n\u0002\u0010\u0002\n\u0000\b\u0002\u0018\u00002\u00020\u0001B\r\u0012\u0006\u0010\u0002\u001a\u00020\u0003\u00a2\u0006\u0002\u0010\u0004J\b\u0010\u0007\u001a\u00020\bH\u0016R\u0011\u0010\u0002\u001a\u00020\u0003\u00a2\u0006\b\n\u0000\u001a\u0004\b\u0005\u0010\u0006\u00a8\u0006\t"}, d2 = {"Luniffi/grafito_ffi/JavaLangRefCleanable;", "Luniffi/grafito_ffi/UniffiCleaner$Cleanable;", "cleanable", "Ljava/lang/ref/Cleaner$Cleanable;", "(Ljava/lang/ref/Cleaner$Cleanable;)V", "getCleanable", "()Ljava/lang/ref/Cleaner$Cleanable;", "clean", "", "app_debug"})
final class JavaLangRefCleanable implements uniffi.grafito_ffi.UniffiCleaner.Cleanable {
    @org.jetbrains.annotations.NotNull()
    private final java.lang.ref.Cleaner.Cleanable cleanable = null;
    
    public JavaLangRefCleanable(@org.jetbrains.annotations.NotNull()
    java.lang.ref.Cleaner.Cleanable cleanable) {
        super();
    }
    
    @org.jetbrains.annotations.NotNull()
    public final java.lang.ref.Cleaner.Cleanable getCleanable() {
        return null;
    }
    
    @java.lang.Override()
    public void clean() {
    }
}