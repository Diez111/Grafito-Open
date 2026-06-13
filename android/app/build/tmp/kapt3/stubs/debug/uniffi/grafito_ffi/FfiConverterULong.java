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
@kotlin.Metadata(mv = {1, 9, 0}, k = 1, xi = 48, d1 = {"\u0000$\n\u0002\u0018\u0002\n\u0002\u0018\u0002\n\u0002\u0018\u0002\n\u0002\u0010\t\n\u0002\b\f\n\u0002\u0018\u0002\n\u0002\b\u0003\n\u0002\u0010\u0002\n\u0002\b\u0003\b\u00c6\u0002\u0018\u00002\u000e\u0012\u0004\u0012\u00020\u0002\u0012\u0004\u0012\u00020\u00030\u0001B\u0007\b\u0002\u00a2\u0006\u0002\u0010\u0004J\u001a\u0010\u0005\u001a\u00020\u00022\u0006\u0010\u0006\u001a\u00020\u0002H\u0016\u00f8\u0001\u0000\u00a2\u0006\u0004\b\u0007\u0010\bJ\u001d\u0010\t\u001a\u00020\u00022\u0006\u0010\u0006\u001a\u00020\u0003H\u0016\u00f8\u0001\u0001\u00f8\u0001\u0000\u00a2\u0006\u0004\b\n\u0010\bJ\u001a\u0010\u000b\u001a\u00020\u00032\u0006\u0010\u0006\u001a\u00020\u0002H\u0016\u00f8\u0001\u0000\u00a2\u0006\u0004\b\f\u0010\rJ\u001d\u0010\u000e\u001a\u00020\u00022\u0006\u0010\u000f\u001a\u00020\u0010H\u0016\u00f8\u0001\u0001\u00f8\u0001\u0000\u00a2\u0006\u0004\b\u0011\u0010\u0012J\"\u0010\u0013\u001a\u00020\u00142\u0006\u0010\u0006\u001a\u00020\u00022\u0006\u0010\u000f\u001a\u00020\u0010H\u0016\u00f8\u0001\u0000\u00a2\u0006\u0004\b\u0015\u0010\u0016\u0082\u0002\u000b\n\u0005\b\u00a1\u001e0\u0001\n\u0002\b!\u00a8\u0006\u0017"}, d2 = {"Luniffi/grafito_ffi/FfiConverterULong;", "Luniffi/grafito_ffi/FfiConverter;", "Lkotlin/ULong;", "", "()V", "allocationSize", "value", "allocationSize-PUiSbYQ", "(J)J", "lift", "lift-I7RO_PI", "lower", "lower-VKZWuLQ", "(J)Ljava/lang/Long;", "read", "buf", "Ljava/nio/ByteBuffer;", "read-I7RO_PI", "(Ljava/nio/ByteBuffer;)J", "write", "", "write-4PLdz1A", "(JLjava/nio/ByteBuffer;)V", "app_debug"})
public final class FfiConverterULong implements uniffi.grafito_ffi.FfiConverter<kotlin.ULong, java.lang.Long> {
    @org.jetbrains.annotations.NotNull()
    public static final uniffi.grafito_ffi.FfiConverterULong INSTANCE = null;
    
    private FfiConverterULong() {
        super();
    }
}