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

@kotlin.Metadata(mv = {1, 9, 0}, k = 1, xi = 48, d1 = {"\u0000&\n\u0002\u0018\u0002\n\u0000\n\u0002\u0010\u0000\n\u0002\b\u0002\n\u0002\u0018\u0002\n\u0000\n\u0002\u0018\u0002\n\u0002\u0010\t\n\u0000\n\u0002\u0010\b\n\u0002\b\u000b\b\u0000\u0018\u0000*\b\b\u0000\u0010\u0001*\u00020\u00022\u00020\u0002B\u0005\u00a2\u0006\u0002\u0010\u0003J\u000e\u0010\r\u001a\u00020\b2\u0006\u0010\u000e\u001a\u00020\bJ\u0013\u0010\u000f\u001a\u00028\u00002\u0006\u0010\u000e\u001a\u00020\b\u00a2\u0006\u0002\u0010\u0010J\u0013\u0010\u0011\u001a\u00020\b2\u0006\u0010\u0012\u001a\u00028\u0000\u00a2\u0006\u0002\u0010\u0013J\u0013\u0010\u0014\u001a\u00028\u00002\u0006\u0010\u000e\u001a\u00020\b\u00a2\u0006\u0002\u0010\u0010R\u000e\u0010\u0004\u001a\u00020\u0005X\u0082\u0004\u00a2\u0006\u0002\n\u0000R\u001a\u0010\u0006\u001a\u000e\u0012\u0004\u0012\u00020\b\u0012\u0004\u0012\u00028\u00000\u0007X\u0082\u0004\u00a2\u0006\u0002\n\u0000R\u0011\u0010\t\u001a\u00020\n8F\u00a2\u0006\u0006\u001a\u0004\b\u000b\u0010\f\u00a8\u0006\u0015"}, d2 = {"Luniffi/grafito_ffi/UniffiHandleMap;", "T", "", "()V", "counter", "Ljava/util/concurrent/atomic/AtomicLong;", "map", "Ljava/util/concurrent/ConcurrentHashMap;", "", "size", "", "getSize", "()I", "clone", "handle", "get", "(J)Ljava/lang/Object;", "insert", "obj", "(Ljava/lang/Object;)J", "remove", "app_debug"})
public final class UniffiHandleMap<T extends java.lang.Object> {
    @org.jetbrains.annotations.NotNull()
    private final java.util.concurrent.ConcurrentHashMap<java.lang.Long, T> map = null;
    @org.jetbrains.annotations.NotNull()
    private final java.util.concurrent.atomic.AtomicLong counter = null;
    
    public UniffiHandleMap() {
        super();
    }
    
    public final int getSize() {
        return 0;
    }
    
    public final long insert(@org.jetbrains.annotations.NotNull()
    T obj) {
        return 0L;
    }
    
    public final long clone(long handle) {
        return 0L;
    }
    
    @org.jetbrains.annotations.NotNull()
    public final T get(long handle) {
        return null;
    }
    
    @org.jetbrains.annotations.NotNull()
    public final T remove(long handle) {
        return null;
    }
}