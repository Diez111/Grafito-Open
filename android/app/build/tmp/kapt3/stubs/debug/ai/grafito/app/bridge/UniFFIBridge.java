package ai.grafito.app.bridge;

import uniffi.grafito_ffi.*;

/**
 * Wrapper limpio sobre los bindings UniFFI.
 * Expone tanto el GrafitoEngine como el CanvasRenderer.
 */
@kotlin.Metadata(mv = {1, 9, 0}, k = 1, xi = 48, d1 = {"\u0000n\n\u0002\u0018\u0002\n\u0002\u0010\u0000\n\u0000\n\u0002\u0010\u0007\n\u0002\b\u0003\n\u0002\u0018\u0002\n\u0002\b\u0003\n\u0002\u0010\u0002\n\u0002\b\u0003\n\u0002\u0018\u0002\n\u0002\b\b\n\u0002\u0018\u0002\n\u0000\n\u0002\u0010\u000b\n\u0000\n\u0002\u0010\u000e\n\u0002\b\u0002\n\u0002\u0018\u0002\n\u0000\n\u0002\u0018\u0002\n\u0000\n\u0002\u0018\u0002\n\u0002\b\b\n\u0002\u0010 \n\u0002\u0018\u0002\n\u0002\b\u0004\n\u0002\u0018\u0002\n\u0002\b\n\n\u0002\u0010\u0006\n\u0002\b\u0005\u0018\u00002\u00020\u0001B\u0019\u0012\b\b\u0002\u0010\u0002\u001a\u00020\u0003\u0012\b\b\u0002\u0010\u0004\u001a\u00020\u0003\u00a2\u0006\u0002\u0010\u0005J\u0016\u0010\n\u001a\u00020\u000b2\u0006\u0010\f\u001a\u00020\u00032\u0006\u0010\r\u001a\u00020\u0003J\u0016\u0010\u000e\u001a\u00020\u000f2\u0006\u0010\u0010\u001a\u00020\u00032\u0006\u0010\u0011\u001a\u00020\u0003J\u001e\u0010\u0012\u001a\u00020\u000b2\u0006\u0010\u0013\u001a\u00020\u00032\u0006\u0010\u0014\u001a\u00020\u00032\u0006\u0010\u0015\u001a\u00020\u0003J\u0006\u0010\u0016\u001a\u00020\u000bJ\u0006\u0010\u0017\u001a\u00020\u0018J\u000e\u0010\u0019\u001a\u00020\u001a2\u0006\u0010\u001b\u001a\u00020\u001cJ\u0006\u0010\u001d\u001a\u00020\u000bJ\u0006\u0010\u001e\u001a\u00020\u001fJ\u0006\u0010 \u001a\u00020!J\u0006\u0010\"\u001a\u00020#J\u0006\u0010$\u001a\u00020\u001aJ\u000e\u0010%\u001a\u00020\u001a2\u0006\u0010&\u001a\u00020\u001cJ\u000e\u0010\'\u001a\u00020\u000f2\u0006\u0010(\u001a\u00020\u001cJ\u0006\u0010)\u001a\u00020\u001aJ\u000e\u0010*\u001a\u00020\u001a2\u0006\u0010&\u001a\u00020\u001cJ\u0014\u0010+\u001a\b\u0012\u0004\u0012\u00020-0,2\u0006\u0010.\u001a\u00020\u001cJ\u0010\u0010/\u001a\u00020\u000b2\b\u0010\u001b\u001a\u0004\u0018\u00010\u001cJ(\u00100\u001a\u00020\u000b2\u0006\u00101\u001a\u0002022\u0006\u00103\u001a\u0002022\u0006\u00104\u001a\u00020\u001c\u00f8\u0001\u0000\u00a2\u0006\u0004\b5\u00106J\u000e\u00107\u001a\u00020\u000b2\u0006\u00108\u001a\u00020\u001aJ\u000e\u00109\u001a\u00020\u000b2\u0006\u0010:\u001a\u00020#J\u0016\u0010;\u001a\u00020\u000b2\u0006\u0010<\u001a\u00020\u001c2\u0006\u00104\u001a\u00020=J\u000e\u0010>\u001a\u00020\u000b2\u0006\u0010?\u001a\u00020\u001cJ\u000e\u0010@\u001a\u00020\u001a2\u0006\u0010\u001b\u001a\u00020\u001cJ\u0006\u0010A\u001a\u00020\u001aR\u0011\u0010\u0006\u001a\u00020\u0007\u00a2\u0006\b\n\u0000\u001a\u0004\b\b\u0010\t\u0082\u0002\u0007\n\u0005\b\u00a1\u001e0\u0001\u00a8\u0006B"}, d2 = {"Lai/grafito/app/bridge/UniFFIBridge;", "", "screenWidth", "", "screenHeight", "(FF)V", "engine", "Luniffi/grafito_ffi/GrafitoEngine;", "getEngine", "()Luniffi/grafito_ffi/GrafitoEngine;", "canvasPan", "", "dx", "dy", "canvasTap", "Luniffi/grafito_ffi/CommandResult;", "x", "y", "canvasZoom", "factor", "centerX", "centerY", "clear", "createCanvasRenderer", "Luniffi/grafito_ffi/CanvasRenderer;", "deleteObject", "", "id", "", "dispose", "getSnapshot", "Luniffi/grafito_ffi/DocumentSnapshot;", "getSpreadsheet", "Luniffi/grafito_ffi/SpreadsheetDto;", "getTool", "Luniffi/grafito_ffi/ToolDto;", "isDarkMode", "loadFromFile", "path", "processCommand", "input", "redo", "saveToFile", "searchCommands", "", "Luniffi/grafito_ffi/PaletteCommandDto;", "query", "selectObject", "setCell", "row", "Lkotlin/UInt;", "col", "value", "setCell-t3GQkyU", "(IILjava/lang/String;)V", "setDarkMode", "dark", "setTool", "tool", "setVariable", "name", "", "setViewMode", "mode", "toggleVisibility", "undo", "app_debug"})
public final class UniFFIBridge {
    @org.jetbrains.annotations.NotNull()
    private final uniffi.grafito_ffi.GrafitoEngine engine = null;
    
    public UniFFIBridge(float screenWidth, float screenHeight) {
        super();
    }
    
    @org.jetbrains.annotations.NotNull()
    public final uniffi.grafito_ffi.GrafitoEngine getEngine() {
        return null;
    }
    
    /**
     * Crea un CanvasRenderer vinculado a este engine
     */
    @org.jetbrains.annotations.NotNull()
    public final uniffi.grafito_ffi.CanvasRenderer createCanvasRenderer() {
        return null;
    }
    
    @org.jetbrains.annotations.NotNull()
    public final uniffi.grafito_ffi.CommandResult processCommand(@org.jetbrains.annotations.NotNull()
    java.lang.String input) {
        return null;
    }
    
    @org.jetbrains.annotations.NotNull()
    public final uniffi.grafito_ffi.DocumentSnapshot getSnapshot() {
        return null;
    }
    
    public final void selectObject(@org.jetbrains.annotations.Nullable()
    java.lang.String id) {
    }
    
    public final boolean toggleVisibility(@org.jetbrains.annotations.NotNull()
    java.lang.String id) {
        return false;
    }
    
    public final boolean deleteObject(@org.jetbrains.annotations.NotNull()
    java.lang.String id) {
        return false;
    }
    
    @org.jetbrains.annotations.NotNull()
    public final uniffi.grafito_ffi.CommandResult canvasTap(float x, float y) {
        return null;
    }
    
    public final void canvasPan(float dx, float dy) {
    }
    
    public final void canvasZoom(float factor, float centerX, float centerY) {
    }
    
    public final void setTool(@org.jetbrains.annotations.NotNull()
    uniffi.grafito_ffi.ToolDto tool) {
    }
    
    @org.jetbrains.annotations.NotNull()
    public final uniffi.grafito_ffi.ToolDto getTool() {
        return null;
    }
    
    public final void setViewMode(@org.jetbrains.annotations.NotNull()
    java.lang.String mode) {
    }
    
    public final void setDarkMode(boolean dark) {
    }
    
    public final boolean isDarkMode() {
        return false;
    }
    
    public final boolean undo() {
        return false;
    }
    
    public final boolean redo() {
        return false;
    }
    
    public final void clear() {
    }
    
    public final void setVariable(@org.jetbrains.annotations.NotNull()
    java.lang.String name, double value) {
    }
    
    @org.jetbrains.annotations.NotNull()
    public final uniffi.grafito_ffi.SpreadsheetDto getSpreadsheet() {
        return null;
    }
    
    @org.jetbrains.annotations.NotNull()
    public final java.util.List<uniffi.grafito_ffi.PaletteCommandDto> searchCommands(@org.jetbrains.annotations.NotNull()
    java.lang.String query) {
        return null;
    }
    
    public final boolean saveToFile(@org.jetbrains.annotations.NotNull()
    java.lang.String path) {
        return false;
    }
    
    public final boolean loadFromFile(@org.jetbrains.annotations.NotNull()
    java.lang.String path) {
        return false;
    }
    
    public final void dispose() {
    }
    
    public UniFFIBridge() {
        super();
    }
}