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
@kotlin.Metadata(mv = {1, 9, 0}, k = 1, xi = 48, d1 = {"\u0000,\n\u0002\u0018\u0002\n\u0002\u0018\u0002\n\u0002\u0018\u0002\n\u0002\u0010\b\n\u0002\b\u0002\n\u0002\u0018\u0002\n\u0002\b\u000b\n\u0002\u0018\u0002\n\u0002\b\u0003\n\u0002\u0010\u0002\n\u0002\b\u0003\b\u00c6\u0002\u0018\u00002\u000e\u0012\u0004\u0012\u00020\u0002\u0012\u0004\u0012\u00020\u00030\u0001B\u0007\b\u0002\u00a2\u0006\u0002\u0010\u0004J\u001a\u0010\u0005\u001a\u00020\u00062\u0006\u0010\u0007\u001a\u00020\u0002H\u0016\u00f8\u0001\u0000\u00a2\u0006\u0004\b\b\u0010\tJ\u001d\u0010\n\u001a\u00020\u00022\u0006\u0010\u0007\u001a\u00020\u0003H\u0016\u00f8\u0001\u0001\u00f8\u0001\u0000\u00a2\u0006\u0004\b\u000b\u0010\fJ\u001a\u0010\r\u001a\u00020\u00032\u0006\u0010\u0007\u001a\u00020\u0002H\u0016\u00f8\u0001\u0000\u00a2\u0006\u0004\b\u000e\u0010\u000fJ\u001d\u0010\u0010\u001a\u00020\u00022\u0006\u0010\u0011\u001a\u00020\u0012H\u0016\u00f8\u0001\u0001\u00f8\u0001\u0000\u00a2\u0006\u0004\b\u0013\u0010\u0014J\"\u0010\u0015\u001a\u00020\u00162\u0006\u0010\u0007\u001a\u00020\u00022\u0006\u0010\u0011\u001a\u00020\u0012H\u0016\u00f8\u0001\u0000\u00a2\u0006\u0004\b\u0017\u0010\u0018\u0082\u0002\u000b\n\u0005\b\u00a1\u001e0\u0001\n\u0002\b!\u00a8\u0006\u0019"}, d2 = {"Luniffi/grafito_ffi/FfiConverterUInt;", "Luniffi/grafito_ffi/FfiConverter;", "Lkotlin/UInt;", "", "()V", "allocationSize", "Lkotlin/ULong;", "value", "allocationSize-j8A87jM", "(I)J", "lift", "lift-OGnWXxg", "(I)I", "lower", "lower-WZ4Q5Ns", "(I)Ljava/lang/Integer;", "read", "buf", "Ljava/nio/ByteBuffer;", "read-OGnWXxg", "(Ljava/nio/ByteBuffer;)I", "write", "", "write-qim9Vi0", "(ILjava/nio/ByteBuffer;)V", "app_debug"})
public final class FfiConverterUInt implements uniffi.grafito_ffi.FfiConverter<kotlin.UInt, java.lang.Integer> {
    @org.jetbrains.annotations.NotNull()
    public static final uniffi.grafito_ffi.FfiConverterUInt INSTANCE = null;
    
    private FfiConverterUInt() {
        super();
    }
}