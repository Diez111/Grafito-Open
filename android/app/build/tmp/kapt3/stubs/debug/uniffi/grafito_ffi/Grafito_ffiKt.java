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

@kotlin.Metadata(mv = {1, 9, 0}, k = 2, xi = 48, d1 = {"\u0000p\n\u0000\n\u0002\u0010\u0005\n\u0002\b\u0003\n\u0002\u0010\t\n\u0002\b\u0002\n\u0002\u0010\u000e\n\u0002\b\u0002\n\u0002\u0010\u0002\n\u0000\n\u0002\u0018\u0002\n\u0002\b\u0002\n\u0002\u0018\u0002\n\u0002\u0018\u0002\n\u0000\n\u0002\u0018\u0002\n\u0000\n\u0002\u0018\u0002\n\u0002\b\u0005\n\u0002\u0018\u0002\n\u0002\b\u0007\n\u0002\u0018\u0002\n\u0002\b\u0002\n\u0002\u0010\u0003\n\u0000\n\u0002\u0018\u0002\n\u0000\n\u0002\u0018\u0002\n\u0002\u0018\u0002\n\u0002\b\u0002\n\u0002\u0018\u0002\n\u0002\b\u0003\u001a\u0010\u0010\u0007\u001a\u00020\b2\u0006\u0010\t\u001a\u00020\bH\u0002\u001a\u0010\u0010\n\u001a\u00020\u000b2\u0006\u0010\f\u001a\u00020\rH\u0002\u001a,\u0010\u000e\u001a\u00020\u000b\"\f\b\u0000\u0010\u000f*\u00060\u0010j\u0002`\u00112\f\u0010\u0012\u001a\b\u0012\u0004\u0012\u0002H\u000f0\u00132\u0006\u0010\u0014\u001a\u00020\u0015H\u0002\u001a\u0010\u0010\u0016\u001a\u00020\u000b2\u0006\u0010\f\u001a\u00020\rH\u0002\u001a\u0006\u0010\u0017\u001a\u00020\u000b\u001a(\u0010\u0018\u001a\u0002H\u0019\"\u0004\b\u0000\u0010\u00192\u0012\u0010\u001a\u001a\u000e\u0012\u0004\u0012\u00020\u0015\u0012\u0004\u0012\u0002H\u00190\u001bH\u0082\b\u00a2\u0006\u0002\u0010\u001c\u001aD\u0010\u001d\u001a\u0002H\u0019\"\u0004\b\u0000\u0010\u0019\"\f\b\u0001\u0010\u000f*\u00060\u0010j\u0002`\u00112\f\u0010\u0012\u001a\b\u0012\u0004\u0012\u0002H\u000f0\u00132\u0012\u0010\u001a\u001a\u000e\u0012\u0004\u0012\u00020\u0015\u0012\u0004\u0012\u0002H\u00190\u001bH\u0082\b\u00a2\u0006\u0002\u0010\u001e\u001a<\u0010\u001f\u001a\u00020\u000b\"\u0004\b\u0000\u0010 2\u0006\u0010!\u001a\u00020\u00152\f\u0010\"\u001a\b\u0012\u0004\u0012\u0002H 0#2\u0012\u0010$\u001a\u000e\u0012\u0004\u0012\u0002H \u0012\u0004\u0012\u00020\u000b0\u001bH\u0080\b\u00f8\u0001\u0000\u001a\\\u0010%\u001a\u00020\u000b\"\u0004\b\u0000\u0010 \"\n\b\u0001\u0010\u000f\u0018\u0001*\u00020&2\u0006\u0010!\u001a\u00020\u00152\f\u0010\"\u001a\b\u0012\u0004\u0012\u0002H 0#2\u0012\u0010$\u001a\u000e\u0012\u0004\u0012\u0002H \u0012\u0004\u0012\u00020\u000b0\u001b2\u0012\u0010\'\u001a\u000e\u0012\u0004\u0012\u0002H\u000f\u0012\u0004\u0012\u00020(0\u001bH\u0080\b\u00f8\u0001\u0000\u001a\f\u0010)\u001a\u00020**\u00020+H\u0002\u001a;\u0010,\u001a\u0002H-\"\n\b\u0000\u0010 *\u0004\u0018\u00010.\"\u0004\b\u0001\u0010-*\u0002H 2\u0012\u0010/\u001a\u000e\u0012\u0004\u0012\u0002H \u0012\u0004\u0012\u0002H-0\u001bH\u0086\b\u00f8\u0001\u0000\u00a2\u0006\u0002\u00100\"\u000e\u0010\u0000\u001a\u00020\u0001X\u0080T\u00a2\u0006\u0002\n\u0000\"\u000e\u0010\u0002\u001a\u00020\u0001X\u0080T\u00a2\u0006\u0002\n\u0000\"\u000e\u0010\u0003\u001a\u00020\u0001X\u0080T\u00a2\u0006\u0002\n\u0000\"\u000e\u0010\u0004\u001a\u00020\u0005X\u0082T\u00a2\u0006\u0002\n\u0000\"\u000e\u0010\u0006\u001a\u00020\u0005X\u0082T\u00a2\u0006\u0002\n\u0000\u0082\u0002\u0007\n\u0005\b\u009920\u0001\u00a8\u00061"}, d2 = {"UNIFFI_CALL_ERROR", "", "UNIFFI_CALL_SUCCESS", "UNIFFI_CALL_UNEXPECTED_ERROR", "UNIFFI_HANDLEMAP_DELTA", "", "UNIFFI_HANDLEMAP_INITIAL", "findLibraryName", "", "componentName", "uniffiCheckApiChecksums", "", "lib", "Luniffi/grafito_ffi/IntegrityCheckingUniffiLib;", "uniffiCheckCallStatus", "E", "Ljava/lang/Exception;", "Lkotlin/Exception;", "errorHandler", "Luniffi/grafito_ffi/UniffiRustCallStatusErrorHandler;", "status", "Luniffi/grafito_ffi/UniffiRustCallStatus;", "uniffiCheckContractApiVersion", "uniffiEnsureInitialized", "uniffiRustCall", "U", "callback", "Lkotlin/Function1;", "(Lkotlin/jvm/functions/Function1;)Ljava/lang/Object;", "uniffiRustCallWithError", "(Luniffi/grafito_ffi/UniffiRustCallStatusErrorHandler;Lkotlin/jvm/functions/Function1;)Ljava/lang/Object;", "uniffiTraitInterfaceCall", "T", "callStatus", "makeCall", "Lkotlin/Function0;", "writeReturn", "uniffiTraitInterfaceCallWithError", "", "lowerError", "Luniffi/grafito_ffi/RustBuffer$ByValue;", "create", "Luniffi/grafito_ffi/UniffiCleaner;", "Luniffi/grafito_ffi/UniffiCleaner$Companion;", "use", "R", "Luniffi/grafito_ffi/Disposable;", "block", "(Luniffi/grafito_ffi/Disposable;Lkotlin/jvm/functions/Function1;)Ljava/lang/Object;", "app_debug"})
@kotlin.Suppress(names = {"NAME_SHADOWING"})
public final class Grafito_ffiKt {
    public static final byte UNIFFI_CALL_SUCCESS = (byte)0;
    public static final byte UNIFFI_CALL_ERROR = (byte)1;
    public static final byte UNIFFI_CALL_UNEXPECTED_ERROR = (byte)2;
    private static final long UNIFFI_HANDLEMAP_INITIAL = 1L;
    private static final long UNIFFI_HANDLEMAP_DELTA = 2L;
    
    private static final <U extends java.lang.Object, E extends java.lang.Exception>U uniffiRustCallWithError(uniffi.grafito_ffi.UniffiRustCallStatusErrorHandler<E> errorHandler, kotlin.jvm.functions.Function1<? super uniffi.grafito_ffi.UniffiRustCallStatus, ? extends U> callback) {
        return null;
    }
    
    private static final <E extends java.lang.Exception>void uniffiCheckCallStatus(uniffi.grafito_ffi.UniffiRustCallStatusErrorHandler<E> errorHandler, uniffi.grafito_ffi.UniffiRustCallStatus status) {
    }
    
    private static final <U extends java.lang.Object>U uniffiRustCall(kotlin.jvm.functions.Function1<? super uniffi.grafito_ffi.UniffiRustCallStatus, ? extends U> callback) {
        return null;
    }
    
    public static final <T extends java.lang.Object>void uniffiTraitInterfaceCall(@org.jetbrains.annotations.NotNull()
    uniffi.grafito_ffi.UniffiRustCallStatus callStatus, @org.jetbrains.annotations.NotNull()
    kotlin.jvm.functions.Function0<? extends T> makeCall, @org.jetbrains.annotations.NotNull()
    kotlin.jvm.functions.Function1<? super T, kotlin.Unit> writeReturn) {
    }
    
    @kotlin.jvm.Synchronized()
    private static final synchronized java.lang.String findLibraryName(java.lang.String componentName) {
        return null;
    }
    
    private static final void uniffiCheckContractApiVersion(uniffi.grafito_ffi.IntegrityCheckingUniffiLib lib) {
    }
    
    @kotlin.Suppress(names = {"UNUSED_PARAMETER"})
    private static final void uniffiCheckApiChecksums(uniffi.grafito_ffi.IntegrityCheckingUniffiLib lib) {
    }
    
    /**
     * @suppress
     */
    public static final void uniffiEnsureInitialized() {
    }
    
    /**
     * @suppress
     */
    public static final <T extends uniffi.grafito_ffi.Disposable, R extends java.lang.Object>R use(T $this$use, @org.jetbrains.annotations.NotNull()
    kotlin.jvm.functions.Function1<? super T, ? extends R> block) {
        return null;
    }
    
    private static final uniffi.grafito_ffi.UniffiCleaner create(uniffi.grafito_ffi.UniffiCleaner.Companion $this$create) {
        return null;
    }
}