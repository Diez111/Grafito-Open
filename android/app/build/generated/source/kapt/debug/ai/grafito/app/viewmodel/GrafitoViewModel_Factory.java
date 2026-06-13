package ai.grafito.app.viewmodel;

import ai.grafito.app.bridge.UniFFIBridge;
import dagger.internal.DaggerGenerated;
import dagger.internal.Factory;
import dagger.internal.QualifierMetadata;
import dagger.internal.ScopeMetadata;
import javax.annotation.processing.Generated;
import javax.inject.Provider;

@ScopeMetadata
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
public final class GrafitoViewModel_Factory implements Factory<GrafitoViewModel> {
  private final Provider<UniFFIBridge> bridgeProvider;

  public GrafitoViewModel_Factory(Provider<UniFFIBridge> bridgeProvider) {
    this.bridgeProvider = bridgeProvider;
  }

  @Override
  public GrafitoViewModel get() {
    return newInstance(bridgeProvider.get());
  }

  public static GrafitoViewModel_Factory create(Provider<UniFFIBridge> bridgeProvider) {
    return new GrafitoViewModel_Factory(bridgeProvider);
  }

  public static GrafitoViewModel newInstance(UniFFIBridge bridge) {
    return new GrafitoViewModel(bridge);
  }
}
