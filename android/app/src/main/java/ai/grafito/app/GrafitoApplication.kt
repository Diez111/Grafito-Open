package ai.grafito.app

import android.app.Application
import dagger.hilt.android.HiltAndroidApp

@HiltAndroidApp
class GrafitoApplication : Application() {
    companion object {
        init {
            System.loadLibrary("grafito_ffi")
        }
    }
}
