package ai.grafito.app.di;

import ai.grafito.app.bridge.UniFFIBridge;
import dagger.internal.DaggerGenerated;
import dagger.internal.Factory;
import dagger.internal.Preconditions;
import dagger.internal.QualifierMetadata;
import dagger.internal.ScopeMetadata;
import javax.annotation.processing.Generated;

@ScopeMetadata("javax.inject.Singleton")
@QualifierMetadata
@DaggerGenerated
@Generated(
    value = "dagger.internal.codegen.ComponentProcessor",
    comments = "https://dagger.dev"
)
@SuppressWarnings({
    "unchecked",
    "rawtypes",
    "KotlinInternal",
    "KotlinInternalInJava"
})
public final class AppModule_ProvideUniFFIBridgeFactory implements Factory<UniFFIBridge> {
  @Override
  public UniFFIBridge get() {
    return provideUniFFIBridge();
  }

  public static AppModule_ProvideUniFFIBridgeFactory create() {
    return InstanceHolder.INSTANCE;
  }

  public static UniFFIBridge provideUniFFIBridge() {
    return Preconditions.checkNotNullFromProvides(AppModule.INSTANCE.provideUniFFIBridge());
  }

  private static final class InstanceHolder {
    private static final AppModule_ProvideUniFFIBridgeFactory INSTANCE = new AppModule_ProvideUniFFIBridgeFactory();
  }
}
