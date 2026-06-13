package ai.grafito.app.viewmodel;

import uniffi.grafito_ffi.ToolDto;

@kotlin.Metadata(mv = {1, 9, 0}, k = 1, xi = 48, d1 = {"\u00002\n\u0002\u0018\u0002\n\u0002\u0010\u0000\n\u0000\n\u0002\u0010 \n\u0002\u0018\u0002\n\u0000\n\u0002\u0018\u0002\n\u0000\n\u0002\u0010\u000e\n\u0002\b\u0002\n\u0002\u0010\u000b\n\u0002\b\u0015\n\u0002\u0010\b\n\u0002\b\u0002\b\u0086\b\u0018\u00002\u00020\u0001BO\u0012\u000e\b\u0002\u0010\u0002\u001a\b\u0012\u0004\u0012\u00020\u00040\u0003\u0012\u000e\b\u0002\u0010\u0005\u001a\b\u0012\u0004\u0012\u00020\u00060\u0003\u0012\n\b\u0002\u0010\u0007\u001a\u0004\u0018\u00010\b\u0012\b\b\u0002\u0010\t\u001a\u00020\b\u0012\b\b\u0002\u0010\n\u001a\u00020\u000b\u0012\b\b\u0002\u0010\f\u001a\u00020\u000b\u00a2\u0006\u0002\u0010\rJ\u000f\u0010\u0017\u001a\b\u0012\u0004\u0012\u00020\u00040\u0003H\u00c6\u0003J\u000f\u0010\u0018\u001a\b\u0012\u0004\u0012\u00020\u00060\u0003H\u00c6\u0003J\u000b\u0010\u0019\u001a\u0004\u0018\u00010\bH\u00c6\u0003J\t\u0010\u001a\u001a\u00020\bH\u00c6\u0003J\t\u0010\u001b\u001a\u00020\u000bH\u00c6\u0003J\t\u0010\u001c\u001a\u00020\u000bH\u00c6\u0003JS\u0010\u001d\u001a\u00020\u00002\u000e\b\u0002\u0010\u0002\u001a\b\u0012\u0004\u0012\u00020\u00040\u00032\u000e\b\u0002\u0010\u0005\u001a\b\u0012\u0004\u0012\u00020\u00060\u00032\n\b\u0002\u0010\u0007\u001a\u0004\u0018\u00010\b2\b\b\u0002\u0010\t\u001a\u00020\b2\b\b\u0002\u0010\n\u001a\u00020\u000b2\b\b\u0002\u0010\f\u001a\u00020\u000bH\u00c6\u0001J\u0013\u0010\u001e\u001a\u00020\u000b2\b\u0010\u001f\u001a\u0004\u0018\u00010\u0001H\u00d6\u0003J\t\u0010 \u001a\u00020!H\u00d6\u0001J\t\u0010\"\u001a\u00020\bH\u00d6\u0001R\u0017\u0010\u0002\u001a\b\u0012\u0004\u0012\u00020\u00040\u0003\u00a2\u0006\b\n\u0000\u001a\u0004\b\u000e\u0010\u000fR\u0011\u0010\f\u001a\u00020\u000b\u00a2\u0006\b\n\u0000\u001a\u0004\b\u0010\u0010\u0011R\u0013\u0010\u0007\u001a\u0004\u0018\u00010\b\u00a2\u0006\b\n\u0000\u001a\u0004\b\u0012\u0010\u0013R\u0011\u0010\n\u001a\u00020\u000b\u00a2\u0006\b\n\u0000\u001a\u0004\b\u0014\u0010\u0011R\u0017\u0010\u0005\u001a\b\u0012\u0004\u0012\u00020\u00060\u0003\u00a2\u0006\b\n\u0000\u001a\u0004\b\u0015\u0010\u000fR\u0011\u0010\t\u001a\u00020\b\u00a2\u0006\b\n\u0000\u001a\u0004\b\u0016\u0010\u0013\u00a8\u0006#"}, d2 = {"Lai/grafito/app/viewmodel/DocumentUiState;", "", "objects", "", "Lai/grafito/app/viewmodel/ObjectUiItem;", "variables", "Lai/grafito/app/viewmodel/VariableUiItem;", "selectedId", "", "viewMode", "undoAvailable", "", "redoAvailable", "(Ljava/util/List;Ljava/util/List;Ljava/lang/String;Ljava/lang/String;ZZ)V", "getObjects", "()Ljava/util/List;", "getRedoAvailable", "()Z", "getSelectedId", "()Ljava/lang/String;", "getUndoAvailable", "getVariables", "getViewMode", "component1", "component2", "component3", "component4", "component5", "component6", "copy", "equals", "other", "hashCode", "", "toString", "app_debug"})
public final class DocumentUiState {
    @org.jetbrains.annotations.NotNull()
    private final java.util.List<ai.grafito.app.viewmodel.ObjectUiItem> objects = null;
    @org.jetbrains.annotations.NotNull()
    private final java.util.List<ai.grafito.app.viewmodel.VariableUiItem> variables = null;
    @org.jetbrains.annotations.Nullable()
    private final java.lang.String selectedId = null;
    @org.jetbrains.annotations.NotNull()
    private final java.lang.String viewMode = null;
    private final boolean undoAvailable = false;
    private final boolean redoAvailable = false;
    
    public DocumentUiState(@org.jetbrains.annotations.NotNull()
    java.util.List<ai.grafito.app.viewmodel.ObjectUiItem> objects, @org.jetbrains.annotations.NotNull()
    java.util.List<ai.grafito.app.viewmodel.VariableUiItem> variables, @org.jetbrains.annotations.Nullable()
    java.lang.String selectedId, @org.jetbrains.annotations.NotNull()
    java.lang.String viewMode, boolean undoAvailable, boolean redoAvailable) {
        super();
    }
    
    @org.jetbrains.annotations.NotNull()
    public final java.util.List<ai.grafito.app.viewmodel.ObjectUiItem> getObjects() {
        return null;
    }
    
    @org.jetbrains.annotations.NotNull()
    public final java.util.List<ai.grafito.app.viewmodel.VariableUiItem> getVariables() {
        return null;
    }
    
    @org.jetbrains.annotations.Nullable()
    public final java.lang.String getSelectedId() {
        return null;
    }
    
    @org.jetbrains.annotations.NotNull()
    public final java.lang.String getViewMode() {
        return null;
    }
    
    public final boolean getUndoAvailable() {
        return false;
    }
    
    public final boolean getRedoAvailable() {
        return false;
    }
    
    public DocumentUiState() {
        super();
    }
    
    @org.jetbrains.annotations.NotNull()
    public final java.util.List<ai.grafito.app.viewmodel.ObjectUiItem> component1() {
        return null;
    }
    
    @org.jetbrains.annotations.NotNull()
    public final java.util.List<ai.grafito.app.viewmodel.VariableUiItem> component2() {
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
    public final ai.grafito.app.viewmodel.DocumentUiState copy(@org.jetbrains.annotations.NotNull()
    java.util.List<ai.grafito.app.viewmodel.ObjectUiItem> objects, @org.jetbrains.annotations.NotNull()
    java.util.List<ai.grafito.app.viewmodel.VariableUiItem> variables, @org.jetbrains.annotations.Nullable()
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
}