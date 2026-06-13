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

@kotlin.Metadata(mv = {1, 9, 0}, k = 1, xi = 48, d1 = {"\u0000\u00a6\u0001\n\u0002\u0018\u0002\n\u0002\u0018\u0002\n\u0002\u0018\u0002\n\u0002\u0018\u0002\n\u0000\n\u0002\u0018\u0002\n\u0000\n\u0002\u0010\t\n\u0002\b\u0002\n\u0002\u0018\u0002\n\u0002\b\u0002\n\u0002\u0010\u0007\n\u0002\b\u0003\n\u0002\u0018\u0002\n\u0000\n\u0002\u0018\u0002\n\u0002\b\u0005\n\u0002\u0018\u0002\n\u0002\b\u0003\n\u0002\u0018\u0002\n\u0002\u0018\u0002\n\u0002\b\u0004\n\u0002\u0010\u0002\n\u0002\b\b\n\u0002\u0018\u0002\n\u0002\b\t\n\u0002\u0018\u0002\n\u0000\n\u0002\u0010\u000b\n\u0000\n\u0002\u0010\u000e\n\u0002\b\u0002\n\u0002\u0018\u0002\n\u0000\n\u0002\u0018\u0002\n\u0000\n\u0002\u0018\u0002\n\u0002\b\u000b\n\u0002\u0010 \n\u0002\u0018\u0002\n\u0002\b\u0004\n\u0002\u0018\u0002\n\u0002\b\u000f\n\u0002\u0010\u0006\n\u0002\b\f\b\u0016\u0018\u0000 m2\u00020\u00012\u00020\u00022\u00020\u0003:\u0002mnB\u0017\b\u0016\u0012\u0006\u0010\u0004\u001a\u00020\u0005\u0012\u0006\u0010\u0006\u001a\u00020\u0007\u00a2\u0006\u0002\u0010\bB\u000f\b\u0016\u0012\u0006\u0010\t\u001a\u00020\n\u00a2\u0006\u0002\u0010\u000bB\u0017\b\u0016\u0012\u0006\u0010\f\u001a\u00020\r\u0012\u0006\u0010\u000e\u001a\u00020\r\u00a2\u0006\u0002\u0010\u000fJ<\u0010\u001a\u001a\u0002H\u001b\"\u0004\b\u0000\u0010\u001b2!\u0010\u001c\u001a\u001d\u0012\u0013\u0012\u00110\u0007\u00a2\u0006\f\b\u001e\u0012\b\b\u001f\u0012\u0004\b\b(\u0006\u0012\u0004\u0012\u0002H\u001b0\u001dH\u0080\b\u00f8\u0001\u0000\u00a2\u0006\u0004\b \u0010!J\u0010\u0010\"\u001a\u00020#2\u0006\u0010$\u001a\u00020\rH\u0016J\u0018\u0010%\u001a\u00020#2\u0006\u0010&\u001a\u00020\r2\u0006\u0010\'\u001a\u00020\rH\u0016J\u0018\u0010(\u001a\u00020#2\u0006\u0010)\u001a\u00020\r2\u0006\u0010*\u001a\u00020\rH\u0016J\u0018\u0010+\u001a\u00020,2\u0006\u0010-\u001a\u00020\r2\u0006\u0010.\u001a\u00020\rH\u0016J \u0010/\u001a\u00020#2\u0006\u00100\u001a\u00020\r2\u0006\u00101\u001a\u00020\r2\u0006\u00102\u001a\u00020\rH\u0016J\b\u00103\u001a\u00020#H\u0016J\b\u00104\u001a\u00020#H\u0016J\b\u00105\u001a\u000206H\u0016J\u0010\u00107\u001a\u0002082\u0006\u00109\u001a\u00020:H\u0016J\b\u0010;\u001a\u00020#H\u0016J\b\u0010<\u001a\u00020=H\u0016J\b\u0010>\u001a\u00020?H\u0016J\b\u0010@\u001a\u00020AH\u0016J\b\u0010B\u001a\u000208H\u0016J\u0010\u0010C\u001a\u0002082\u0006\u0010D\u001a\u00020:H\u0016J\u001a\u0010E\u001a\u0004\u0018\u00010:2\u0006\u0010F\u001a\u00020\r2\u0006\u0010G\u001a\u00020\rH\u0016J\u0010\u0010H\u001a\u00020,2\u0006\u0010I\u001a\u00020:H\u0016J\b\u0010J\u001a\u000208H\u0016J\u0010\u0010K\u001a\u0002082\u0006\u0010D\u001a\u00020:H\u0016J\u0016\u0010L\u001a\b\u0012\u0004\u0012\u00020N0M2\u0006\u0010O\u001a\u00020:H\u0016J\u0012\u0010P\u001a\u00020#2\b\u00109\u001a\u0004\u0018\u00010:H\u0016J*\u0010Q\u001a\u00020#2\u0006\u0010R\u001a\u00020S2\u0006\u0010T\u001a\u00020S2\u0006\u0010U\u001a\u00020:H\u0016\u00f8\u0001\u0001\u00a2\u0006\u0004\bV\u0010WJ\u0010\u0010X\u001a\u00020#2\u0006\u0010Y\u001a\u000208H\u0016J(\u0010Z\u001a\u0002082\u0006\u00109\u001a\u00020:2\u0006\u0010[\u001a\u00020\r2\u0006\u0010\\\u001a\u00020\r2\u0006\u0010]\u001a\u00020\rH\u0016J\u0018\u0010^\u001a\u0002082\u0006\u00109\u001a\u00020:2\u0006\u0010_\u001a\u00020:H\u0016J\u0010\u0010`\u001a\u00020#2\u0006\u0010a\u001a\u00020AH\u0016J\u0018\u0010b\u001a\u00020#2\u0006\u0010\u001f\u001a\u00020:2\u0006\u0010U\u001a\u00020cH\u0016J\u0010\u0010d\u001a\u00020#2\u0006\u0010e\u001a\u00020:H\u0016J\u0010\u0010f\u001a\u0002082\u0006\u00109\u001a\u00020:H\u0016J\b\u0010g\u001a\u000208H\u0016J\u0006\u0010h\u001a\u00020\u0007J\u0018\u0010i\u001a\u00020#2\u0006\u0010j\u001a\u00020\r2\u0006\u0010k\u001a\u00020\rH\u0016J\b\u0010l\u001a\u00020#H\u0016R\u000e\u0010\u0010\u001a\u00020\u0011X\u0082\u0004\u00a2\u0006\u0002\n\u0000R\u0016\u0010\u0012\u001a\u0004\u0018\u00010\u0013X\u0084\u0004\u00a2\u0006\b\n\u0000\u001a\u0004\b\u0014\u0010\u0015R\u0014\u0010\u0006\u001a\u00020\u0007X\u0084\u0004\u00a2\u0006\b\n\u0000\u001a\u0004\b\u0016\u0010\u0017R\u000e\u0010\u0018\u001a\u00020\u0019X\u0082\u0004\u00a2\u0006\u0002\n\u0000\u0082\u0002\u000e\n\u0005\b\u009920\u0001\n\u0005\b\u00a1\u001e0\u0001\u00a8\u0006o"}, d2 = {"Luniffi/grafito_ffi/GrafitoEngine;", "Luniffi/grafito_ffi/Disposable;", "Ljava/lang/AutoCloseable;", "Luniffi/grafito_ffi/GrafitoEngineInterface;", "withHandle", "Luniffi/grafito_ffi/UniffiWithHandle;", "handle", "", "(Luniffi/grafito_ffi/UniffiWithHandle;J)V", "noHandle", "Luniffi/grafito_ffi/NoHandle;", "(Luniffi/grafito_ffi/NoHandle;)V", "screenWidth", "", "screenHeight", "(FF)V", "callCounter", "Ljava/util/concurrent/atomic/AtomicLong;", "cleanable", "Luniffi/grafito_ffi/UniffiCleaner$Cleanable;", "getCleanable", "()Luniffi/grafito_ffi/UniffiCleaner$Cleanable;", "getHandle", "()J", "wasDestroyed", "Ljava/util/concurrent/atomic/AtomicBoolean;", "callWithHandle", "R", "block", "Lkotlin/Function1;", "Lkotlin/ParameterName;", "name", "callWithHandle$app_debug", "(Lkotlin/jvm/functions/Function1;)Ljava/lang/Object;", "cameraDolly", "", "delta", "cameraOrbit", "deltaAzimuth", "deltaElevation", "canvasPan", "dx", "dy", "canvasTap", "Luniffi/grafito_ffi/CommandResult;", "x", "y", "canvasZoom", "factor", "centerX", "centerY", "clear", "close", "createCanvasRenderer", "Luniffi/grafito_ffi/CanvasRenderer;", "deleteObject", "", "id", "", "destroy", "getSnapshot", "Luniffi/grafito_ffi/DocumentSnapshot;", "getSpreadsheet", "Luniffi/grafito_ffi/SpreadsheetDto;", "getTool", "Luniffi/grafito_ffi/ToolDto;", "isDarkMode", "loadFromFile", "path", "pickObjectAt", "screenX", "screenY", "processCommand", "input", "redo", "saveToFile", "searchCommands", "", "Luniffi/grafito_ffi/PaletteCommandDto;", "query", "selectObject", "setCell", "row", "Lkotlin/UInt;", "col", "value", "setCell-t3GQkyU", "(IILjava/lang/String;)V", "setDarkMode", "dark", "setObjectColor", "r", "g", "b", "setObjectLabel", "label", "setTool", "tool", "setVariable", "", "setViewMode", "mode", "toggleVisibility", "undo", "uniffiCloneHandle", "updateScreenSize", "width", "height", "zoomToFit", "Companion", "UniffiCleanAction", "app_debug"})
public class GrafitoEngine implements uniffi.grafito_ffi.Disposable, java.lang.AutoCloseable, uniffi.grafito_ffi.GrafitoEngineInterface {
    private final long handle = 0L;
    @org.jetbrains.annotations.Nullable()
    private final uniffi.grafito_ffi.UniffiCleaner.Cleanable cleanable = null;
    @org.jetbrains.annotations.NotNull()
    private final java.util.concurrent.atomic.AtomicBoolean wasDestroyed = null;
    @org.jetbrains.annotations.NotNull()
    private final java.util.concurrent.atomic.AtomicLong callCounter = null;
    @org.jetbrains.annotations.NotNull()
    public static final uniffi.grafito_ffi.GrafitoEngine.Companion Companion = null;
    
    /**
     * @suppress
     */
    @kotlin.Suppress(names = {"UNUSED_PARAMETER"})
    public GrafitoEngine(@org.jetbrains.annotations.NotNull()
    uniffi.grafito_ffi.UniffiWithHandle withHandle, long handle) {
        super();
    }
    
    /**
     * @suppress
     *
     * This constructor can be used to instantiate a fake object. Only used for tests. Any
     * attempt to actually use an object constructed this way will fail as there is no
     * connected Rust object.
     */
    @kotlin.Suppress(names = {"UNUSED_PARAMETER"})
    public GrafitoEngine(@org.jetbrains.annotations.NotNull()
    uniffi.grafito_ffi.NoHandle noHandle) {
        super();
    }
    
    public GrafitoEngine(float screenWidth, float screenHeight) {
        super();
    }
    
    protected final long getHandle() {
        return 0L;
    }
    
    @org.jetbrains.annotations.Nullable()
    protected final uniffi.grafito_ffi.UniffiCleaner.Cleanable getCleanable() {
        return null;
    }
    
    @java.lang.Override()
    public void destroy() {
    }
    
    @java.lang.Override()
    @kotlin.jvm.Synchronized()
    public synchronized void close() {
    }
    
    public final <R extends java.lang.Object>R callWithHandle$app_debug(@org.jetbrains.annotations.NotNull()
    kotlin.jvm.functions.Function1<? super java.lang.Long, ? extends R> block) {
        return null;
    }
    
    /**
     * @suppress
     */
    public final long uniffiCloneHandle() {
        return 0L;
    }
    
    @java.lang.Override()
    public void cameraDolly(float delta) {
    }
    
    @java.lang.Override()
    public void cameraOrbit(float deltaAzimuth, float deltaElevation) {
    }
    
    @java.lang.Override()
    public void canvasPan(float dx, float dy) {
    }
    
    @java.lang.Override()
    @org.jetbrains.annotations.NotNull()
    public uniffi.grafito_ffi.CommandResult canvasTap(float x, float y) {
        return null;
    }
    
    @java.lang.Override()
    public void canvasZoom(float factor, float centerX, float centerY) {
    }
    
    @java.lang.Override()
    public void clear() {
    }
    
    @java.lang.Override()
    @org.jetbrains.annotations.NotNull()
    public uniffi.grafito_ffi.CanvasRenderer createCanvasRenderer() {
        return null;
    }
    
    @java.lang.Override()
    public boolean deleteObject(@org.jetbrains.annotations.NotNull()
    java.lang.String id) {
        return false;
    }
    
    @java.lang.Override()
    @org.jetbrains.annotations.NotNull()
    public uniffi.grafito_ffi.DocumentSnapshot getSnapshot() {
        return null;
    }
    
    @java.lang.Override()
    @org.jetbrains.annotations.NotNull()
    public uniffi.grafito_ffi.SpreadsheetDto getSpreadsheet() {
        return null;
    }
    
    @java.lang.Override()
    @org.jetbrains.annotations.NotNull()
    public uniffi.grafito_ffi.ToolDto getTool() {
        return null;
    }
    
    @java.lang.Override()
    public boolean isDarkMode() {
        return false;
    }
    
    @java.lang.Override()
    public boolean loadFromFile(@org.jetbrains.annotations.NotNull()
    java.lang.String path) {
        return false;
    }
    
    @java.lang.Override()
    @org.jetbrains.annotations.Nullable()
    public java.lang.String pickObjectAt(float screenX, float screenY) {
        return null;
    }
    
    @java.lang.Override()
    @org.jetbrains.annotations.NotNull()
    public uniffi.grafito_ffi.CommandResult processCommand(@org.jetbrains.annotations.NotNull()
    java.lang.String input) {
        return null;
    }
    
    @java.lang.Override()
    public boolean redo() {
        return false;
    }
    
    @java.lang.Override()
    public boolean saveToFile(@org.jetbrains.annotations.NotNull()
    java.lang.String path) {
        return false;
    }
    
    @java.lang.Override()
    @org.jetbrains.annotations.NotNull()
    public java.util.List<uniffi.grafito_ffi.PaletteCommandDto> searchCommands(@org.jetbrains.annotations.NotNull()
    java.lang.String query) {
        return null;
    }
    
    @java.lang.Override()
    public void selectObject(@org.jetbrains.annotations.Nullable()
    java.lang.String id) {
    }
    
    @java.lang.Override()
    public void setDarkMode(boolean dark) {
    }
    
    @java.lang.Override()
    public boolean setObjectColor(@org.jetbrains.annotations.NotNull()
    java.lang.String id, float r, float g, float b) {
        return false;
    }
    
    @java.lang.Override()
    public boolean setObjectLabel(@org.jetbrains.annotations.NotNull()
    java.lang.String id, @org.jetbrains.annotations.NotNull()
    java.lang.String label) {
        return false;
    }
    
    @java.lang.Override()
    public void setTool(@org.jetbrains.annotations.NotNull()
    uniffi.grafito_ffi.ToolDto tool) {
    }
    
    @java.lang.Override()
    public void setVariable(@org.jetbrains.annotations.NotNull()
    java.lang.String name, double value) {
    }
    
    @java.lang.Override()
    public void setViewMode(@org.jetbrains.annotations.NotNull()
    java.lang.String mode) {
    }
    
    @java.lang.Override()
    public boolean toggleVisibility(@org.jetbrains.annotations.NotNull()
    java.lang.String id) {
        return false;
    }
    
    @java.lang.Override()
    public boolean undo() {
        return false;
    }
    
    @java.lang.Override()
    public void updateScreenSize(float width, float height) {
    }
    
    @java.lang.Override()
    public void zoomToFit() {
    }
    
    /**
     * @suppress
     */
    @kotlin.Metadata(mv = {1, 9, 0}, k = 1, xi = 48, d1 = {"\u0000\f\n\u0002\u0018\u0002\n\u0002\u0010\u0000\n\u0002\b\u0002\b\u0086\u0003\u0018\u00002\u00020\u0001B\u0007\b\u0002\u00a2\u0006\u0002\u0010\u0002\u00a8\u0006\u0003"}, d2 = {"Luniffi/grafito_ffi/GrafitoEngine$Companion;", "", "()V", "app_debug"})
    public static final class Companion {
        
        private Companion() {
            super();
        }
    }
    
    @kotlin.Metadata(mv = {1, 9, 0}, k = 1, xi = 48, d1 = {"\u0000\u0018\n\u0002\u0018\u0002\n\u0002\u0018\u0002\n\u0000\n\u0002\u0010\t\n\u0002\b\u0002\n\u0002\u0010\u0002\n\u0000\b\u0002\u0018\u00002\u00020\u0001B\r\u0012\u0006\u0010\u0002\u001a\u00020\u0003\u00a2\u0006\u0002\u0010\u0004J\b\u0010\u0005\u001a\u00020\u0006H\u0016R\u000e\u0010\u0002\u001a\u00020\u0003X\u0082\u0004\u00a2\u0006\u0002\n\u0000\u00a8\u0006\u0007"}, d2 = {"Luniffi/grafito_ffi/GrafitoEngine$UniffiCleanAction;", "Ljava/lang/Runnable;", "handle", "", "(J)V", "run", "", "app_debug"})
    static final class UniffiCleanAction implements java.lang.Runnable {
        private final long handle = 0L;
        
        public UniffiCleanAction(long handle) {
            super();
        }
        
        @java.lang.Override()
        public void run() {
        }
    }
}