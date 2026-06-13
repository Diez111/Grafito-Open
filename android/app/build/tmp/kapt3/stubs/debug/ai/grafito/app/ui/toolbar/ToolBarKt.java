package ai.grafito.app.ui.toolbar;

import androidx.compose.foundation.layout.*;
import androidx.compose.material.icons.Icons;
import androidx.compose.material.icons.filled.*;
import androidx.compose.material3.*;
import androidx.compose.runtime.Composable;
import androidx.compose.ui.Alignment;
import androidx.compose.ui.Modifier;
import androidx.compose.ui.graphics.vector.ImageVector;
import uniffi.grafito_ffi.ToolDto;

@kotlin.Metadata(mv = {1, 9, 0}, k = 2, xi = 48, d1 = {"\u0000L\n\u0000\n\u0002\u0010 \n\u0002\u0018\u0002\n\u0002\b\u0003\n\u0002\u0010\u0002\n\u0000\n\u0002\u0018\u0002\n\u0000\n\u0002\u0010\u000e\n\u0000\n\u0002\u0018\u0002\n\u0000\n\u0002\u0018\u0002\n\u0002\b\u0004\n\u0002\u0018\u0002\n\u0000\n\u0002\u0010\u000b\n\u0002\b\u0002\n\u0002\u0018\u0002\n\u0002\b\u0002\n\u0002\u0018\u0002\n\u0002\b\u0005\u001a:\u0010\u0005\u001a\u00020\u00062\u0006\u0010\u0007\u001a\u00020\b2\u0006\u0010\t\u001a\u00020\n2\f\u0010\u000b\u001a\b\u0012\u0004\u0012\u00020\u00060\f2\b\b\u0002\u0010\r\u001a\u00020\u000eH\u0003\u00f8\u0001\u0000\u00a2\u0006\u0004\b\u000f\u0010\u0010\u001aL\u0010\u0011\u001a\u00020\u00062\u0006\u0010\u0012\u001a\u00020\u00132\u0006\u0010\u0014\u001a\u00020\u00152\u0006\u0010\u0016\u001a\u00020\n2\u0012\u0010\u0017\u001a\u000e\u0012\u0004\u0012\u00020\u0013\u0012\u0004\u0012\u00020\u00060\u00182\f\u0010\u0019\u001a\b\u0012\u0004\u0012\u00020\u00060\f2\b\b\u0002\u0010\u001a\u001a\u00020\u001bH\u0007\u001a6\u0010\u001c\u001a\u00020\u00062\u0006\u0010\u001d\u001a\u00020\u00132\u0006\u0010\u0007\u001a\u00020\b2\u0006\u0010\u001e\u001a\u00020\n2\u0006\u0010\u001f\u001a\u00020\u00152\f\u0010\u000b\u001a\b\u0012\u0004\u0012\u00020\u00060\fH\u0003\"\u0014\u0010\u0000\u001a\b\u0012\u0004\u0012\u00020\u00020\u0001X\u0082\u0004\u00a2\u0006\u0002\n\u0000\"\u0014\u0010\u0003\u001a\b\u0012\u0004\u0012\u00020\u00020\u0001X\u0082\u0004\u00a2\u0006\u0002\n\u0000\"\u0014\u0010\u0004\u001a\b\u0012\u0004\u0012\u00020\u00020\u0001X\u0082\u0004\u00a2\u0006\u0002\n\u0000\u0082\u0002\u0007\n\u0005\b\u00a1\u001e0\u0001\u00a8\u0006 "}, d2 = {"tools2D", "", "Lai/grafito/app/ui/toolbar/ToolDef;", "tools3D", "toolsAdvanced", "SmallIconButton", "", "icon", "Landroidx/compose/ui/graphics/vector/ImageVector;", "desc", "", "onClick", "Lkotlin/Function0;", "tint", "Landroidx/compose/ui/graphics/Color;", "SmallIconButton-g2O1Hgs", "(Landroidx/compose/ui/graphics/vector/ImageVector;Ljava/lang/String;Lkotlin/jvm/functions/Function0;J)V", "ToolBar", "currentTool", "Luniffi/grafito_ffi/ToolDto;", "darkMode", "", "viewMode", "onToolSelected", "Lkotlin/Function1;", "onClear", "modifier", "Landroidx/compose/ui/Modifier;", "ToolChip", "tool", "label", "isSelected", "app_debug"})
public final class ToolBarKt {
    @org.jetbrains.annotations.NotNull()
    private static final java.util.List<ai.grafito.app.ui.toolbar.ToolDef> tools2D = null;
    @org.jetbrains.annotations.NotNull()
    private static final java.util.List<ai.grafito.app.ui.toolbar.ToolDef> tools3D = null;
    @org.jetbrains.annotations.NotNull()
    private static final java.util.List<ai.grafito.app.ui.toolbar.ToolDef> toolsAdvanced = null;
    
    @androidx.compose.runtime.Composable()
    public static final void ToolBar(@org.jetbrains.annotations.NotNull()
    uniffi.grafito_ffi.ToolDto currentTool, boolean darkMode, @org.jetbrains.annotations.NotNull()
    java.lang.String viewMode, @org.jetbrains.annotations.NotNull()
    kotlin.jvm.functions.Function1<? super uniffi.grafito_ffi.ToolDto, kotlin.Unit> onToolSelected, @org.jetbrains.annotations.NotNull()
    kotlin.jvm.functions.Function0<kotlin.Unit> onClear, @org.jetbrains.annotations.NotNull()
    androidx.compose.ui.Modifier modifier) {
    }
    
    @androidx.compose.runtime.Composable()
    private static final void ToolChip(uniffi.grafito_ffi.ToolDto tool, androidx.compose.ui.graphics.vector.ImageVector icon, java.lang.String label, boolean isSelected, kotlin.jvm.functions.Function0<kotlin.Unit> onClick) {
    }
}