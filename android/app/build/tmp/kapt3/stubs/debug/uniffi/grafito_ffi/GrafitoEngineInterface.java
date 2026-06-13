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

@kotlin.Metadata(mv = {1, 9, 0}, k = 1, xi = 48, d1 = {"\u0000b\n\u0002\u0018\u0002\n\u0002\u0010\u0000\n\u0000\n\u0002\u0010\u0002\n\u0000\n\u0002\u0010\u0007\n\u0002\b\u0007\n\u0002\u0018\u0002\n\u0002\b\b\n\u0002\u0018\u0002\n\u0000\n\u0002\u0010\u000b\n\u0000\n\u0002\u0010\u000e\n\u0000\n\u0002\u0018\u0002\n\u0000\n\u0002\u0018\u0002\n\u0000\n\u0002\u0018\u0002\n\u0002\b\u000b\n\u0002\u0010 \n\u0002\u0018\u0002\n\u0002\b\u0004\n\u0002\u0018\u0002\n\u0002\b\u0010\n\u0002\u0010\u0006\n\u0002\b\n\bf\u0018\u0000 L2\u00020\u0001:\u0001LJ\u0010\u0010\u0002\u001a\u00020\u00032\u0006\u0010\u0004\u001a\u00020\u0005H&J\u0018\u0010\u0006\u001a\u00020\u00032\u0006\u0010\u0007\u001a\u00020\u00052\u0006\u0010\b\u001a\u00020\u0005H&J\u0018\u0010\t\u001a\u00020\u00032\u0006\u0010\n\u001a\u00020\u00052\u0006\u0010\u000b\u001a\u00020\u0005H&J\u0018\u0010\f\u001a\u00020\r2\u0006\u0010\u000e\u001a\u00020\u00052\u0006\u0010\u000f\u001a\u00020\u0005H&J \u0010\u0010\u001a\u00020\u00032\u0006\u0010\u0011\u001a\u00020\u00052\u0006\u0010\u0012\u001a\u00020\u00052\u0006\u0010\u0013\u001a\u00020\u0005H&J\b\u0010\u0014\u001a\u00020\u0003H&J\b\u0010\u0015\u001a\u00020\u0016H&J\u0010\u0010\u0017\u001a\u00020\u00182\u0006\u0010\u0019\u001a\u00020\u001aH&J\b\u0010\u001b\u001a\u00020\u001cH&J\b\u0010\u001d\u001a\u00020\u001eH&J\b\u0010\u001f\u001a\u00020 H&J\b\u0010!\u001a\u00020\u0018H&J\u0010\u0010\"\u001a\u00020\u00182\u0006\u0010#\u001a\u00020\u001aH&J\u001a\u0010$\u001a\u0004\u0018\u00010\u001a2\u0006\u0010%\u001a\u00020\u00052\u0006\u0010&\u001a\u00020\u0005H&J\u0010\u0010\'\u001a\u00020\r2\u0006\u0010(\u001a\u00020\u001aH&J\b\u0010)\u001a\u00020\u0018H&J\u0010\u0010*\u001a\u00020\u00182\u0006\u0010#\u001a\u00020\u001aH&J\u0016\u0010+\u001a\b\u0012\u0004\u0012\u00020-0,2\u0006\u0010.\u001a\u00020\u001aH&J\u0012\u0010/\u001a\u00020\u00032\b\u0010\u0019\u001a\u0004\u0018\u00010\u001aH&J*\u00100\u001a\u00020\u00032\u0006\u00101\u001a\u0002022\u0006\u00103\u001a\u0002022\u0006\u00104\u001a\u00020\u001aH&\u00f8\u0001\u0000\u00a2\u0006\u0004\b5\u00106J\u0010\u00107\u001a\u00020\u00032\u0006\u00108\u001a\u00020\u0018H&J(\u00109\u001a\u00020\u00182\u0006\u0010\u0019\u001a\u00020\u001a2\u0006\u0010:\u001a\u00020\u00052\u0006\u0010;\u001a\u00020\u00052\u0006\u0010<\u001a\u00020\u0005H&J\u0018\u0010=\u001a\u00020\u00182\u0006\u0010\u0019\u001a\u00020\u001a2\u0006\u0010>\u001a\u00020\u001aH&J\u0010\u0010?\u001a\u00020\u00032\u0006\u0010@\u001a\u00020 H&J\u0018\u0010A\u001a\u00020\u00032\u0006\u0010B\u001a\u00020\u001a2\u0006\u00104\u001a\u00020CH&J\u0010\u0010D\u001a\u00020\u00032\u0006\u0010E\u001a\u00020\u001aH&J\u0010\u0010F\u001a\u00020\u00182\u0006\u0010\u0019\u001a\u00020\u001aH&J\b\u0010G\u001a\u00020\u0018H&J\u0018\u0010H\u001a\u00020\u00032\u0006\u0010I\u001a\u00020\u00052\u0006\u0010J\u001a\u00020\u0005H&J\b\u0010K\u001a\u00020\u0003H&\u0082\u0002\u0007\n\u0005\b\u00a1\u001e0\u0001\u00a8\u0006M"}, d2 = {"Luniffi/grafito_ffi/GrafitoEngineInterface;", "", "cameraDolly", "", "delta", "", "cameraOrbit", "deltaAzimuth", "deltaElevation", "canvasPan", "dx", "dy", "canvasTap", "Luniffi/grafito_ffi/CommandResult;", "x", "y", "canvasZoom", "factor", "centerX", "centerY", "clear", "createCanvasRenderer", "Luniffi/grafito_ffi/CanvasRenderer;", "deleteObject", "", "id", "", "getSnapshot", "Luniffi/grafito_ffi/DocumentSnapshot;", "getSpreadsheet", "Luniffi/grafito_ffi/SpreadsheetDto;", "getTool", "Luniffi/grafito_ffi/ToolDto;", "isDarkMode", "loadFromFile", "path", "pickObjectAt", "screenX", "screenY", "processCommand", "input", "redo", "saveToFile", "searchCommands", "", "Luniffi/grafito_ffi/PaletteCommandDto;", "query", "selectObject", "setCell", "row", "Lkotlin/UInt;", "col", "value", "setCell-t3GQkyU", "(IILjava/lang/String;)V", "setDarkMode", "dark", "setObjectColor", "r", "g", "b", "setObjectLabel", "label", "setTool", "tool", "setVariable", "name", "", "setViewMode", "mode", "toggleVisibility", "undo", "updateScreenSize", "width", "height", "zoomToFit", "Companion", "app_debug"})
public abstract interface GrafitoEngineInterface {
    @org.jetbrains.annotations.NotNull()
    public static final uniffi.grafito_ffi.GrafitoEngineInterface.Companion Companion = null;
    
    public abstract void cameraDolly(float delta);
    
    public abstract void cameraOrbit(float deltaAzimuth, float deltaElevation);
    
    public abstract void canvasPan(float dx, float dy);
    
    @org.jetbrains.annotations.NotNull()
    public abstract uniffi.grafito_ffi.CommandResult canvasTap(float x, float y);
    
    public abstract void canvasZoom(float factor, float centerX, float centerY);
    
    public abstract void clear();
    
    @org.jetbrains.annotations.NotNull()
    public abstract uniffi.grafito_ffi.CanvasRenderer createCanvasRenderer();
    
    public abstract boolean deleteObject(@org.jetbrains.annotations.NotNull()
    java.lang.String id);
    
    @org.jetbrains.annotations.NotNull()
    public abstract uniffi.grafito_ffi.DocumentSnapshot getSnapshot();
    
    @org.jetbrains.annotations.NotNull()
    public abstract uniffi.grafito_ffi.SpreadsheetDto getSpreadsheet();
    
    @org.jetbrains.annotations.NotNull()
    public abstract uniffi.grafito_ffi.ToolDto getTool();
    
    public abstract boolean isDarkMode();
    
    public abstract boolean loadFromFile(@org.jetbrains.annotations.NotNull()
    java.lang.String path);
    
    @org.jetbrains.annotations.Nullable()
    public abstract java.lang.String pickObjectAt(float screenX, float screenY);
    
    @org.jetbrains.annotations.NotNull()
    public abstract uniffi.grafito_ffi.CommandResult processCommand(@org.jetbrains.annotations.NotNull()
    java.lang.String input);
    
    public abstract boolean redo();
    
    public abstract boolean saveToFile(@org.jetbrains.annotations.NotNull()
    java.lang.String path);
    
    @org.jetbrains.annotations.NotNull()
    public abstract java.util.List<uniffi.grafito_ffi.PaletteCommandDto> searchCommands(@org.jetbrains.annotations.NotNull()
    java.lang.String query);
    
    public abstract void selectObject(@org.jetbrains.annotations.Nullable()
    java.lang.String id);
    
    public abstract void setDarkMode(boolean dark);
    
    public abstract boolean setObjectColor(@org.jetbrains.annotations.NotNull()
    java.lang.String id, float r, float g, float b);
    
    public abstract boolean setObjectLabel(@org.jetbrains.annotations.NotNull()
    java.lang.String id, @org.jetbrains.annotations.NotNull()
    java.lang.String label);
    
    public abstract void setTool(@org.jetbrains.annotations.NotNull()
    uniffi.grafito_ffi.ToolDto tool);
    
    public abstract void setVariable(@org.jetbrains.annotations.NotNull()
    java.lang.String name, double value);
    
    public abstract void setViewMode(@org.jetbrains.annotations.NotNull()
    java.lang.String mode);
    
    public abstract boolean toggleVisibility(@org.jetbrains.annotations.NotNull()
    java.lang.String id);
    
    public abstract boolean undo();
    
    public abstract void updateScreenSize(float width, float height);
    
    public abstract void zoomToFit();
    
    @kotlin.Metadata(mv = {1, 9, 0}, k = 1, xi = 48, d1 = {"\u0000\f\n\u0002\u0018\u0002\n\u0002\u0010\u0000\n\u0002\b\u0002\b\u0086\u0003\u0018\u00002\u00020\u0001B\u0007\b\u0002\u00a2\u0006\u0002\u0010\u0002\u00a8\u0006\u0003"}, d2 = {"Luniffi/grafito_ffi/GrafitoEngineInterface$Companion;", "", "()V", "app_debug"})
    public static final class Companion {
        
        private Companion() {
            super();
        }
    }
}