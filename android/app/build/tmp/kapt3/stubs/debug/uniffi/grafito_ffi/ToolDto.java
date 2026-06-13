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
 * Herramienta de dibujo activa
 */
@kotlin.Metadata(mv = {1, 9, 0}, k = 1, xi = 48, d1 = {"\u0000\f\n\u0002\u0018\u0002\n\u0002\u0010\u0010\n\u0002\b\u0010\b\u0086\u0081\u0002\u0018\u0000 \u00102\b\u0012\u0004\u0012\u00020\u00000\u0001:\u0001\u0010B\u0007\b\u0002\u00a2\u0006\u0002\u0010\u0002j\u0002\b\u0003j\u0002\b\u0004j\u0002\b\u0005j\u0002\b\u0006j\u0002\b\u0007j\u0002\b\bj\u0002\b\tj\u0002\b\nj\u0002\b\u000bj\u0002\b\fj\u0002\b\rj\u0002\b\u000ej\u0002\b\u000f\u00a8\u0006\u0011"}, d2 = {"Luniffi/grafito_ffi/ToolDto;", "", "(Ljava/lang/String;I)V", "SELECT", "POINT", "LINE", "CIRCLE", "POLYGON", "FUNCTION", "POINT3_D", "SPHERE3_D", "CUBE3_D", "ATTRACTOR", "FRACTAL", "HISTOGRAM", "SCATTER_PLOT", "Companion", "app_debug"})
public enum ToolDto {
    /*public static final*/ SELECT /* = new SELECT() */,
    /*public static final*/ POINT /* = new POINT() */,
    /*public static final*/ LINE /* = new LINE() */,
    /*public static final*/ CIRCLE /* = new CIRCLE() */,
    /*public static final*/ POLYGON /* = new POLYGON() */,
    /*public static final*/ FUNCTION /* = new FUNCTION() */,
    /*public static final*/ POINT3_D /* = new POINT3_D() */,
    /*public static final*/ SPHERE3_D /* = new SPHERE3_D() */,
    /*public static final*/ CUBE3_D /* = new CUBE3_D() */,
    /*public static final*/ ATTRACTOR /* = new ATTRACTOR() */,
    /*public static final*/ FRACTAL /* = new FRACTAL() */,
    /*public static final*/ HISTOGRAM /* = new HISTOGRAM() */,
    /*public static final*/ SCATTER_PLOT /* = new SCATTER_PLOT() */;
    @org.jetbrains.annotations.NotNull()
    public static final uniffi.grafito_ffi.ToolDto.Companion Companion = null;
    
    ToolDto() {
    }
    
    @org.jetbrains.annotations.NotNull()
    public static kotlin.enums.EnumEntries<uniffi.grafito_ffi.ToolDto> getEntries() {
        return null;
    }
    
    @kotlin.Metadata(mv = {1, 9, 0}, k = 1, xi = 48, d1 = {"\u0000\f\n\u0002\u0018\u0002\n\u0002\u0010\u0000\n\u0002\b\u0002\b\u0086\u0003\u0018\u00002\u00020\u0001B\u0007\b\u0002\u00a2\u0006\u0002\u0010\u0002\u00a8\u0006\u0003"}, d2 = {"Luniffi/grafito_ffi/ToolDto$Companion;", "", "()V", "app_debug"})
    public static final class Companion {
        
        private Companion() {
            super();
        }
    }
}