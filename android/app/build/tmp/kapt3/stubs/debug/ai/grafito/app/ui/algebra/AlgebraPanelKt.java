package ai.grafito.app.ui.algebra;

import androidx.compose.foundation.layout.*;
import androidx.compose.material.icons.Icons;
import androidx.compose.material3.*;
import androidx.compose.runtime.*;
import androidx.compose.ui.Alignment;
import androidx.compose.ui.Modifier;
import ai.grafito.app.viewmodel.GrafitoViewModel;
import ai.grafito.app.viewmodel.ObjectUiItem;

@kotlin.Metadata(mv = {1, 9, 0}, k = 2, xi = 48, d1 = {"\u0000>\n\u0000\n\u0002\u0010\u0002\n\u0000\n\u0002\u0018\u0002\n\u0000\n\u0002\u0010\u000e\n\u0000\n\u0002\u0018\u0002\n\u0000\n\u0002\u0010\u000b\n\u0000\n\u0002\u0018\u0002\n\u0000\n\u0002\u0018\u0002\n\u0002\b\u0002\n\u0002\u0018\u0002\n\u0002\b\u0007\n\u0002\u0010\u0006\n\u0002\b\u0004\u001aP\u0010\u0000\u001a\u00020\u00012\u0006\u0010\u0002\u001a\u00020\u00032\u0006\u0010\u0004\u001a\u00020\u00052\u0012\u0010\u0006\u001a\u000e\u0012\u0004\u0012\u00020\u0005\u0012\u0004\u0012\u00020\u00010\u00072\b\b\u0002\u0010\b\u001a\u00020\t2\u000e\b\u0002\u0010\n\u001a\b\u0012\u0004\u0012\u00020\u00010\u000b2\b\b\u0002\u0010\f\u001a\u00020\rH\u0007\u001aB\u0010\u000e\u001a\u00020\u00012\u0006\u0010\u000f\u001a\u00020\u00102\u0006\u0010\u0011\u001a\u00020\t2\f\u0010\u0012\u001a\b\u0012\u0004\u0012\u00020\u00010\u000b2\f\u0010\u0013\u001a\b\u0012\u0004\u0012\u00020\u00010\u000b2\f\u0010\u0014\u001a\b\u0012\u0004\u0012\u00020\u00010\u000bH\u0007\u001aF\u0010\u0015\u001a\u00020\u00012\u0006\u0010\u0016\u001a\u00020\u00052\u0006\u0010\u0017\u001a\u00020\u00182\u0006\u0010\u0019\u001a\u00020\u00182\u0006\u0010\u001a\u001a\u00020\u00182\u0012\u0010\u001b\u001a\u000e\u0012\u0004\u0012\u00020\u0018\u0012\u0004\u0012\u00020\u00010\u00072\b\b\u0002\u0010\f\u001a\u00020\rH\u0003\u00a8\u0006\u001c"}, d2 = {"AlgebraPanel", "", "viewModel", "Lai/grafito/app/viewmodel/GrafitoViewModel;", "cmdInput", "", "onCmdInputChange", "Lkotlin/Function1;", "showMathKb", "", "onToggleMathKb", "Lkotlin/Function0;", "modifier", "Landroidx/compose/ui/Modifier;", "ObjectItem", "obj", "Lai/grafito/app/viewmodel/ObjectUiItem;", "isSelected", "onClick", "onToggleVisibility", "onDelete", "VariableSlider", "name", "value", "", "min", "max", "onValueChange", "app_debug"})
public final class AlgebraPanelKt {
    
    @kotlin.OptIn(markerClass = {androidx.compose.material3.ExperimentalMaterial3Api.class})
    @androidx.compose.runtime.Composable()
    public static final void AlgebraPanel(@org.jetbrains.annotations.NotNull()
    ai.grafito.app.viewmodel.GrafitoViewModel viewModel, @org.jetbrains.annotations.NotNull()
    java.lang.String cmdInput, @org.jetbrains.annotations.NotNull()
    kotlin.jvm.functions.Function1<? super java.lang.String, kotlin.Unit> onCmdInputChange, boolean showMathKb, @org.jetbrains.annotations.NotNull()
    kotlin.jvm.functions.Function0<kotlin.Unit> onToggleMathKb, @org.jetbrains.annotations.NotNull()
    androidx.compose.ui.Modifier modifier) {
    }
    
    @androidx.compose.runtime.Composable()
    public static final void ObjectItem(@org.jetbrains.annotations.NotNull()
    ai.grafito.app.viewmodel.ObjectUiItem obj, boolean isSelected, @org.jetbrains.annotations.NotNull()
    kotlin.jvm.functions.Function0<kotlin.Unit> onClick, @org.jetbrains.annotations.NotNull()
    kotlin.jvm.functions.Function0<kotlin.Unit> onToggleVisibility, @org.jetbrains.annotations.NotNull()
    kotlin.jvm.functions.Function0<kotlin.Unit> onDelete) {
    }
    
    @androidx.compose.runtime.Composable()
    private static final void VariableSlider(java.lang.String name, double value, double min, double max, kotlin.jvm.functions.Function1<? super java.lang.Double, kotlin.Unit> onValueChange, androidx.compose.ui.Modifier modifier) {
    }
}