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

@kotlin.Metadata(mv = {1, 9, 0}, k = 1, xi = 48, d1 = {"\u0000`\n\u0002\u0018\u0002\n\u0002\u0010\u0000\n\u0002\b\u0002\n\u0002\u0018\u0002\n\u0002\b\u0005\n\u0002\u0010\u0002\n\u0000\n\u0002\u0010\t\n\u0002\b\f\n\u0002\u0010\u0007\n\u0000\n\u0002\u0018\u0002\n\u0000\n\u0002\u0010\u0006\n\u0000\n\u0002\u0010\n\n\u0000\n\u0002\u0010\b\n\u0002\b\u0002\n\u0002\u0010\u0005\n\u0000\n\u0002\u0018\u0002\n\u0002\b\u0013\n\u0002\u0018\u0002\n\u0002\b\u0012\n\u0002\u0018\u0002\n\u0002\bQ\b\u00c0\u0002\u0018\u00002\u00020\u0001B\u0007\b\u0002\u00a2\u0006\u0002\u0010\u0002J\u0011\u0010\t\u001a\u00020\n2\u0006\u0010\u000b\u001a\u00020\fH\u0086 J\u0011\u0010\r\u001a\u00020\n2\u0006\u0010\u000b\u001a\u00020\fH\u0086 J\u0011\u0010\u000e\u001a\u00020\n2\u0006\u0010\u000b\u001a\u00020\fH\u0086 J\u0011\u0010\u000f\u001a\u00020\n2\u0006\u0010\u000b\u001a\u00020\fH\u0086 J\u0011\u0010\u0010\u001a\u00020\n2\u0006\u0010\u000b\u001a\u00020\fH\u0086 J\u0011\u0010\u0011\u001a\u00020\n2\u0006\u0010\u000b\u001a\u00020\fH\u0086 J\u0011\u0010\u0012\u001a\u00020\n2\u0006\u0010\u000b\u001a\u00020\fH\u0086 J\u0011\u0010\u0013\u001a\u00020\n2\u0006\u0010\u000b\u001a\u00020\fH\u0086 J\u0011\u0010\u0014\u001a\u00020\n2\u0006\u0010\u000b\u001a\u00020\fH\u0086 J\u0011\u0010\u0015\u001a\u00020\n2\u0006\u0010\u000b\u001a\u00020\fH\u0086 J\u0011\u0010\u0016\u001a\u00020\n2\u0006\u0010\u000b\u001a\u00020\fH\u0086 J\u0011\u0010\u0017\u001a\u00020\n2\u0006\u0010\u000b\u001a\u00020\fH\u0086 J\u0019\u0010\u0018\u001a\u00020\u00192\u0006\u0010\u000b\u001a\u00020\f2\u0006\u0010\u001a\u001a\u00020\u001bH\u0086 J\u0019\u0010\u001c\u001a\u00020\u001d2\u0006\u0010\u000b\u001a\u00020\f2\u0006\u0010\u001a\u001a\u00020\u001bH\u0086 J\u0019\u0010\u001e\u001a\u00020\u001f2\u0006\u0010\u000b\u001a\u00020\f2\u0006\u0010\u001a\u001a\u00020\u001bH\u0086 J\u0019\u0010 \u001a\u00020!2\u0006\u0010\u000b\u001a\u00020\f2\u0006\u0010\u001a\u001a\u00020\u001bH\u0086 J\u0019\u0010\"\u001a\u00020\f2\u0006\u0010\u000b\u001a\u00020\f2\u0006\u0010\u001a\u001a\u00020\u001bH\u0086 J\u0019\u0010#\u001a\u00020$2\u0006\u0010\u000b\u001a\u00020\f2\u0006\u0010\u001a\u001a\u00020\u001bH\u0086 J\u0019\u0010%\u001a\u00020&2\u0006\u0010\u000b\u001a\u00020\f2\u0006\u0010\u001a\u001a\u00020\u001bH\u0086 J\u0019\u0010\'\u001a\u00020\u001f2\u0006\u0010\u000b\u001a\u00020\f2\u0006\u0010\u001a\u001a\u00020\u001bH\u0086 J\u0019\u0010(\u001a\u00020!2\u0006\u0010\u000b\u001a\u00020\f2\u0006\u0010\u001a\u001a\u00020\u001bH\u0086 J\u0019\u0010)\u001a\u00020\f2\u0006\u0010\u000b\u001a\u00020\f2\u0006\u0010\u001a\u001a\u00020\u001bH\u0086 J\u0019\u0010*\u001a\u00020$2\u0006\u0010\u000b\u001a\u00020\f2\u0006\u0010\u001a\u001a\u00020\u001bH\u0086 J\u0019\u0010+\u001a\u00020\n2\u0006\u0010\u000b\u001a\u00020\f2\u0006\u0010\u001a\u001a\u00020\u001bH\u0086 J\u0011\u0010,\u001a\u00020\n2\u0006\u0010\u000b\u001a\u00020\fH\u0086 J\u0011\u0010-\u001a\u00020\n2\u0006\u0010\u000b\u001a\u00020\fH\u0086 J\u0011\u0010.\u001a\u00020\n2\u0006\u0010\u000b\u001a\u00020\fH\u0086 J\u0011\u0010/\u001a\u00020\n2\u0006\u0010\u000b\u001a\u00020\fH\u0086 J\u0011\u00100\u001a\u00020\n2\u0006\u0010\u000b\u001a\u00020\fH\u0086 J\u0011\u00101\u001a\u00020\n2\u0006\u0010\u000b\u001a\u00020\fH\u0086 J\u0011\u00102\u001a\u00020\n2\u0006\u0010\u000b\u001a\u00020\fH\u0086 J\u0011\u00103\u001a\u00020\n2\u0006\u0010\u000b\u001a\u00020\fH\u0086 J\u0011\u00104\u001a\u00020\n2\u0006\u0010\u000b\u001a\u00020\fH\u0086 J\u0011\u00105\u001a\u00020\n2\u0006\u0010\u000b\u001a\u00020\fH\u0086 J\u0011\u00106\u001a\u00020\n2\u0006\u0010\u000b\u001a\u00020\fH\u0086 J\u0011\u00107\u001a\u00020\n2\u0006\u0010\u000b\u001a\u00020\fH\u0086 J!\u00108\u001a\u00020\n2\u0006\u0010\u000b\u001a\u00020\f2\u0006\u00109\u001a\u00020:2\u0006\u0010;\u001a\u00020\fH\u0086 J!\u0010<\u001a\u00020\n2\u0006\u0010\u000b\u001a\u00020\f2\u0006\u00109\u001a\u00020:2\u0006\u0010;\u001a\u00020\fH\u0086 J!\u0010=\u001a\u00020\n2\u0006\u0010\u000b\u001a\u00020\f2\u0006\u00109\u001a\u00020:2\u0006\u0010;\u001a\u00020\fH\u0086 J!\u0010>\u001a\u00020\n2\u0006\u0010\u000b\u001a\u00020\f2\u0006\u00109\u001a\u00020:2\u0006\u0010;\u001a\u00020\fH\u0086 J!\u0010?\u001a\u00020\n2\u0006\u0010\u000b\u001a\u00020\f2\u0006\u00109\u001a\u00020:2\u0006\u0010;\u001a\u00020\fH\u0086 J!\u0010@\u001a\u00020\n2\u0006\u0010\u000b\u001a\u00020\f2\u0006\u00109\u001a\u00020:2\u0006\u0010;\u001a\u00020\fH\u0086 J!\u0010A\u001a\u00020\n2\u0006\u0010\u000b\u001a\u00020\f2\u0006\u00109\u001a\u00020:2\u0006\u0010;\u001a\u00020\fH\u0086 J!\u0010B\u001a\u00020\n2\u0006\u0010\u000b\u001a\u00020\f2\u0006\u00109\u001a\u00020:2\u0006\u0010;\u001a\u00020\fH\u0086 J!\u0010C\u001a\u00020\n2\u0006\u0010\u000b\u001a\u00020\f2\u0006\u00109\u001a\u00020:2\u0006\u0010;\u001a\u00020\fH\u0086 J!\u0010D\u001a\u00020\n2\u0006\u0010\u000b\u001a\u00020\f2\u0006\u00109\u001a\u00020:2\u0006\u0010;\u001a\u00020\fH\u0086 J!\u0010E\u001a\u00020\n2\u0006\u0010\u000b\u001a\u00020\f2\u0006\u00109\u001a\u00020:2\u0006\u0010;\u001a\u00020\fH\u0086 J!\u0010F\u001a\u00020\n2\u0006\u0010\u000b\u001a\u00020\f2\u0006\u00109\u001a\u00020:2\u0006\u0010;\u001a\u00020\fH\u0086 J\u0019\u0010G\u001a\u00020&2\u0006\u0010H\u001a\u00020\f2\u0006\u0010\u001a\u001a\u00020\u001bH\u0086 J\u0019\u0010I\u001a\u00020\n2\u0006\u0010J\u001a\u00020&2\u0006\u0010\u001a\u001a\u00020\u001bH\u0086 J\u0019\u0010K\u001a\u00020&2\u0006\u0010L\u001a\u00020M2\u0006\u0010\u001a\u001a\u00020\u001bH\u0086 J!\u0010N\u001a\u00020&2\u0006\u0010J\u001a\u00020&2\u0006\u0010O\u001a\u00020\f2\u0006\u0010\u001a\u001a\u00020\u001bH\u0086 J\u0019\u0010P\u001a\u00020\f2\u0006\u0010\u000b\u001a\u00020\f2\u0006\u0010\u001a\u001a\u00020\u001bH\u0086 J\u0019\u0010Q\u001a\u00020\f2\u0006\u0010\u000b\u001a\u00020\f2\u0006\u0010\u001a\u001a\u00020\u001bH\u0086 J)\u0010R\u001a\u00020\f2\u0006\u0010S\u001a\u00020\f2\u0006\u0010T\u001a\u00020!2\u0006\u0010U\u001a\u00020!2\u0006\u0010\u001a\u001a\u00020\u001bH\u0086 J!\u0010V\u001a\u00020\f2\u0006\u0010W\u001a\u00020\u00192\u0006\u0010X\u001a\u00020\u00192\u0006\u0010\u001a\u001a\u00020\u001bH\u0086 J\u0019\u0010Y\u001a\u00020\n2\u0006\u0010\u000b\u001a\u00020\f2\u0006\u0010\u001a\u001a\u00020\u001bH\u0086 J\u0019\u0010Z\u001a\u00020\n2\u0006\u0010\u000b\u001a\u00020\f2\u0006\u0010\u001a\u001a\u00020\u001bH\u0086 J\u0019\u0010[\u001a\u00020\n2\u0006\u0010\\\u001a\u00020\f2\u0006\u0010\u001a\u001a\u00020\u001bH\u0086 J\u0019\u0010]\u001a\u00020!2\u0006\u0010\\\u001a\u00020\f2\u0006\u0010\u001a\u001a\u00020\u001bH\u0086 J\u0019\u0010^\u001a\u00020!2\u0006\u0010\\\u001a\u00020\f2\u0006\u0010\u001a\u001a\u00020\u001bH\u0086 J!\u0010_\u001a\u00020\n2\u0006\u0010\\\u001a\u00020\f2\u0006\u0010`\u001a\u00020\f2\u0006\u0010\u001a\u001a\u00020\u001bH\u0086 J\u0019\u0010a\u001a\u00020\n2\u0006\u0010\\\u001a\u00020\f2\u0006\u0010\u001a\u001a\u00020\u001bH\u0086 J)\u0010b\u001a\u00020\n2\u0006\u0010\\\u001a\u00020\f2\u0006\u0010T\u001a\u00020!2\u0006\u0010U\u001a\u00020!2\u0006\u0010\u001a\u001a\u00020\u001bH\u0086 J\u0019\u0010c\u001a\u00020\n2\u0006\u0010\\\u001a\u00020\f2\u0006\u0010\u001a\u001a\u00020\u001bH\u0086 J\u0019\u0010d\u001a\u00020\n2\u0006\u0010\\\u001a\u00020\f2\u0006\u0010\u001a\u001a\u00020\u001bH\u0086 J!\u0010e\u001a\u00020\n2\u0006\u0010\\\u001a\u00020\f2\u0006\u0010f\u001a\u00020\u00192\u0006\u0010\u001a\u001a\u00020\u001bH\u0086 J)\u0010g\u001a\u00020\n2\u0006\u0010\\\u001a\u00020\f2\u0006\u0010h\u001a\u00020\u00192\u0006\u0010i\u001a\u00020\u00192\u0006\u0010\u001a\u001a\u00020\u001bH\u0086 J)\u0010j\u001a\u00020\n2\u0006\u0010\\\u001a\u00020\f2\u0006\u0010k\u001a\u00020\u00192\u0006\u0010l\u001a\u00020\u00192\u0006\u0010\u001a\u001a\u00020\u001bH\u0086 J)\u0010m\u001a\u00020&2\u0006\u0010\\\u001a\u00020\f2\u0006\u0010n\u001a\u00020\u00192\u0006\u0010o\u001a\u00020\u00192\u0006\u0010\u001a\u001a\u00020\u001bH\u0086 J1\u0010p\u001a\u00020\n2\u0006\u0010\\\u001a\u00020\f2\u0006\u0010q\u001a\u00020\u00192\u0006\u0010r\u001a\u00020\u00192\u0006\u0010s\u001a\u00020\u00192\u0006\u0010\u001a\u001a\u00020\u001bH\u0086 J\u0019\u0010t\u001a\u00020\n2\u0006\u0010\\\u001a\u00020\f2\u0006\u0010\u001a\u001a\u00020\u001bH\u0086 J\u0019\u0010u\u001a\u00020\f2\u0006\u0010\\\u001a\u00020\f2\u0006\u0010\u001a\u001a\u00020\u001bH\u0086 J!\u0010v\u001a\u00020$2\u0006\u0010\\\u001a\u00020\f2\u0006\u0010w\u001a\u00020&2\u0006\u0010\u001a\u001a\u00020\u001bH\u0086 J\u0019\u0010x\u001a\u00020&2\u0006\u0010\\\u001a\u00020\f2\u0006\u0010\u001a\u001a\u00020\u001bH\u0086 J\u0019\u0010y\u001a\u00020&2\u0006\u0010\\\u001a\u00020\f2\u0006\u0010\u001a\u001a\u00020\u001bH\u0086 J\u0019\u0010z\u001a\u00020&2\u0006\u0010\\\u001a\u00020\f2\u0006\u0010\u001a\u001a\u00020\u001bH\u0086 J\u0019\u0010{\u001a\u00020$2\u0006\u0010\\\u001a\u00020\f2\u0006\u0010\u001a\u001a\u00020\u001bH\u0086 J!\u0010|\u001a\u00020$2\u0006\u0010\\\u001a\u00020\f2\u0006\u0010}\u001a\u00020&2\u0006\u0010\u001a\u001a\u00020\u001bH\u0086 J*\u0010~\u001a\u00020&2\u0006\u0010\\\u001a\u00020\f2\u0006\u0010\u007f\u001a\u00020\u00192\u0007\u0010\u0080\u0001\u001a\u00020\u00192\u0006\u0010\u001a\u001a\u00020\u001bH\u0086 J#\u0010\u0081\u0001\u001a\u00020&2\u0006\u0010\\\u001a\u00020\f2\u0007\u0010\u0082\u0001\u001a\u00020&2\u0006\u0010\u001a\u001a\u00020\u001bH\u0086 J\u001a\u0010\u0083\u0001\u001a\u00020$2\u0006\u0010\\\u001a\u00020\f2\u0006\u0010\u001a\u001a\u00020\u001bH\u0086 J\"\u0010\u0084\u0001\u001a\u00020$2\u0006\u0010\\\u001a\u00020\f2\u0006\u0010}\u001a\u00020&2\u0006\u0010\u001a\u001a\u00020\u001bH\u0086 J#\u0010\u0085\u0001\u001a\u00020&2\u0006\u0010\\\u001a\u00020\f2\u0007\u0010\u0086\u0001\u001a\u00020&2\u0006\u0010\u001a\u001a\u00020\u001bH\u0086 J\"\u0010\u0087\u0001\u001a\u00020\n2\u0006\u0010\\\u001a\u00020\f2\u0006\u0010w\u001a\u00020&2\u0006\u0010\u001a\u001a\u00020\u001bH\u0086 J5\u0010\u0088\u0001\u001a\u00020\n2\u0006\u0010\\\u001a\u00020\f2\u0007\u0010\u0089\u0001\u001a\u00020!2\u0007\u0010\u008a\u0001\u001a\u00020!2\u0007\u0010\u008b\u0001\u001a\u00020&2\u0006\u0010\u001a\u001a\u00020\u001bH\u0086 J#\u0010\u008c\u0001\u001a\u00020\n2\u0006\u0010\\\u001a\u00020\f2\u0007\u0010\u008d\u0001\u001a\u00020$2\u0006\u0010\u001a\u001a\u00020\u001bH\u0086 J=\u0010\u008e\u0001\u001a\u00020$2\u0006\u0010\\\u001a\u00020\f2\u0006\u0010w\u001a\u00020&2\u0007\u0010\u008f\u0001\u001a\u00020\u00192\u0007\u0010\u0090\u0001\u001a\u00020\u00192\u0007\u0010\u0091\u0001\u001a\u00020\u00192\u0006\u0010\u001a\u001a\u00020\u001bH\u0086 J+\u0010\u0092\u0001\u001a\u00020$2\u0006\u0010\\\u001a\u00020\f2\u0006\u0010w\u001a\u00020&2\u0007\u0010\u0093\u0001\u001a\u00020&2\u0006\u0010\u001a\u001a\u00020\u001bH\u0086 J#\u0010\u0094\u0001\u001a\u00020\n2\u0006\u0010\\\u001a\u00020\f2\u0007\u0010\u0095\u0001\u001a\u00020&2\u0006\u0010\u001a\u001a\u00020\u001bH\u0086 J,\u0010\u0096\u0001\u001a\u00020\n2\u0006\u0010\\\u001a\u00020\f2\u0007\u0010\u0097\u0001\u001a\u00020&2\u0007\u0010\u008b\u0001\u001a\u00020\u001d2\u0006\u0010\u001a\u001a\u00020\u001bH\u0086 J#\u0010\u0098\u0001\u001a\u00020\n2\u0006\u0010\\\u001a\u00020\f2\u0007\u0010\u0099\u0001\u001a\u00020&2\u0006\u0010\u001a\u001a\u00020\u001bH\u0086 J\"\u0010\u009a\u0001\u001a\u00020$2\u0006\u0010\\\u001a\u00020\f2\u0006\u0010w\u001a\u00020&2\u0006\u0010\u001a\u001a\u00020\u001bH\u0086 J\u001a\u0010\u009b\u0001\u001a\u00020$2\u0006\u0010\\\u001a\u00020\f2\u0006\u0010\u001a\u001a\u00020\u001bH\u0086 J*\u0010\u009c\u0001\u001a\u00020\n2\u0006\u0010\\\u001a\u00020\f2\u0006\u0010T\u001a\u00020\u00192\u0006\u0010U\u001a\u00020\u00192\u0006\u0010\u001a\u001a\u00020\u001bH\u0086 J\u001a\u0010\u009d\u0001\u001a\u00020\n2\u0006\u0010\\\u001a\u00020\f2\u0006\u0010\u001a\u001a\u00020\u001bH\u0086 R\u001b\u0010\u0003\u001a\u00020\u00048@X\u0080\u0084\u0002\u00a2\u0006\f\n\u0004\b\u0007\u0010\b\u001a\u0004\b\u0005\u0010\u0006\u00a8\u0006\u009e\u0001"}, d2 = {"Luniffi/grafito_ffi/UniffiLib;", "", "()V", "CLEANER", "Luniffi/grafito_ffi/UniffiCleaner;", "getCLEANER$app_debug", "()Luniffi/grafito_ffi/UniffiCleaner;", "CLEANER$delegate", "Lkotlin/Lazy;", "ffi_grafito_ffi_rust_future_cancel_f32", "", "handle", "", "ffi_grafito_ffi_rust_future_cancel_f64", "ffi_grafito_ffi_rust_future_cancel_i16", "ffi_grafito_ffi_rust_future_cancel_i32", "ffi_grafito_ffi_rust_future_cancel_i64", "ffi_grafito_ffi_rust_future_cancel_i8", "ffi_grafito_ffi_rust_future_cancel_rust_buffer", "ffi_grafito_ffi_rust_future_cancel_u16", "ffi_grafito_ffi_rust_future_cancel_u32", "ffi_grafito_ffi_rust_future_cancel_u64", "ffi_grafito_ffi_rust_future_cancel_u8", "ffi_grafito_ffi_rust_future_cancel_void", "ffi_grafito_ffi_rust_future_complete_f32", "", "uniffi_out_err", "Luniffi/grafito_ffi/UniffiRustCallStatus;", "ffi_grafito_ffi_rust_future_complete_f64", "", "ffi_grafito_ffi_rust_future_complete_i16", "", "ffi_grafito_ffi_rust_future_complete_i32", "", "ffi_grafito_ffi_rust_future_complete_i64", "ffi_grafito_ffi_rust_future_complete_i8", "", "ffi_grafito_ffi_rust_future_complete_rust_buffer", "Luniffi/grafito_ffi/RustBuffer$ByValue;", "ffi_grafito_ffi_rust_future_complete_u16", "ffi_grafito_ffi_rust_future_complete_u32", "ffi_grafito_ffi_rust_future_complete_u64", "ffi_grafito_ffi_rust_future_complete_u8", "ffi_grafito_ffi_rust_future_complete_void", "ffi_grafito_ffi_rust_future_free_f32", "ffi_grafito_ffi_rust_future_free_f64", "ffi_grafito_ffi_rust_future_free_i16", "ffi_grafito_ffi_rust_future_free_i32", "ffi_grafito_ffi_rust_future_free_i64", "ffi_grafito_ffi_rust_future_free_i8", "ffi_grafito_ffi_rust_future_free_rust_buffer", "ffi_grafito_ffi_rust_future_free_u16", "ffi_grafito_ffi_rust_future_free_u32", "ffi_grafito_ffi_rust_future_free_u64", "ffi_grafito_ffi_rust_future_free_u8", "ffi_grafito_ffi_rust_future_free_void", "ffi_grafito_ffi_rust_future_poll_f32", "callback", "Luniffi/grafito_ffi/UniffiRustFutureContinuationCallback;", "callbackData", "ffi_grafito_ffi_rust_future_poll_f64", "ffi_grafito_ffi_rust_future_poll_i16", "ffi_grafito_ffi_rust_future_poll_i32", "ffi_grafito_ffi_rust_future_poll_i64", "ffi_grafito_ffi_rust_future_poll_i8", "ffi_grafito_ffi_rust_future_poll_rust_buffer", "ffi_grafito_ffi_rust_future_poll_u16", "ffi_grafito_ffi_rust_future_poll_u32", "ffi_grafito_ffi_rust_future_poll_u64", "ffi_grafito_ffi_rust_future_poll_u8", "ffi_grafito_ffi_rust_future_poll_void", "ffi_grafito_ffi_rustbuffer_alloc", "size", "ffi_grafito_ffi_rustbuffer_free", "buf", "ffi_grafito_ffi_rustbuffer_from_bytes", "bytes", "Luniffi/grafito_ffi/ForeignBytes$ByValue;", "ffi_grafito_ffi_rustbuffer_reserve", "additional", "uniffi_grafito_ffi_fn_clone_canvasrenderer", "uniffi_grafito_ffi_fn_clone_grafitoengine", "uniffi_grafito_ffi_fn_constructor_canvasrenderer_new", "engine", "width", "height", "uniffi_grafito_ffi_fn_constructor_grafitoengine_new", "screenWidth", "screenHeight", "uniffi_grafito_ffi_fn_free_canvasrenderer", "uniffi_grafito_ffi_fn_free_grafitoengine", "uniffi_grafito_ffi_fn_method_canvasrenderer_cleanup", "ptr", "uniffi_grafito_ffi_fn_method_canvasrenderer_get_height", "uniffi_grafito_ffi_fn_method_canvasrenderer_get_width", "uniffi_grafito_ffi_fn_method_canvasrenderer_init_with_surface", "surfacePtr", "uniffi_grafito_ffi_fn_method_canvasrenderer_render_frame", "uniffi_grafito_ffi_fn_method_canvasrenderer_resize", "uniffi_grafito_ffi_fn_method_canvasrenderer_start_render_loop", "uniffi_grafito_ffi_fn_method_canvasrenderer_stop_render_loop", "uniffi_grafito_ffi_fn_method_grafitoengine_camera_dolly", "delta", "uniffi_grafito_ffi_fn_method_grafitoengine_camera_orbit", "deltaAzimuth", "deltaElevation", "uniffi_grafito_ffi_fn_method_grafitoengine_canvas_pan", "dx", "dy", "uniffi_grafito_ffi_fn_method_grafitoengine_canvas_tap", "x", "y", "uniffi_grafito_ffi_fn_method_grafitoengine_canvas_zoom", "factor", "centerX", "centerY", "uniffi_grafito_ffi_fn_method_grafitoengine_clear", "uniffi_grafito_ffi_fn_method_grafitoengine_create_canvas_renderer", "uniffi_grafito_ffi_fn_method_grafitoengine_delete_object", "id", "uniffi_grafito_ffi_fn_method_grafitoengine_get_snapshot", "uniffi_grafito_ffi_fn_method_grafitoengine_get_spreadsheet", "uniffi_grafito_ffi_fn_method_grafitoengine_get_tool", "uniffi_grafito_ffi_fn_method_grafitoengine_is_dark_mode", "uniffi_grafito_ffi_fn_method_grafitoengine_load_from_file", "path", "uniffi_grafito_ffi_fn_method_grafitoengine_pick_object_at", "screenX", "screenY", "uniffi_grafito_ffi_fn_method_grafitoengine_process_command", "input", "uniffi_grafito_ffi_fn_method_grafitoengine_redo", "uniffi_grafito_ffi_fn_method_grafitoengine_save_to_file", "uniffi_grafito_ffi_fn_method_grafitoengine_search_commands", "query", "uniffi_grafito_ffi_fn_method_grafitoengine_select_object", "uniffi_grafito_ffi_fn_method_grafitoengine_set_cell", "row", "col", "value", "uniffi_grafito_ffi_fn_method_grafitoengine_set_dark_mode", "dark", "uniffi_grafito_ffi_fn_method_grafitoengine_set_object_color", "r", "g", "b", "uniffi_grafito_ffi_fn_method_grafitoengine_set_object_label", "label", "uniffi_grafito_ffi_fn_method_grafitoengine_set_tool", "tool", "uniffi_grafito_ffi_fn_method_grafitoengine_set_variable", "name", "uniffi_grafito_ffi_fn_method_grafitoengine_set_view_mode", "mode", "uniffi_grafito_ffi_fn_method_grafitoengine_toggle_visibility", "uniffi_grafito_ffi_fn_method_grafitoengine_undo", "uniffi_grafito_ffi_fn_method_grafitoengine_update_screen_size", "uniffi_grafito_ffi_fn_method_grafitoengine_zoom_to_fit", "app_debug"})
public final class UniffiLib {
    @org.jetbrains.annotations.NotNull()
    private static final kotlin.Lazy CLEANER$delegate = null;
    @org.jetbrains.annotations.NotNull()
    public static final uniffi.grafito_ffi.UniffiLib INSTANCE = null;
    
    private UniffiLib() {
        super();
    }
    
    @org.jetbrains.annotations.NotNull()
    public final uniffi.grafito_ffi.UniffiCleaner getCLEANER$app_debug() {
        return null;
    }
    
    public final native long uniffi_grafito_ffi_fn_clone_grafitoengine(long handle, @org.jetbrains.annotations.NotNull()
    uniffi.grafito_ffi.UniffiRustCallStatus uniffi_out_err) {
        return 0L;
    }
    
    public final native void uniffi_grafito_ffi_fn_free_grafitoengine(long handle, @org.jetbrains.annotations.NotNull()
    uniffi.grafito_ffi.UniffiRustCallStatus uniffi_out_err) {
    }
    
    public final native long uniffi_grafito_ffi_fn_constructor_grafitoengine_new(float screenWidth, float screenHeight, @org.jetbrains.annotations.NotNull()
    uniffi.grafito_ffi.UniffiRustCallStatus uniffi_out_err) {
        return 0L;
    }
    
    public final native void uniffi_grafito_ffi_fn_method_grafitoengine_camera_dolly(long ptr, float delta, @org.jetbrains.annotations.NotNull()
    uniffi.grafito_ffi.UniffiRustCallStatus uniffi_out_err) {
    }
    
    public final native void uniffi_grafito_ffi_fn_method_grafitoengine_camera_orbit(long ptr, float deltaAzimuth, float deltaElevation, @org.jetbrains.annotations.NotNull()
    uniffi.grafito_ffi.UniffiRustCallStatus uniffi_out_err) {
    }
    
    public final native void uniffi_grafito_ffi_fn_method_grafitoengine_canvas_pan(long ptr, float dx, float dy, @org.jetbrains.annotations.NotNull()
    uniffi.grafito_ffi.UniffiRustCallStatus uniffi_out_err) {
    }
    
    @org.jetbrains.annotations.NotNull()
    public final native uniffi.grafito_ffi.RustBuffer.ByValue uniffi_grafito_ffi_fn_method_grafitoengine_canvas_tap(long ptr, float x, float y, @org.jetbrains.annotations.NotNull()
    uniffi.grafito_ffi.UniffiRustCallStatus uniffi_out_err) {
        return null;
    }
    
    public final native void uniffi_grafito_ffi_fn_method_grafitoengine_canvas_zoom(long ptr, float factor, float centerX, float centerY, @org.jetbrains.annotations.NotNull()
    uniffi.grafito_ffi.UniffiRustCallStatus uniffi_out_err) {
    }
    
    public final native void uniffi_grafito_ffi_fn_method_grafitoengine_clear(long ptr, @org.jetbrains.annotations.NotNull()
    uniffi.grafito_ffi.UniffiRustCallStatus uniffi_out_err) {
    }
    
    public final native long uniffi_grafito_ffi_fn_method_grafitoengine_create_canvas_renderer(long ptr, @org.jetbrains.annotations.NotNull()
    uniffi.grafito_ffi.UniffiRustCallStatus uniffi_out_err) {
        return 0L;
    }
    
    public final native byte uniffi_grafito_ffi_fn_method_grafitoengine_delete_object(long ptr, @org.jetbrains.annotations.NotNull()
    uniffi.grafito_ffi.RustBuffer.ByValue id, @org.jetbrains.annotations.NotNull()
    uniffi.grafito_ffi.UniffiRustCallStatus uniffi_out_err) {
        return 0;
    }
    
    @org.jetbrains.annotations.NotNull()
    public final native uniffi.grafito_ffi.RustBuffer.ByValue uniffi_grafito_ffi_fn_method_grafitoengine_get_snapshot(long ptr, @org.jetbrains.annotations.NotNull()
    uniffi.grafito_ffi.UniffiRustCallStatus uniffi_out_err) {
        return null;
    }
    
    @org.jetbrains.annotations.NotNull()
    public final native uniffi.grafito_ffi.RustBuffer.ByValue uniffi_grafito_ffi_fn_method_grafitoengine_get_spreadsheet(long ptr, @org.jetbrains.annotations.NotNull()
    uniffi.grafito_ffi.UniffiRustCallStatus uniffi_out_err) {
        return null;
    }
    
    @org.jetbrains.annotations.NotNull()
    public final native uniffi.grafito_ffi.RustBuffer.ByValue uniffi_grafito_ffi_fn_method_grafitoengine_get_tool(long ptr, @org.jetbrains.annotations.NotNull()
    uniffi.grafito_ffi.UniffiRustCallStatus uniffi_out_err) {
        return null;
    }
    
    public final native byte uniffi_grafito_ffi_fn_method_grafitoengine_is_dark_mode(long ptr, @org.jetbrains.annotations.NotNull()
    uniffi.grafito_ffi.UniffiRustCallStatus uniffi_out_err) {
        return 0;
    }
    
    public final native byte uniffi_grafito_ffi_fn_method_grafitoengine_load_from_file(long ptr, @org.jetbrains.annotations.NotNull()
    uniffi.grafito_ffi.RustBuffer.ByValue path, @org.jetbrains.annotations.NotNull()
    uniffi.grafito_ffi.UniffiRustCallStatus uniffi_out_err) {
        return 0;
    }
    
    @org.jetbrains.annotations.NotNull()
    public final native uniffi.grafito_ffi.RustBuffer.ByValue uniffi_grafito_ffi_fn_method_grafitoengine_pick_object_at(long ptr, float screenX, float screenY, @org.jetbrains.annotations.NotNull()
    uniffi.grafito_ffi.UniffiRustCallStatus uniffi_out_err) {
        return null;
    }
    
    @org.jetbrains.annotations.NotNull()
    public final native uniffi.grafito_ffi.RustBuffer.ByValue uniffi_grafito_ffi_fn_method_grafitoengine_process_command(long ptr, @org.jetbrains.annotations.NotNull()
    uniffi.grafito_ffi.RustBuffer.ByValue input, @org.jetbrains.annotations.NotNull()
    uniffi.grafito_ffi.UniffiRustCallStatus uniffi_out_err) {
        return null;
    }
    
    public final native byte uniffi_grafito_ffi_fn_method_grafitoengine_redo(long ptr, @org.jetbrains.annotations.NotNull()
    uniffi.grafito_ffi.UniffiRustCallStatus uniffi_out_err) {
        return 0;
    }
    
    public final native byte uniffi_grafito_ffi_fn_method_grafitoengine_save_to_file(long ptr, @org.jetbrains.annotations.NotNull()
    uniffi.grafito_ffi.RustBuffer.ByValue path, @org.jetbrains.annotations.NotNull()
    uniffi.grafito_ffi.UniffiRustCallStatus uniffi_out_err) {
        return 0;
    }
    
    @org.jetbrains.annotations.NotNull()
    public final native uniffi.grafito_ffi.RustBuffer.ByValue uniffi_grafito_ffi_fn_method_grafitoengine_search_commands(long ptr, @org.jetbrains.annotations.NotNull()
    uniffi.grafito_ffi.RustBuffer.ByValue query, @org.jetbrains.annotations.NotNull()
    uniffi.grafito_ffi.UniffiRustCallStatus uniffi_out_err) {
        return null;
    }
    
    public final native void uniffi_grafito_ffi_fn_method_grafitoengine_select_object(long ptr, @org.jetbrains.annotations.NotNull()
    uniffi.grafito_ffi.RustBuffer.ByValue id, @org.jetbrains.annotations.NotNull()
    uniffi.grafito_ffi.UniffiRustCallStatus uniffi_out_err) {
    }
    
    public final native void uniffi_grafito_ffi_fn_method_grafitoengine_set_cell(long ptr, int row, int col, @org.jetbrains.annotations.NotNull()
    uniffi.grafito_ffi.RustBuffer.ByValue value, @org.jetbrains.annotations.NotNull()
    uniffi.grafito_ffi.UniffiRustCallStatus uniffi_out_err) {
    }
    
    public final native void uniffi_grafito_ffi_fn_method_grafitoengine_set_dark_mode(long ptr, byte dark, @org.jetbrains.annotations.NotNull()
    uniffi.grafito_ffi.UniffiRustCallStatus uniffi_out_err) {
    }
    
    public final native byte uniffi_grafito_ffi_fn_method_grafitoengine_set_object_color(long ptr, @org.jetbrains.annotations.NotNull()
    uniffi.grafito_ffi.RustBuffer.ByValue id, float r, float g, float b, @org.jetbrains.annotations.NotNull()
    uniffi.grafito_ffi.UniffiRustCallStatus uniffi_out_err) {
        return 0;
    }
    
    public final native byte uniffi_grafito_ffi_fn_method_grafitoengine_set_object_label(long ptr, @org.jetbrains.annotations.NotNull()
    uniffi.grafito_ffi.RustBuffer.ByValue id, @org.jetbrains.annotations.NotNull()
    uniffi.grafito_ffi.RustBuffer.ByValue label, @org.jetbrains.annotations.NotNull()
    uniffi.grafito_ffi.UniffiRustCallStatus uniffi_out_err) {
        return 0;
    }
    
    public final native void uniffi_grafito_ffi_fn_method_grafitoengine_set_tool(long ptr, @org.jetbrains.annotations.NotNull()
    uniffi.grafito_ffi.RustBuffer.ByValue tool, @org.jetbrains.annotations.NotNull()
    uniffi.grafito_ffi.UniffiRustCallStatus uniffi_out_err) {
    }
    
    public final native void uniffi_grafito_ffi_fn_method_grafitoengine_set_variable(long ptr, @org.jetbrains.annotations.NotNull()
    uniffi.grafito_ffi.RustBuffer.ByValue name, double value, @org.jetbrains.annotations.NotNull()
    uniffi.grafito_ffi.UniffiRustCallStatus uniffi_out_err) {
    }
    
    public final native void uniffi_grafito_ffi_fn_method_grafitoengine_set_view_mode(long ptr, @org.jetbrains.annotations.NotNull()
    uniffi.grafito_ffi.RustBuffer.ByValue mode, @org.jetbrains.annotations.NotNull()
    uniffi.grafito_ffi.UniffiRustCallStatus uniffi_out_err) {
    }
    
    public final native byte uniffi_grafito_ffi_fn_method_grafitoengine_toggle_visibility(long ptr, @org.jetbrains.annotations.NotNull()
    uniffi.grafito_ffi.RustBuffer.ByValue id, @org.jetbrains.annotations.NotNull()
    uniffi.grafito_ffi.UniffiRustCallStatus uniffi_out_err) {
        return 0;
    }
    
    public final native byte uniffi_grafito_ffi_fn_method_grafitoengine_undo(long ptr, @org.jetbrains.annotations.NotNull()
    uniffi.grafito_ffi.UniffiRustCallStatus uniffi_out_err) {
        return 0;
    }
    
    public final native void uniffi_grafito_ffi_fn_method_grafitoengine_update_screen_size(long ptr, float width, float height, @org.jetbrains.annotations.NotNull()
    uniffi.grafito_ffi.UniffiRustCallStatus uniffi_out_err) {
    }
    
    public final native void uniffi_grafito_ffi_fn_method_grafitoengine_zoom_to_fit(long ptr, @org.jetbrains.annotations.NotNull()
    uniffi.grafito_ffi.UniffiRustCallStatus uniffi_out_err) {
    }
    
    public final native long uniffi_grafito_ffi_fn_clone_canvasrenderer(long handle, @org.jetbrains.annotations.NotNull()
    uniffi.grafito_ffi.UniffiRustCallStatus uniffi_out_err) {
        return 0L;
    }
    
    public final native void uniffi_grafito_ffi_fn_free_canvasrenderer(long handle, @org.jetbrains.annotations.NotNull()
    uniffi.grafito_ffi.UniffiRustCallStatus uniffi_out_err) {
    }
    
    public final native long uniffi_grafito_ffi_fn_constructor_canvasrenderer_new(long engine, int width, int height, @org.jetbrains.annotations.NotNull()
    uniffi.grafito_ffi.UniffiRustCallStatus uniffi_out_err) {
        return 0L;
    }
    
    public final native void uniffi_grafito_ffi_fn_method_canvasrenderer_cleanup(long ptr, @org.jetbrains.annotations.NotNull()
    uniffi.grafito_ffi.UniffiRustCallStatus uniffi_out_err) {
    }
    
    public final native int uniffi_grafito_ffi_fn_method_canvasrenderer_get_height(long ptr, @org.jetbrains.annotations.NotNull()
    uniffi.grafito_ffi.UniffiRustCallStatus uniffi_out_err) {
        return 0;
    }
    
    public final native int uniffi_grafito_ffi_fn_method_canvasrenderer_get_width(long ptr, @org.jetbrains.annotations.NotNull()
    uniffi.grafito_ffi.UniffiRustCallStatus uniffi_out_err) {
        return 0;
    }
    
    public final native void uniffi_grafito_ffi_fn_method_canvasrenderer_init_with_surface(long ptr, long surfacePtr, @org.jetbrains.annotations.NotNull()
    uniffi.grafito_ffi.UniffiRustCallStatus uniffi_out_err) {
    }
    
    public final native void uniffi_grafito_ffi_fn_method_canvasrenderer_render_frame(long ptr, @org.jetbrains.annotations.NotNull()
    uniffi.grafito_ffi.UniffiRustCallStatus uniffi_out_err) {
    }
    
    public final native void uniffi_grafito_ffi_fn_method_canvasrenderer_resize(long ptr, int width, int height, @org.jetbrains.annotations.NotNull()
    uniffi.grafito_ffi.UniffiRustCallStatus uniffi_out_err) {
    }
    
    public final native void uniffi_grafito_ffi_fn_method_canvasrenderer_start_render_loop(long ptr, @org.jetbrains.annotations.NotNull()
    uniffi.grafito_ffi.UniffiRustCallStatus uniffi_out_err) {
    }
    
    public final native void uniffi_grafito_ffi_fn_method_canvasrenderer_stop_render_loop(long ptr, @org.jetbrains.annotations.NotNull()
    uniffi.grafito_ffi.UniffiRustCallStatus uniffi_out_err) {
    }
    
    @org.jetbrains.annotations.NotNull()
    public final native uniffi.grafito_ffi.RustBuffer.ByValue ffi_grafito_ffi_rustbuffer_alloc(long size, @org.jetbrains.annotations.NotNull()
    uniffi.grafito_ffi.UniffiRustCallStatus uniffi_out_err) {
        return null;
    }
    
    @org.jetbrains.annotations.NotNull()
    public final native uniffi.grafito_ffi.RustBuffer.ByValue ffi_grafito_ffi_rustbuffer_from_bytes(@org.jetbrains.annotations.NotNull()
    uniffi.grafito_ffi.ForeignBytes.ByValue bytes, @org.jetbrains.annotations.NotNull()
    uniffi.grafito_ffi.UniffiRustCallStatus uniffi_out_err) {
        return null;
    }
    
    public final native void ffi_grafito_ffi_rustbuffer_free(@org.jetbrains.annotations.NotNull()
    uniffi.grafito_ffi.RustBuffer.ByValue buf, @org.jetbrains.annotations.NotNull()
    uniffi.grafito_ffi.UniffiRustCallStatus uniffi_out_err) {
    }
    
    @org.jetbrains.annotations.NotNull()
    public final native uniffi.grafito_ffi.RustBuffer.ByValue ffi_grafito_ffi_rustbuffer_reserve(@org.jetbrains.annotations.NotNull()
    uniffi.grafito_ffi.RustBuffer.ByValue buf, long additional, @org.jetbrains.annotations.NotNull()
    uniffi.grafito_ffi.UniffiRustCallStatus uniffi_out_err) {
        return null;
    }
    
    public final native void ffi_grafito_ffi_rust_future_poll_u8(long handle, @org.jetbrains.annotations.NotNull()
    uniffi.grafito_ffi.UniffiRustFutureContinuationCallback callback, long callbackData) {
    }
    
    public final native void ffi_grafito_ffi_rust_future_cancel_u8(long handle) {
    }
    
    public final native void ffi_grafito_ffi_rust_future_free_u8(long handle) {
    }
    
    public final native byte ffi_grafito_ffi_rust_future_complete_u8(long handle, @org.jetbrains.annotations.NotNull()
    uniffi.grafito_ffi.UniffiRustCallStatus uniffi_out_err) {
        return 0;
    }
    
    public final native void ffi_grafito_ffi_rust_future_poll_i8(long handle, @org.jetbrains.annotations.NotNull()
    uniffi.grafito_ffi.UniffiRustFutureContinuationCallback callback, long callbackData) {
    }
    
    public final native void ffi_grafito_ffi_rust_future_cancel_i8(long handle) {
    }
    
    public final native void ffi_grafito_ffi_rust_future_free_i8(long handle) {
    }
    
    public final native byte ffi_grafito_ffi_rust_future_complete_i8(long handle, @org.jetbrains.annotations.NotNull()
    uniffi.grafito_ffi.UniffiRustCallStatus uniffi_out_err) {
        return 0;
    }
    
    public final native void ffi_grafito_ffi_rust_future_poll_u16(long handle, @org.jetbrains.annotations.NotNull()
    uniffi.grafito_ffi.UniffiRustFutureContinuationCallback callback, long callbackData) {
    }
    
    public final native void ffi_grafito_ffi_rust_future_cancel_u16(long handle) {
    }
    
    public final native void ffi_grafito_ffi_rust_future_free_u16(long handle) {
    }
    
    public final native short ffi_grafito_ffi_rust_future_complete_u16(long handle, @org.jetbrains.annotations.NotNull()
    uniffi.grafito_ffi.UniffiRustCallStatus uniffi_out_err) {
        return 0;
    }
    
    public final native void ffi_grafito_ffi_rust_future_poll_i16(long handle, @org.jetbrains.annotations.NotNull()
    uniffi.grafito_ffi.UniffiRustFutureContinuationCallback callback, long callbackData) {
    }
    
    public final native void ffi_grafito_ffi_rust_future_cancel_i16(long handle) {
    }
    
    public final native void ffi_grafito_ffi_rust_future_free_i16(long handle) {
    }
    
    public final native short ffi_grafito_ffi_rust_future_complete_i16(long handle, @org.jetbrains.annotations.NotNull()
    uniffi.grafito_ffi.UniffiRustCallStatus uniffi_out_err) {
        return 0;
    }
    
    public final native void ffi_grafito_ffi_rust_future_poll_u32(long handle, @org.jetbrains.annotations.NotNull()
    uniffi.grafito_ffi.UniffiRustFutureContinuationCallback callback, long callbackData) {
    }
    
    public final native void ffi_grafito_ffi_rust_future_cancel_u32(long handle) {
    }
    
    public final native void ffi_grafito_ffi_rust_future_free_u32(long handle) {
    }
    
    public final native int ffi_grafito_ffi_rust_future_complete_u32(long handle, @org.jetbrains.annotations.NotNull()
    uniffi.grafito_ffi.UniffiRustCallStatus uniffi_out_err) {
        return 0;
    }
    
    public final native void ffi_grafito_ffi_rust_future_poll_i32(long handle, @org.jetbrains.annotations.NotNull()
    uniffi.grafito_ffi.UniffiRustFutureContinuationCallback callback, long callbackData) {
    }
    
    public final native void ffi_grafito_ffi_rust_future_cancel_i32(long handle) {
    }
    
    public final native void ffi_grafito_ffi_rust_future_free_i32(long handle) {
    }
    
    public final native int ffi_grafito_ffi_rust_future_complete_i32(long handle, @org.jetbrains.annotations.NotNull()
    uniffi.grafito_ffi.UniffiRustCallStatus uniffi_out_err) {
        return 0;
    }
    
    public final native void ffi_grafito_ffi_rust_future_poll_u64(long handle, @org.jetbrains.annotations.NotNull()
    uniffi.grafito_ffi.UniffiRustFutureContinuationCallback callback, long callbackData) {
    }
    
    public final native void ffi_grafito_ffi_rust_future_cancel_u64(long handle) {
    }
    
    public final native void ffi_grafito_ffi_rust_future_free_u64(long handle) {
    }
    
    public final native long ffi_grafito_ffi_rust_future_complete_u64(long handle, @org.jetbrains.annotations.NotNull()
    uniffi.grafito_ffi.UniffiRustCallStatus uniffi_out_err) {
        return 0L;
    }
    
    public final native void ffi_grafito_ffi_rust_future_poll_i64(long handle, @org.jetbrains.annotations.NotNull()
    uniffi.grafito_ffi.UniffiRustFutureContinuationCallback callback, long callbackData) {
    }
    
    public final native void ffi_grafito_ffi_rust_future_cancel_i64(long handle) {
    }
    
    public final native void ffi_grafito_ffi_rust_future_free_i64(long handle) {
    }
    
    public final native long ffi_grafito_ffi_rust_future_complete_i64(long handle, @org.jetbrains.annotations.NotNull()
    uniffi.grafito_ffi.UniffiRustCallStatus uniffi_out_err) {
        return 0L;
    }
    
    public final native void ffi_grafito_ffi_rust_future_poll_f32(long handle, @org.jetbrains.annotations.NotNull()
    uniffi.grafito_ffi.UniffiRustFutureContinuationCallback callback, long callbackData) {
    }
    
    public final native void ffi_grafito_ffi_rust_future_cancel_f32(long handle) {
    }
    
    public final native void ffi_grafito_ffi_rust_future_free_f32(long handle) {
    }
    
    public final native float ffi_grafito_ffi_rust_future_complete_f32(long handle, @org.jetbrains.annotations.NotNull()
    uniffi.grafito_ffi.UniffiRustCallStatus uniffi_out_err) {
        return 0.0F;
    }
    
    public final native void ffi_grafito_ffi_rust_future_poll_f64(long handle, @org.jetbrains.annotations.NotNull()
    uniffi.grafito_ffi.UniffiRustFutureContinuationCallback callback, long callbackData) {
    }
    
    public final native void ffi_grafito_ffi_rust_future_cancel_f64(long handle) {
    }
    
    public final native void ffi_grafito_ffi_rust_future_free_f64(long handle) {
    }
    
    public final native double ffi_grafito_ffi_rust_future_complete_f64(long handle, @org.jetbrains.annotations.NotNull()
    uniffi.grafito_ffi.UniffiRustCallStatus uniffi_out_err) {
        return 0.0;
    }
    
    public final native void ffi_grafito_ffi_rust_future_poll_rust_buffer(long handle, @org.jetbrains.annotations.NotNull()
    uniffi.grafito_ffi.UniffiRustFutureContinuationCallback callback, long callbackData) {
    }
    
    public final native void ffi_grafito_ffi_rust_future_cancel_rust_buffer(long handle) {
    }
    
    public final native void ffi_grafito_ffi_rust_future_free_rust_buffer(long handle) {
    }
    
    @org.jetbrains.annotations.NotNull()
    public final native uniffi.grafito_ffi.RustBuffer.ByValue ffi_grafito_ffi_rust_future_complete_rust_buffer(long handle, @org.jetbrains.annotations.NotNull()
    uniffi.grafito_ffi.UniffiRustCallStatus uniffi_out_err) {
        return null;
    }
    
    public final native void ffi_grafito_ffi_rust_future_poll_void(long handle, @org.jetbrains.annotations.NotNull()
    uniffi.grafito_ffi.UniffiRustFutureContinuationCallback callback, long callbackData) {
    }
    
    public final native void ffi_grafito_ffi_rust_future_cancel_void(long handle) {
    }
    
    public final native void ffi_grafito_ffi_rust_future_free_void(long handle) {
    }
    
    public final native void ffi_grafito_ffi_rust_future_complete_void(long handle, @org.jetbrains.annotations.NotNull()
    uniffi.grafito_ffi.UniffiRustCallStatus uniffi_out_err) {
    }
}