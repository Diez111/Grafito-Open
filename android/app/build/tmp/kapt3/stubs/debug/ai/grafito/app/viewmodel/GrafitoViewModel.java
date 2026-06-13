package ai.grafito.app.viewmodel;

import androidx.lifecycle.ViewModel;
import ai.grafito.app.bridge.UniFFIBridge;
import uniffi.grafito_ffi.*;
import dagger.hilt.android.lifecycle.HiltViewModel;
import kotlinx.coroutines.Dispatchers;
import javax.inject.Inject;

@kotlin.Metadata(mv = {1, 9, 0}, k = 1, xi = 48, d1 = {"\u0000d\n\u0002\u0018\u0002\n\u0002\u0018\u0002\n\u0000\n\u0002\u0018\u0002\n\u0002\b\u0002\n\u0002\u0018\u0002\n\u0002\b\u0007\n\u0002\u0018\u0002\n\u0002\b\u0006\n\u0002\u0010\u000e\n\u0002\b\u0007\n\u0002\u0010\u0002\n\u0000\n\u0002\u0010\u0007\n\u0002\b\r\n\u0002\u0018\u0002\n\u0002\b\u0002\n\u0002\u0010\u000b\n\u0002\b\u0006\n\u0002\u0010 \n\u0002\u0018\u0002\n\u0002\b\u0004\n\u0002\u0018\u0002\n\u0002\b\u0003\n\u0002\u0010\u0006\n\u0002\b\u0006\b\u0007\u0018\u00002\u00020\u0001B\u000f\b\u0007\u0012\u0006\u0010\u0002\u001a\u00020\u0003\u00a2\u0006\u0002\u0010\u0004J\u0016\u0010\u001c\u001a\u00020\u001d2\u0006\u0010\u001e\u001a\u00020\u001f2\u0006\u0010 \u001a\u00020\u001fJ\u0016\u0010!\u001a\u00020\u001d2\u0006\u0010\"\u001a\u00020\u001f2\u0006\u0010#\u001a\u00020\u001fJ\u001e\u0010$\u001a\u00020\u001d2\u0006\u0010%\u001a\u00020\u001f2\u0006\u0010&\u001a\u00020\u001f2\u0006\u0010\'\u001a\u00020\u001fJ\u0006\u0010(\u001a\u00020\u001dJ\u0006\u0010)\u001a\u00020\u001dJ\u000e\u0010*\u001a\u00020\u001d2\u0006\u0010+\u001a\u00020\u0015J\u0006\u0010,\u001a\u00020-J\u000e\u0010.\u001a\u00020\u001d2\u0006\u0010/\u001a\u000200J\b\u00101\u001a\u00020\u001dH\u0014J\u000e\u00102\u001a\u00020\u001d2\u0006\u00103\u001a\u00020\u0015J\u0006\u00104\u001a\u00020\u001dJ\b\u00105\u001a\u00020\u001dH\u0002J\u0014\u00106\u001a\b\u0012\u0004\u0012\u000208072\u0006\u00109\u001a\u00020\u0015J\u0010\u0010:\u001a\u00020\u001d2\b\u0010+\u001a\u0004\u0018\u00010\u0015J\u000e\u0010;\u001a\u00020\u001d2\u0006\u0010<\u001a\u00020=J\u0016\u0010>\u001a\u00020\u001d2\u0006\u0010?\u001a\u00020\u00152\u0006\u0010@\u001a\u00020AJ\u000e\u0010B\u001a\u00020\u001d2\u0006\u0010C\u001a\u00020\u0015J\u0006\u0010D\u001a\u00020\u001dJ\u000e\u0010E\u001a\u00020\u001d2\u0006\u0010+\u001a\u00020\u0015J\u0006\u0010F\u001a\u00020\u001dR\u000e\u0010\u0002\u001a\u00020\u0003X\u0082\u0004\u00a2\u0006\u0002\n\u0000R+\u0010\u0007\u001a\u00020\u00062\u0006\u0010\u0005\u001a\u00020\u00068F@BX\u0086\u008e\u0002\u00a2\u0006\u0012\n\u0004\b\f\u0010\r\u001a\u0004\b\b\u0010\t\"\u0004\b\n\u0010\u000bR+\u0010\u000f\u001a\u00020\u000e2\u0006\u0010\u0005\u001a\u00020\u000e8F@BX\u0086\u008e\u0002\u00a2\u0006\u0012\n\u0004\b\u0014\u0010\r\u001a\u0004\b\u0010\u0010\u0011\"\u0004\b\u0012\u0010\u0013R/\u0010\u0016\u001a\u0004\u0018\u00010\u00152\b\u0010\u0005\u001a\u0004\u0018\u00010\u00158F@BX\u0086\u008e\u0002\u00a2\u0006\u0012\n\u0004\b\u001b\u0010\r\u001a\u0004\b\u0017\u0010\u0018\"\u0004\b\u0019\u0010\u001a\u00a8\u0006G"}, d2 = {"Lai/grafito/app/viewmodel/GrafitoViewModel;", "Landroidx/lifecycle/ViewModel;", "bridge", "Lai/grafito/app/bridge/UniFFIBridge;", "(Lai/grafito/app/bridge/UniFFIBridge;)V", "<set-?>", "Lai/grafito/app/viewmodel/CanvasUiState;", "canvasState", "getCanvasState", "()Lai/grafito/app/viewmodel/CanvasUiState;", "setCanvasState", "(Lai/grafito/app/viewmodel/CanvasUiState;)V", "canvasState$delegate", "Landroidx/compose/runtime/MutableState;", "Lai/grafito/app/viewmodel/DocumentUiState;", "documentState", "getDocumentState", "()Lai/grafito/app/viewmodel/DocumentUiState;", "setDocumentState", "(Lai/grafito/app/viewmodel/DocumentUiState;)V", "documentState$delegate", "", "toastMessage", "getToastMessage", "()Ljava/lang/String;", "setToastMessage", "(Ljava/lang/String;)V", "toastMessage$delegate", "canvasPan", "", "dx", "", "dy", "canvasTap", "x", "y", "canvasZoom", "factor", "centerX", "centerY", "clearAll", "clearToast", "deleteObject", "id", "getEngine", "Luniffi/grafito_ffi/GrafitoEngine;", "initDarkMode", "dark", "", "onCleared", "processCommand", "input", "redo", "refreshSnapshot", "searchCommands", "", "Lai/grafito/app/viewmodel/CommandPaletteItem;", "query", "selectObject", "setTool", "tool", "Luniffi/grafito_ffi/ToolDto;", "setVariable", "name", "value", "", "setViewMode", "mode", "toggleDarkMode", "toggleVisibility", "undo", "app_debug"})
@dagger.hilt.android.lifecycle.HiltViewModel()
public final class GrafitoViewModel extends androidx.lifecycle.ViewModel {
    @org.jetbrains.annotations.NotNull()
    private final ai.grafito.app.bridge.UniFFIBridge bridge = null;
    @org.jetbrains.annotations.NotNull()
    private final androidx.compose.runtime.MutableState documentState$delegate = null;
    @org.jetbrains.annotations.NotNull()
    private final androidx.compose.runtime.MutableState canvasState$delegate = null;
    @org.jetbrains.annotations.NotNull()
    private final androidx.compose.runtime.MutableState toastMessage$delegate = null;
    
    @javax.inject.Inject()
    public GrafitoViewModel(@org.jetbrains.annotations.NotNull()
    ai.grafito.app.bridge.UniFFIBridge bridge) {
        super();
    }
    
    @org.jetbrains.annotations.NotNull()
    public final ai.grafito.app.viewmodel.DocumentUiState getDocumentState() {
        return null;
    }
    
    private final void setDocumentState(ai.grafito.app.viewmodel.DocumentUiState p0) {
    }
    
    @org.jetbrains.annotations.NotNull()
    public final ai.grafito.app.viewmodel.CanvasUiState getCanvasState() {
        return null;
    }
    
    private final void setCanvasState(ai.grafito.app.viewmodel.CanvasUiState p0) {
    }
    
    @org.jetbrains.annotations.Nullable()
    public final java.lang.String getToastMessage() {
        return null;
    }
    
    private final void setToastMessage(java.lang.String p0) {
    }
    
    public final void clearToast() {
    }
    
    public final void initDarkMode(boolean dark) {
    }
    
    @org.jetbrains.annotations.NotNull()
    public final uniffi.grafito_ffi.GrafitoEngine getEngine() {
        return null;
    }
    
    public final void processCommand(@org.jetbrains.annotations.NotNull()
    java.lang.String input) {
    }
    
    public final void selectObject(@org.jetbrains.annotations.Nullable()
    java.lang.String id) {
    }
    
    public final void deleteObject(@org.jetbrains.annotations.NotNull()
    java.lang.String id) {
    }
    
    public final void toggleVisibility(@org.jetbrains.annotations.NotNull()
    java.lang.String id) {
    }
    
    public final void canvasTap(float x, float y) {
    }
    
    public final void canvasPan(float dx, float dy) {
    }
    
    public final void canvasZoom(float factor, float centerX, float centerY) {
    }
    
    public final void setTool(@org.jetbrains.annotations.NotNull()
    uniffi.grafito_ffi.ToolDto tool) {
    }
    
    public final void setViewMode(@org.jetbrains.annotations.NotNull()
    java.lang.String mode) {
    }
    
    public final void toggleDarkMode() {
    }
    
    public final void undo() {
    }
    
    public final void redo() {
    }
    
    public final void clearAll() {
    }
    
    public final void setVariable(@org.jetbrains.annotations.NotNull()
    java.lang.String name, double value) {
    }
    
    @org.jetbrains.annotations.NotNull()
    public final java.util.List<ai.grafito.app.viewmodel.CommandPaletteItem> searchCommands(@org.jetbrains.annotations.NotNull()
    java.lang.String query) {
        return null;
    }
    
    private final void refreshSnapshot() {
    }
    
    @java.lang.Override()
    protected void onCleared() {
    }
}