package ai.grafito.app.di

import ai.grafito.app.bridge.UniFFIBridge
import dagger.Module
import dagger.Provides
import dagger.hilt.InstallIn
import dagger.hilt.components.SingletonComponent
import javax.inject.Singleton

@Module
@InstallIn(SingletonComponent::class)
object AppModule {

    @Provides
    @Singleton
    fun provideUniFFIBridge(): UniFFIBridge = UniFFIBridge()
}
