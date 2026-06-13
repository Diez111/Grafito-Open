#[cfg(target_os = "android")]
mod android_jni {
    /// Obtiene el puntero ANativeWindow desde un Surface de Android.
    /// Se llama desde `CanvasSurfaceView.getNativeWindowPtr(surface)` (método de instancia).
    #[no_mangle]
    pub extern "system" fn Java_ai_grafito_app_bridge_CanvasSurfaceView_getNativeWindowPtr(
        env: jni::JNIEnv,
        _thiz: jni::objects::JObject,
        surface: jni::objects::JObject,
    ) -> jni::sys::jlong {
        #[link(name = "android")]
        extern "C" {
            fn ANativeWindow_fromSurface(
                env: *mut std::ffi::c_void,
                surface: jni::sys::jobject,
            ) -> *mut std::ffi::c_void;
        }

        unsafe {
            let ptr = ANativeWindow_fromSurface(
                env.get_native_interface() as *mut _,
                surface.into_raw(),
            );
            if ptr.is_null() {
                return 0;
            }
            ptr as jni::sys::jlong
        }
    }
}
