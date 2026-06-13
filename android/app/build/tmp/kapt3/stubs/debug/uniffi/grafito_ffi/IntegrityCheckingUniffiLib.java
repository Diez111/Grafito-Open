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

@kotlin.Metadata(mv = {1, 9, 0}, k = 1, xi = 48, d1 = {"\u0000\u001a\n\u0002\u0018\u0002\n\u0002\u0010\u0000\n\u0002\b\u0002\n\u0002\u0010\b\n\u0000\n\u0002\u0010\n\n\u0002\b(\b\u00c0\u0002\u0018\u00002\u00020\u0001B\u0007\b\u0002\u00a2\u0006\u0002\u0010\u0002J\t\u0010\u0003\u001a\u00020\u0004H\u0086 J\t\u0010\u0005\u001a\u00020\u0006H\u0086 J\t\u0010\u0007\u001a\u00020\u0006H\u0086 J\t\u0010\b\u001a\u00020\u0006H\u0086 J\t\u0010\t\u001a\u00020\u0006H\u0086 J\t\u0010\n\u001a\u00020\u0006H\u0086 J\t\u0010\u000b\u001a\u00020\u0006H\u0086 J\t\u0010\f\u001a\u00020\u0006H\u0086 J\t\u0010\r\u001a\u00020\u0006H\u0086 J\t\u0010\u000e\u001a\u00020\u0006H\u0086 J\t\u0010\u000f\u001a\u00020\u0006H\u0086 J\t\u0010\u0010\u001a\u00020\u0006H\u0086 J\t\u0010\u0011\u001a\u00020\u0006H\u0086 J\t\u0010\u0012\u001a\u00020\u0006H\u0086 J\t\u0010\u0013\u001a\u00020\u0006H\u0086 J\t\u0010\u0014\u001a\u00020\u0006H\u0086 J\t\u0010\u0015\u001a\u00020\u0006H\u0086 J\t\u0010\u0016\u001a\u00020\u0006H\u0086 J\t\u0010\u0017\u001a\u00020\u0006H\u0086 J\t\u0010\u0018\u001a\u00020\u0006H\u0086 J\t\u0010\u0019\u001a\u00020\u0006H\u0086 J\t\u0010\u001a\u001a\u00020\u0006H\u0086 J\t\u0010\u001b\u001a\u00020\u0006H\u0086 J\t\u0010\u001c\u001a\u00020\u0006H\u0086 J\t\u0010\u001d\u001a\u00020\u0006H\u0086 J\t\u0010\u001e\u001a\u00020\u0006H\u0086 J\t\u0010\u001f\u001a\u00020\u0006H\u0086 J\t\u0010 \u001a\u00020\u0006H\u0086 J\t\u0010!\u001a\u00020\u0006H\u0086 J\t\u0010\"\u001a\u00020\u0006H\u0086 J\t\u0010#\u001a\u00020\u0006H\u0086 J\t\u0010$\u001a\u00020\u0006H\u0086 J\t\u0010%\u001a\u00020\u0006H\u0086 J\t\u0010&\u001a\u00020\u0006H\u0086 J\t\u0010\'\u001a\u00020\u0006H\u0086 J\t\u0010(\u001a\u00020\u0006H\u0086 J\t\u0010)\u001a\u00020\u0006H\u0086 J\t\u0010*\u001a\u00020\u0006H\u0086 J\t\u0010+\u001a\u00020\u0006H\u0086 J\t\u0010,\u001a\u00020\u0006H\u0086 J\t\u0010-\u001a\u00020\u0006H\u0086 \u00a8\u0006."}, d2 = {"Luniffi/grafito_ffi/IntegrityCheckingUniffiLib;", "", "()V", "ffi_grafito_ffi_uniffi_contract_version", "", "uniffi_grafito_ffi_checksum_constructor_canvasrenderer_new", "", "uniffi_grafito_ffi_checksum_constructor_grafitoengine_new", "uniffi_grafito_ffi_checksum_method_canvasrenderer_cleanup", "uniffi_grafito_ffi_checksum_method_canvasrenderer_get_height", "uniffi_grafito_ffi_checksum_method_canvasrenderer_get_width", "uniffi_grafito_ffi_checksum_method_canvasrenderer_init_with_surface", "uniffi_grafito_ffi_checksum_method_canvasrenderer_render_frame", "uniffi_grafito_ffi_checksum_method_canvasrenderer_resize", "uniffi_grafito_ffi_checksum_method_canvasrenderer_start_render_loop", "uniffi_grafito_ffi_checksum_method_canvasrenderer_stop_render_loop", "uniffi_grafito_ffi_checksum_method_grafitoengine_camera_dolly", "uniffi_grafito_ffi_checksum_method_grafitoengine_camera_orbit", "uniffi_grafito_ffi_checksum_method_grafitoengine_canvas_pan", "uniffi_grafito_ffi_checksum_method_grafitoengine_canvas_tap", "uniffi_grafito_ffi_checksum_method_grafitoengine_canvas_zoom", "uniffi_grafito_ffi_checksum_method_grafitoengine_clear", "uniffi_grafito_ffi_checksum_method_grafitoengine_create_canvas_renderer", "uniffi_grafito_ffi_checksum_method_grafitoengine_delete_object", "uniffi_grafito_ffi_checksum_method_grafitoengine_get_snapshot", "uniffi_grafito_ffi_checksum_method_grafitoengine_get_spreadsheet", "uniffi_grafito_ffi_checksum_method_grafitoengine_get_tool", "uniffi_grafito_ffi_checksum_method_grafitoengine_is_dark_mode", "uniffi_grafito_ffi_checksum_method_grafitoengine_load_from_file", "uniffi_grafito_ffi_checksum_method_grafitoengine_pick_object_at", "uniffi_grafito_ffi_checksum_method_grafitoengine_process_command", "uniffi_grafito_ffi_checksum_method_grafitoengine_redo", "uniffi_grafito_ffi_checksum_method_grafitoengine_save_to_file", "uniffi_grafito_ffi_checksum_method_grafitoengine_search_commands", "uniffi_grafito_ffi_checksum_method_grafitoengine_select_object", "uniffi_grafito_ffi_checksum_method_grafitoengine_set_cell", "uniffi_grafito_ffi_checksum_method_grafitoengine_set_dark_mode", "uniffi_grafito_ffi_checksum_method_grafitoengine_set_object_color", "uniffi_grafito_ffi_checksum_method_grafitoengine_set_object_label", "uniffi_grafito_ffi_checksum_method_grafitoengine_set_tool", "uniffi_grafito_ffi_checksum_method_grafitoengine_set_variable", "uniffi_grafito_ffi_checksum_method_grafitoengine_set_view_mode", "uniffi_grafito_ffi_checksum_method_grafitoengine_toggle_visibility", "uniffi_grafito_ffi_checksum_method_grafitoengine_undo", "uniffi_grafito_ffi_checksum_method_grafitoengine_update_screen_size", "uniffi_grafito_ffi_checksum_method_grafitoengine_zoom_to_fit", "app_debug"})
public final class IntegrityCheckingUniffiLib {
    @org.jetbrains.annotations.NotNull()
    public static final uniffi.grafito_ffi.IntegrityCheckingUniffiLib INSTANCE = null;
    
    private IntegrityCheckingUniffiLib() {
        super();
    }
    
    public final native short uniffi_grafito_ffi_checksum_method_grafitoengine_camera_dolly() {
        return 0;
    }
    
    public final native short uniffi_grafito_ffi_checksum_method_grafitoengine_camera_orbit() {
        return 0;
    }
    
    public final native short uniffi_grafito_ffi_checksum_method_grafitoengine_canvas_pan() {
        return 0;
    }
    
    public final native short uniffi_grafito_ffi_checksum_method_grafitoengine_canvas_tap() {
        return 0;
    }
    
    public final native short uniffi_grafito_ffi_checksum_method_grafitoengine_canvas_zoom() {
        return 0;
    }
    
    public final native short uniffi_grafito_ffi_checksum_method_grafitoengine_clear() {
        return 0;
    }
    
    public final native short uniffi_grafito_ffi_checksum_method_grafitoengine_create_canvas_renderer() {
        return 0;
    }
    
    public final native short uniffi_grafito_ffi_checksum_method_grafitoengine_delete_object() {
        return 0;
    }
    
    public final native short uniffi_grafito_ffi_checksum_method_grafitoengine_get_snapshot() {
        return 0;
    }
    
    public final native short uniffi_grafito_ffi_checksum_method_grafitoengine_get_spreadsheet() {
        return 0;
    }
    
    public final native short uniffi_grafito_ffi_checksum_method_grafitoengine_get_tool() {
        return 0;
    }
    
    public final native short uniffi_grafito_ffi_checksum_method_grafitoengine_is_dark_mode() {
        return 0;
    }
    
    public final native short uniffi_grafito_ffi_checksum_method_grafitoengine_load_from_file() {
        return 0;
    }
    
    public final native short uniffi_grafito_ffi_checksum_method_grafitoengine_pick_object_at() {
        return 0;
    }
    
    public final native short uniffi_grafito_ffi_checksum_method_grafitoengine_process_command() {
        return 0;
    }
    
    public final native short uniffi_grafito_ffi_checksum_method_grafitoengine_redo() {
        return 0;
    }
    
    public final native short uniffi_grafito_ffi_checksum_method_grafitoengine_save_to_file() {
        return 0;
    }
    
    public final native short uniffi_grafito_ffi_checksum_method_grafitoengine_search_commands() {
        return 0;
    }
    
    public final native short uniffi_grafito_ffi_checksum_method_grafitoengine_select_object() {
        return 0;
    }
    
    public final native short uniffi_grafito_ffi_checksum_method_grafitoengine_set_cell() {
        return 0;
    }
    
    public final native short uniffi_grafito_ffi_checksum_method_grafitoengine_set_dark_mode() {
        return 0;
    }
    
    public final native short uniffi_grafito_ffi_checksum_method_grafitoengine_set_object_color() {
        return 0;
    }
    
    public final native short uniffi_grafito_ffi_checksum_method_grafitoengine_set_object_label() {
        return 0;
    }
    
    public final native short uniffi_grafito_ffi_checksum_method_grafitoengine_set_tool() {
        return 0;
    }
    
    public final native short uniffi_grafito_ffi_checksum_method_grafitoengine_set_variable() {
        return 0;
    }
    
    public final native short uniffi_grafito_ffi_checksum_method_grafitoengine_set_view_mode() {
        return 0;
    }
    
    public final native short uniffi_grafito_ffi_checksum_method_grafitoengine_toggle_visibility() {
        return 0;
    }
    
    public final native short uniffi_grafito_ffi_checksum_method_grafitoengine_undo() {
        return 0;
    }
    
    public final native short uniffi_grafito_ffi_checksum_method_grafitoengine_update_screen_size() {
        return 0;
    }
    
    public final native short uniffi_grafito_ffi_checksum_method_grafitoengine_zoom_to_fit() {
        return 0;
    }
    
    public final native short uniffi_grafito_ffi_checksum_method_canvasrenderer_cleanup() {
        return 0;
    }
    
    public final native short uniffi_grafito_ffi_checksum_method_canvasrenderer_get_height() {
        return 0;
    }
    
    public final native short uniffi_grafito_ffi_checksum_method_canvasrenderer_get_width() {
        return 0;
    }
    
    public final native short uniffi_grafito_ffi_checksum_method_canvasrenderer_init_with_surface() {
        return 0;
    }
    
    public final native short uniffi_grafito_ffi_checksum_method_canvasrenderer_render_frame() {
        return 0;
    }
    
    public final native short uniffi_grafito_ffi_checksum_method_canvasrenderer_resize() {
        return 0;
    }
    
    public final native short uniffi_grafito_ffi_checksum_method_canvasrenderer_start_render_loop() {
        return 0;
    }
    
    public final native short uniffi_grafito_ffi_checksum_method_canvasrenderer_stop_render_loop() {
        return 0;
    }
    
    public final native short uniffi_grafito_ffi_checksum_constructor_grafitoengine_new() {
        return 0;
    }
    
    public final native short uniffi_grafito_ffi_checksum_constructor_canvasrenderer_new() {
        return 0;
    }
    
    public final native int ffi_grafito_ffi_uniffi_contract_version() {
        return 0;
    }
}