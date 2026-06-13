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
 * Snapshot completo del documento
 */
@kotlin.Metadata(mv = {1, 9, 0}, k = 1, xi = 48, d1 = {"\u00002\n\u0002\u0018\u0002\n\u0002\u0010\u0000\n\u0000\n\u0002\u0010 \n\u0002\u0018\u0002\n\u0000\n\u0002\u0018\u0002\n\u0000\n\u0002\u0010\u000e\n\u0002\b\u0002\n\u0002\u0010\u000b\n\u0002\b\u001e\n\u0002\u0010\b\n\u0002\b\u0003\b\u0086\b\u0018\u0000 ,2\u00020\u0001:\u0001,BC\u0012\f\u0010\u0002\u001a\b\u0012\u0004\u0012\u00020\u00040\u0003\u0012\f\u0010\u0005\u001a\b\u0012\u0004\u0012\u00020\u00060\u0003\u0012\b\u0010\u0007\u001a\u0004\u0018\u00010\b\u0012\u0006\u0010\t\u001a\u00020\b\u0012\u0006\u0010\n\u001a\u00020\u000b\u0012\u0006\u0010\f\u001a\u00020\u000b\u00a2\u0006\u0002\u0010\rJ\u000f\u0010 \u001a\b\u0012\u0004\u0012\u00020\u00040\u0003H\u00c6\u0003J\u000f\u0010!\u001a\b\u0012\u0004\u0012\u00020\u00060\u0003H\u00c6\u0003J\u000b\u0010\"\u001a\u0004\u0018\u00010\bH\u00c6\u0003J\t\u0010#\u001a\u00020\bH\u00c6\u0003J\t\u0010$\u001a\u00020\u000bH\u00c6\u0003J\t\u0010%\u001a\u00020\u000bH\u00c6\u0003JS\u0010&\u001a\u00020\u00002\u000e\b\u0002\u0010\u0002\u001a\b\u0012\u0004\u0012\u00020\u00040\u00032\u000e\b\u0002\u0010\u0005\u001a\b\u0012\u0004\u0012\u00020\u00060\u00032\n\b\u0002\u0010\u0007\u001a\u0004\u0018\u00010\b2\b\b\u0002\u0010\t\u001a\u00020\b2\b\b\u0002\u0010\n\u001a\u00020\u000b2\b\b\u0002\u0010\f\u001a\u00020\u000bH\u00c6\u0001J\u0013\u0010\'\u001a\u00020\u000b2\b\u0010(\u001a\u0004\u0018\u00010\u0001H\u00d6\u0003J\t\u0010)\u001a\u00020*H\u00d6\u0001J\t\u0010+\u001a\u00020\bH\u00d6\u0001R \u0010\u0002\u001a\b\u0012\u0004\u0012\u00020\u00040\u0003X\u0086\u000e\u00a2\u0006\u000e\n\u0000\u001a\u0004\b\u000e\u0010\u000f\"\u0004\b\u0010\u0010\u0011R\u001a\u0010\f\u001a\u00020\u000bX\u0086\u000e\u00a2\u0006\u000e\n\u0000\u001a\u0004\b\u0012\u0010\u0013\"\u0004\b\u0014\u0010\u0015R\u001c\u0010\u0007\u001a\u0004\u0018\u00010\bX\u0086\u000e\u00a2\u0006\u000e\n\u0000\u001a\u0004\b\u0016\u0010\u0017\"\u0004\b\u0018\u0010\u0019R\u001a\u0010\n\u001a\u00020\u000bX\u0086\u000e\u00a2\u0006\u000e\n\u0000\u001a\u0004\b\u001a\u0010\u0013\"\u0004\b\u001b\u0010\u0015R \u0010\u0005\u001a\b\u0012\u0004\u0012\u00020\u00060\u0003X\u0086\u000e\u00a2\u0006\u000e\n\u0000\u001a\u0004\b\u001c\u0010\u000f\"\u0004\b\u001d\u0010\u0011R\u001a\u0010\t\u001a\u00020\bX\u0086\u000e\u00a2\u0006\u000e\n\u0000\u001a\u0004\b\u001e\u0010\u0017\"\u0004\b\u001f\u0010\u0019\u00a8\u0006-"}, d2 = {"Luniffi/grafito_ffi/DocumentSnapshot;", "", "objects", "", "Luniffi/grafito_ffi/ObjectDto;", "variables", "Luniffi/grafito_ffi/VariableDto;", "selectedId", "", "viewMode", "undoAvailable", "", "redoAvailable", "(Ljava/util/List;Ljava/util/List;Ljava/lang/String;Ljava/lang/String;ZZ)V", "getObjects", "()Ljava/util/List;", "setObjects", "(Ljava/util/List;)V", "getRedoAvailable", "()Z", "setRedoAvailable", "(Z)V", "getSelectedId", "()Ljava/lang/String;", "setSelectedId", "(Ljava/lang/String;)V", "getUndoAvailable", "setUndoAvailable", "getVariables", "setVariables", "getViewMode", "setViewMode", "component1", "component2", "component3", "component4", "component5", "component6", "copy", "equals", "other", "hashCode", "", "toString", "Companion", "app_debug"})
public final class DocumentSnapshot {
    @org.jetbrains.annotations.NotNull()
    private java.util.List<uniffi.grafito_ffi.ObjectDto> objects;
    @org.jetbrains.annotations.NotNull()
    private java.util.List<uniffi.grafito_ffi.VariableDto> variables;
    @org.jetbrains.annotations.Nullable()
    private java.lang.String selectedId;
    @org.jetbrains.annotations.NotNull()
    private java.lang.String viewMode;
    private boolean undoAvailable;
    private boolean redoAvailable;
    @org.jetbrains.annotations.NotNull()
    public static final uniffi.grafito_ffi.DocumentSnapshot.Companion Companion = null;
    
    public DocumentSnapshot(@org.jetbrains.annotations.NotNull()
    java.util.List<uniffi.grafito_ffi.ObjectDto> objects, @org.jetbrains.annotations.NotNull()
    java.util.List<uniffi.grafito_ffi.VariableDto> variables, @org.jetbrains.annotations.Nullable()
    java.lang.String selectedId, @org.jetbrains.annotations.NotNull()
    java.lang.String viewMode, boolean undoAvailable, boolean redoAvailable) {
        super();
    }
    
    @org.jetbrains.annotations.NotNull()
    public final java.util.List<uniffi.grafito_ffi.ObjectDto> getObjects() {
        return null;
    }
    
    public final void setObjects(@org.jetbrains.annotations.NotNull()
    java.util.List<uniffi.grafito_ffi.ObjectDto> p0) {
    }
    
    @org.jetbrains.annotations.NotNull()
    public final java.util.List<uniffi.grafito_ffi.VariableDto> getVariables() {
        return null;
    }
    
    public final void setVariables(@org.jetbrains.annotations.NotNull()
    java.util.List<uniffi.grafito_ffi.VariableDto> p0) {
    }
    
    @org.jetbrains.annotations.Nullable()
    public final java.lang.String getSelectedId() {
        return null;
    }
    
    public final void setSelectedId(@org.jetbrains.annotations.Nullable()
    java.lang.String p0) {
    }
    
    @org.jetbrains.annotations.NotNull()
    public final java.lang.String getViewMode() {
        return null;
    }
    
    public final void setViewMode(@org.jetbrains.annotations.NotNull()
    java.lang.String p0) {
    }
    
    public final boolean getUndoAvailable() {
        return false;
    }
    
    public final void setUndoAvailable(boolean p0) {
    }
    
    public final boolean getRedoAvailable() {
        return false;
    }
    
    public final void setRedoAvailable(boolean p0) {
    }
    
    @org.jetbrains.annotations.NotNull()
    public final java.util.List<uniffi.grafito_ffi.ObjectDto> component1() {
        return null;
    }
    
    @org.jetbrains.annotations.NotNull()
    public final java.util.List<uniffi.grafito_ffi.VariableDto> component2() {
        return null;
    }
    
    @org.jetbrains.annotations.Nullable()
    public final java.lang.String component3() {
        return null;
    }
    
    @org.jetbrains.annotations.NotNull()
    public final java.lang.String component4() {
        return null;
    }
    
    public final boolean component5() {
        return false;
    }
    
    public final boolean component6() {
        return false;
    }
    
    @org.jetbrains.annotations.NotNull()
    public final uniffi.grafito_ffi.DocumentSnapshot copy(@org.jetbrains.annotations.NotNull()
    java.util.List<uniffi.grafito_ffi.ObjectDto> objects, @org.jetbrains.annotations.NotNull()
    java.util.List<uniffi.grafito_ffi.VariableDto> variables, @org.jetbrains.annotations.Nullable()
    java.lang.String selectedId, @org.jetbrains.annotations.NotNull()
    java.lang.String viewMode, boolean undoAvailable, boolean redoAvailable) {
        return null;
    }
    
    @java.lang.Override()
    public boolean equals(@org.jetbrains.annotations.Nullable()
    java.lang.Object other) {
        return false;
    }
    
    @java.lang.Override()
    public int hashCode() {
        return 0;
    }
    
    @java.lang.Override()
    @org.jetbrains.annotations.NotNull()
    public java.lang.String toString() {
        return null;
    }
    
    @kotlin.Metadata(mv = {1, 9, 0}, k = 1, xi = 48, d1 = {"\u0000\f\n\u0002\u0018\u0002\n\u0002\u0010\u0000\n\u0002\b\u0002\b\u0086\u0003\u0018\u00002\u00020\u0001B\u0007\b\u0002\u00a2\u0006\u0002\u0010\u0002\u00a8\u0006\u0003"}, d2 = {"Luniffi/grafito_ffi/DocumentSnapshot$Companion;", "", "()V", "app_debug"})
    public static final class Companion {
        
        private Companion() {
            super();
        }
    }
}