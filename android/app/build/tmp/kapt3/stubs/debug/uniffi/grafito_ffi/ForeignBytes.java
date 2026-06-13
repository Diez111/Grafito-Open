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

@com.sun.jna.Structure.FieldOrder(value = {"len", "data"})
@kotlin.Metadata(mv = {1, 9, 0}, k = 1, xi = 48, d1 = {"\u0000\u001a\n\u0002\u0018\u0002\n\u0002\u0018\u0002\n\u0002\b\u0002\n\u0002\u0018\u0002\n\u0000\n\u0002\u0010\b\n\u0002\b\u0002\b\u0011\u0018\u00002\u00020\u0001:\u0001\u0007B\u0005\u00a2\u0006\u0002\u0010\u0002R\u0014\u0010\u0003\u001a\u0004\u0018\u00010\u00048\u0006@\u0006X\u0087\u000e\u00a2\u0006\u0002\n\u0000R\u0012\u0010\u0005\u001a\u00020\u00068\u0006@\u0006X\u0087\u000e\u00a2\u0006\u0002\n\u0000\u00a8\u0006\b"}, d2 = {"Luniffi/grafito_ffi/ForeignBytes;", "Lcom/sun/jna/Structure;", "()V", "data", "Lcom/sun/jna/Pointer;", "len", "", "ByValue", "app_debug"})
public class ForeignBytes extends com.sun.jna.Structure {
    @kotlin.jvm.JvmField()
    public int len = 0;
    @kotlin.jvm.JvmField()
    @org.jetbrains.annotations.Nullable()
    public com.sun.jna.Pointer data;
    
    public ForeignBytes() {
        super();
    }
    
    @kotlin.Metadata(mv = {1, 9, 0}, k = 1, xi = 48, d1 = {"\u0000\u0010\n\u0002\u0018\u0002\n\u0002\u0018\u0002\n\u0002\u0018\u0002\n\u0002\b\u0002\u0018\u00002\u00020\u00012\u00020\u0002B\u0005\u00a2\u0006\u0002\u0010\u0003\u00a8\u0006\u0004"}, d2 = {"Luniffi/grafito_ffi/ForeignBytes$ByValue;", "Luniffi/grafito_ffi/ForeignBytes;", "Lcom/sun/jna/Structure$ByValue;", "()V", "app_debug"})
    public static final class ByValue extends uniffi.grafito_ffi.ForeignBytes implements com.sun.jna.Structure.ByValue {
        
        public ByValue() {
            super();
        }
    }
}