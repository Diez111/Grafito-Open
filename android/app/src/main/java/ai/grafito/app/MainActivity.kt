package ai.grafito.app

import android.os.Bundle
import android.view.WindowManager
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.material3.windowsizeclass.ExperimentalMaterial3WindowSizeClassApi
import androidx.compose.material3.windowsizeclass.calculateWindowSizeClass
import androidx.compose.material3.windowsizeclass.WindowWidthSizeClass
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.core.view.WindowCompat
import androidx.hilt.navigation.compose.hiltViewModel
import ai.grafito.app.ui.screens.PhoneLayout
import ai.grafito.app.ui.screens.TabletLayout
import ai.grafito.app.ui.theme.GrafitoTheme
import ai.grafito.app.viewmodel.GrafitoViewModel
import dagger.hilt.android.AndroidEntryPoint

@AndroidEntryPoint
class MainActivity : ComponentActivity() {

    @OptIn(ExperimentalMaterial3WindowSizeClassApi::class)
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()

        WindowCompat.setDecorFitsSystemWindows(window, false)
        window.addFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)

        setContent {
            val windowSizeClass = calculateWindowSizeClass(this)
            val viewModel: GrafitoViewModel = hiltViewModel()
            val darkMode by androidx.compose.runtime.derivedStateOf { viewModel.canvasState.darkMode }
            val systemDark = isSystemInDarkTheme()

            LaunchedEffect(darkMode) {
                val controller = WindowCompat.getInsetsController(window, window.decorView)
                controller.isAppearanceLightStatusBars = !darkMode
                controller.isAppearanceLightNavigationBars = !darkMode
                window.statusBarColor = android.graphics.Color.TRANSPARENT
                window.navigationBarColor = android.graphics.Color.TRANSPARENT
            }

            LaunchedEffect(Unit) {
                viewModel.initDarkMode(systemDark)
            }

            GrafitoTheme(darkTheme = darkMode) {
                when (windowSizeClass.widthSizeClass) {
                    WindowWidthSizeClass.Compact -> PhoneLayout(viewModel)
                    WindowWidthSizeClass.Medium,
                    WindowWidthSizeClass.Expanded -> TabletLayout(viewModel)
                    else -> PhoneLayout(viewModel)
                }
            }
        }
    }
}
