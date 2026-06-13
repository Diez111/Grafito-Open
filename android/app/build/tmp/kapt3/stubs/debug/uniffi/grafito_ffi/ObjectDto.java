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
 * Representación plana de un GeoObject para FFI
 */
@kotlin.Metadata(mv = {1, 9, 0}, k = 1, xi = 48, d1 = {"\u00002\n\u0002\u0018\u0002\n\u0002\u0010\u0000\n\u0000\n\u0002\u0010\u000e\n\u0002\b\u0003\n\u0002\u0010\u000b\n\u0000\n\u0002\u0018\u0002\n\u0000\n\u0002\u0010 \n\u0002\u0018\u0002\n\u0002\b#\n\u0002\u0010\b\n\u0002\b\u0003\b\u0086\b\u0018\u0000 22\u00020\u0001:\u00012BC\u0012\u0006\u0010\u0002\u001a\u00020\u0003\u0012\u0006\u0010\u0004\u001a\u00020\u0003\u0012\u0006\u0010\u0005\u001a\u00020\u0003\u0012\u0006\u0010\u0006\u001a\u00020\u0007\u0012\u0006\u0010\b\u001a\u00020\t\u0012\f\u0010\n\u001a\b\u0012\u0004\u0012\u00020\f0\u000b\u0012\u0006\u0010\r\u001a\u00020\u0003\u00a2\u0006\u0002\u0010\u000eJ\t\u0010%\u001a\u00020\u0003H\u00c6\u0003J\t\u0010&\u001a\u00020\u0003H\u00c6\u0003J\t\u0010\'\u001a\u00020\u0003H\u00c6\u0003J\t\u0010(\u001a\u00020\u0007H\u00c6\u0003J\t\u0010)\u001a\u00020\tH\u00c6\u0003J\u000f\u0010*\u001a\b\u0012\u0004\u0012\u00020\f0\u000bH\u00c6\u0003J\t\u0010+\u001a\u00020\u0003H\u00c6\u0003JU\u0010,\u001a\u00020\u00002\b\b\u0002\u0010\u0002\u001a\u00020\u00032\b\b\u0002\u0010\u0004\u001a\u00020\u00032\b\b\u0002\u0010\u0005\u001a\u00020\u00032\b\b\u0002\u0010\u0006\u001a\u00020\u00072\b\b\u0002\u0010\b\u001a\u00020\t2\u000e\b\u0002\u0010\n\u001a\b\u0012\u0004\u0012\u00020\f0\u000b2\b\b\u0002\u0010\r\u001a\u00020\u0003H\u00c6\u0001J\u0013\u0010-\u001a\u00020\u00072\b\u0010.\u001a\u0004\u0018\u00010\u0001H\u00d6\u0003J\t\u0010/\u001a\u000200H\u00d6\u0001J\t\u00101\u001a\u00020\u0003H\u00d6\u0001R\u001a\u0010\b\u001a\u00020\tX\u0086\u000e\u00a2\u0006\u000e\n\u0000\u001a\u0004\b\u000f\u0010\u0010\"\u0004\b\u0011\u0010\u0012R\u001a\u0010\u0002\u001a\u00020\u0003X\u0086\u000e\u00a2\u0006\u000e\n\u0000\u001a\u0004\b\u0013\u0010\u0014\"\u0004\b\u0015\u0010\u0016R\u001a\u0010\u0004\u001a\u00020\u0003X\u0086\u000e\u00a2\u0006\u000e\n\u0000\u001a\u0004\b\u0017\u0010\u0014\"\u0004\b\u0018\u0010\u0016R\u001a\u0010\u0005\u001a\u00020\u0003X\u0086\u000e\u00a2\u0006\u000e\n\u0000\u001a\u0004\b\u0019\u0010\u0014\"\u0004\b\u001a\u0010\u0016R \u0010\n\u001a\b\u0012\u0004\u0012\u00020\f0\u000bX\u0086\u000e\u00a2\u0006\u000e\n\u0000\u001a\u0004\b\u001b\u0010\u001c\"\u0004\b\u001d\u0010\u001eR\u001a\u0010\r\u001a\u00020\u0003X\u0086\u000e\u00a2\u0006\u000e\n\u0000\u001a\u0004\b\u001f\u0010\u0014\"\u0004\b \u0010\u0016R\u001a\u0010\u0006\u001a\u00020\u0007X\u0086\u000e\u00a2\u0006\u000e\n\u0000\u001a\u0004\b!\u0010\"\"\u0004\b#\u0010$\u00a8\u00063"}, d2 = {"Luniffi/grafito_ffi/ObjectDto;", "", "id", "", "label", "objectType", "visible", "", "color", "Luniffi/grafito_ffi/ColorDto;", "properties", "", "Luniffi/grafito_ffi/PropertyDto;", "summary", "(Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;ZLuniffi/grafito_ffi/ColorDto;Ljava/util/List;Ljava/lang/String;)V", "getColor", "()Luniffi/grafito_ffi/ColorDto;", "setColor", "(Luniffi/grafito_ffi/ColorDto;)V", "getId", "()Ljava/lang/String;", "setId", "(Ljava/lang/String;)V", "getLabel", "setLabel", "getObjectType", "setObjectType", "getProperties", "()Ljava/util/List;", "setProperties", "(Ljava/util/List;)V", "getSummary", "setSummary", "getVisible", "()Z", "setVisible", "(Z)V", "component1", "component2", "component3", "component4", "component5", "component6", "component7", "copy", "equals", "other", "hashCode", "", "toString", "Companion", "app_debug"})
public final class ObjectDto {
    @org.jetbrains.annotations.NotNull()
    private java.lang.String id;
    @org.jetbrains.annotations.NotNull()
    private java.lang.String label;
    @org.jetbrains.annotations.NotNull()
    private java.lang.String objectType;
    private boolean visible;
    @org.jetbrains.annotations.NotNull()
    private uniffi.grafito_ffi.ColorDto color;
    @org.jetbrains.annotations.NotNull()
    private java.util.List<uniffi.grafito_ffi.PropertyDto> properties;
    @org.jetbrains.annotations.NotNull()
    private java.lang.String summary;
    @org.jetbrains.annotations.NotNull()
    public static final uniffi.grafito_ffi.ObjectDto.Companion Companion = null;
    
    public ObjectDto(@org.jetbrains.annotations.NotNull()
    java.lang.String id, @org.jetbrains.annotations.NotNull()
    java.lang.String label, @org.jetbrains.annotations.NotNull()
    java.lang.String objectType, boolean visible, @org.jetbrains.annotations.NotNull()
    uniffi.grafito_ffi.ColorDto color, @org.jetbrains.annotations.NotNull()
    java.util.List<uniffi.grafito_ffi.PropertyDto> properties, @org.jetbrains.annotations.NotNull()
    java.lang.String summary) {
        super();
    }
    
    @org.jetbrains.annotations.NotNull()
    public final java.lang.String getId() {
        return null;
    }
    
    public final void setId(@org.jetbrains.annotations.NotNull()
    java.lang.String p0) {
    }
    
    @org.jetbrains.annotations.NotNull()
    public final java.lang.String getLabel() {
        return null;
    }
    
    public final void setLabel(@org.jetbrains.annotations.NotNull()
    java.lang.String p0) {
    }
    
    @org.jetbrains.annotations.NotNull()
    public final java.lang.String getObjectType() {
        return null;
    }
    
    public final void setObjectType(@org.jetbrains.annotations.NotNull()
    java.lang.String p0) {
    }
    
    public final boolean getVisible() {
        return false;
    }
    
    public final void setVisible(boolean p0) {
    }
    
    @org.jetbrains.annotations.NotNull()
    public final uniffi.grafito_ffi.ColorDto getColor() {
        return null;
    }
    
    public final void setColor(@org.jetbrains.annotations.NotNull()
    uniffi.grafito_ffi.ColorDto p0) {
    }
    
    @org.jetbrains.annotations.NotNull()
    public final java.util.List<uniffi.grafito_ffi.PropertyDto> getProperties() {
        return null;
    }
    
    public final void setProperties(@org.jetbrains.annotations.NotNull()
    java.util.List<uniffi.grafito_ffi.PropertyDto> p0) {
    }
    
    @org.jetbrains.annotations.NotNull()
    public final java.lang.String getSummary() {
        return null;
    }
    
    public final void setSummary(@org.jetbrains.annotations.NotNull()
    java.lang.String p0) {
    }
    
    @org.jetbrains.annotations.NotNull()
    public final java.lang.String component1() {
        return null;
    }
    
    @org.jetbrains.annotations.NotNull()
    public final java.lang.String component2() {
        return null;
    }
    
    @org.jetbrains.annotations.NotNull()
    public final java.lang.String component3() {
        return null;
    }
    
    public final boolean component4() {
        return false;
    }
    
    @org.jetbrains.annotations.NotNull()
    public final uniffi.grafito_ffi.ColorDto component5() {
        return null;
    }
    
    @org.jetbrains.annotations.NotNull()
    public final java.util.List<uniffi.grafito_ffi.PropertyDto> component6() {
        return null;
    }
    
    @org.jetbrains.annotations.NotNull()
    public final java.lang.String component7() {
        return null;
    }
    
    @org.jetbrains.annotations.NotNull()
    public final uniffi.grafito_ffi.ObjectDto copy(@org.jetbrains.annotations.NotNull()
    java.lang.String id, @org.jetbrains.annotations.NotNull()
    java.lang.String label, @org.jetbrains.annotations.NotNull()
    java.lang.String objectType, boolean visible, @org.jetbrains.annotations.NotNull()
    uniffi.grafito_ffi.ColorDto color, @org.jetbrains.annotations.NotNull()
    java.util.List<uniffi.grafito_ffi.PropertyDto> properties, @org.jetbrains.annotations.NotNull()
    java.lang.String summary) {
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
    
    @kotlin.Metadata(mv = {1, 9, 0}, k = 1, xi = 48, d1 = {"\u0000\f\n\u0002\u0018\u0002\n\u0002\u0010\u0000\n\u0002\b\u0002\b\u0086\u0003\u0018\u00002\u00020\u0001B\u0007\b\u0002\u00a2\u0006\u0002\u0010\u0002\u00a8\u0006\u0003"}, d2 = {"Luniffi/grafito_ffi/ObjectDto$Companion;", "", "()V", "app_debug"})
    public static final class Companion {
        
        private Companion() {
            super();
        }
    }
}