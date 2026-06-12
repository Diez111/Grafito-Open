# Grafito Android Build Guide

## Requisitos Previos

### 1. Android SDK y NDK (vía Android Studio Flatpak)

Si usas Android Studio desde Flatpak:

```bash
# Instalar Android Studio desde Flatpak
flatpak install flathub com.google.AndroidStudio

# Configurar variables de entorno
export ANDROID_HOME=$HOME/.var/app/com.google.AndroidStudio/data/Android/Sdk
export NDK_HOME=$ANDROID_HOME/ndk/25.2.9519653
export PATH=$PATH:$ANDROID_HOME/platform-tools
export PATH=$PATH:$ANDROID_HOME/cmdline-tools/latest/bin

# Agregar a ~/.bashrc o ~/.zshrc para persistencia
echo 'export ANDROID_HOME=$HOME/.var/app/com.google.AndroidStudio/data/Android/Sdk' >> ~/.bashrc
echo 'export NDK_HOME=$ANDROID_HOME/ndk/25.2.9519653' >> ~/.bashrc
echo 'export PATH=$PATH:$ANDROID_HOME/platform-tools' >> ~/.bashrc
```

### 2. Configurar Android Studio

1. Abrir Android Studio: `flatpak run com.google.AndroidStudio`
2. Ir a **SDK Manager** (ícono de cubo con flecha)
3. En la pestaña **SDK Platforms**, seleccionar:
   - Android 13.0 (Tiramisu) - API Level 33
4. En la pestaña **SDK Tools**, seleccionar:
   - Android SDK Build-Tools 33.0.2
   - Android SDK Command-line Tools (latest)
   - Android SDK Platform-Tools
   - NDK (Side by side) - versión 25.2.9519653 o superior
   - CMake (opcional, para builds nativos)
5. Click en **Apply** e instalar

### 3. Herramientas de Rust

```bash
# Instalar cargo-ndk
cargo install cargo-ndk

# Agregar targets de Android
rustup target add aarch64-linux-android armv7-linux-androideabi x86_64-linux-android i686-linux-android
```

## Construir la App

### Build completo (todas las arquitecturas)

```bash
cd grafito
./scripts/build-android.sh
```

Este script:
1. Construye las bibliotecas nativas para arm64-v8a, armeabi-v7a, x86_64 y x86
2. Genera el APK en `android/app/build/outputs/apk/debug/app-debug.apk`

### Build para una arquitectura específica (más rápido)

```bash
# Solo ARM64 (la mayoría de dispositivos modernos)
cargo ndk -t arm64-v8a -o android/app/src/main/jniLibs build --release

# Luego construir el APK
cd android
./gradlew assembleDebug
```

### Build de release (optimizado)

```bash
cd android
./gradlew assembleRelease
```

El APK firmado estará en `android/app/build/outputs/apk/release/app-release-unsigned.apk`

## Instalar en Dispositivo

### Método 1: Script automático

```bash
./scripts/install-android.sh
```

### Método 2: Manual con adb

```bash
# Listar dispositivos conectados
adb devices

# Instalar APK
adb install android/app/build/outputs/apk/debug/app-debug.apk

# O reinstalar (mantiene datos)
adb install -r android/app/build/outputs/apk/debug/app-debug.apk
```

### Método 3: Desde Android Studio

1. Abrir el proyecto en Android Studio:
   ```bash
   flatpak run com.google.AndroidStudio /path/to/grafito/android
   ```
2. Conectar dispositivo o iniciar emulador
3. Click en **Run** (ícono de play verde)

## Depuración

### Ver logs en tiempo real

```bash
# Todos los logs
adb logcat

# Solo logs de Grafito
adb logcat -s Grafito

# Logs con timestamp
adb logcat -v time | grep Grafito
```

### Limpiar datos de la app

```bash
adb shell pm clear ai.grafito.app
```

### Desinstalar

```bash
adb uninstall ai.grafito.app
```

## Estructura del Proyecto

```
grafito/
├── android/                          # Proyecto Android
│   ├── app/
│   │   ├── src/main/
│   │   │   ├── java/ai/grafito/app/
│   │   │   │   └── MainActivity.kt  # Actividad principal
│   │   │   ├── jniLibs/             # Bibliotecas nativas (generadas)
│   │   │   │   ├── arm64-v8a/
│   │   │   │   ├── armeabi-v7a/
│   │   │   │   ├── x86_64/
│   │   │   │   └── x86/
│   │   │   ├── res/                 # Recursos Android
│   │   │   └── AndroidManifest.xml
│   │   └── build.gradle
│   ├── build.gradle
│   └── settings.gradle
├── crates/
│   └── grafito-app/
│       └── src/
│           ├── android.rs           # Código específico de Android
│           ├── lib.rs               # Lógica compartida
│           └── main.rs              # Entry point desktop
└── scripts/
    ├── build-android.sh             # Script de build
    └── install-android.sh           # Script de instalación
```

## Arquitecturas Soportadas

- **arm64-v8a**: Dispositivos ARM de 64 bits (la mayoría de dispositivos modernos)
- **armeabi-v7a**: Dispositivos ARM de 32 bits (dispositivos antiguos)
- **x86_64**: Emuladores y tablets x86 de 64 bits
- **x86**: Emuladores x86 de 32 bits

## Solución de Problemas

### Error: "NDK not found"

```bash
# Verificar que NDK_HOME esté configurado
echo $NDK_HOME

# Si está vacío, configurar:
export NDK_HOME=$ANDROID_HOME/ndk/25.2.9519653
```

### Error: "cargo-ndk: command not found"

```bash
cargo install cargo-ndk
```

### Error: "target not found"

```bash
rustup target add aarch64-linux-android armv7-linux-androideabi x86_64-linux-android i686-linux-android
```

### Error: "adb: command not found"

```bash
# Agregar platform-tools al PATH
export PATH=$PATH:$ANDROID_HOME/platform-tools
```

### La app se cierra inmediatamente

```bash
# Ver logs de crash
adb logcat -d | grep -A 20 "FATAL"

# Ver logs de Grafito
adb logcat -s Grafito
```

### Problemas de permisos

Asegúrate de que `AndroidManifest.xml` tenga los permisos necesarios:
- `WRITE_EXTERNAL_STORAGE` (para Android < 10)
- `READ_EXTERNAL_STORAGE` (para Android < 13)

## Desarrollo

### Abrir proyecto Android en Android Studio

```bash
flatpak run com.google.AndroidStudio /path/to/grafito/android
```

### Sincronizar cambios de Rust

Después de modificar código Rust:

```bash
# Reconstruir bibliotecas nativas
./scripts/build-android.sh

# O solo para ARM64 (más rápido)
cargo ndk -t arm64-v8a -o android/app/src/main/jniLibs build --release
cd android && ./gradlew assembleDebug && cd ..

# Reinstalar
./scripts/install-android.sh
```

### Hot reload (no disponible)

Android no soporta hot reload para código nativo. Debes reconstruir e reinstalar después de cada cambio.

## Publicación

### Generar APK firmado

1. Crear keystore:
   ```bash
   keytool -genkey -v -keystore grafito-release-key.jks -keyalg RSA -keysize 2048 -validity 10000 -alias grafito
   ```

2. Configurar en `android/app/build.gradle`:
   ```gradle
   android {
       signingConfigs {
           release {
               storeFile file("../grafito-release-key.jks")
               storePassword "tu_password"
               keyAlias "grafito"
               keyPassword "tu_password"
           }
       }
       buildTypes {
           release {
               signingConfig signingConfigs.release
           }
       }
   }
   ```

3. Construir:
   ```bash
   cd android
   ./gradlew assembleRelease
   ```

### Publicar en Google Play Store

1. Crear cuenta de desarrollador en Google Play Console
2. Crear nueva aplicación
3. Subir el APK firmado o AAB (Android App Bundle)
4. Completar información de la tienda
5. Enviar para revisión

## Recursos Adicionales

- [Android NDK Documentation](https://developer.android.com/ndk)
- [Rust Android Examples](https://github.com/rust-mobile/rust-android-examples)
- [egui Android Support](https://github.com/emilk/egui#egui-on-android)
