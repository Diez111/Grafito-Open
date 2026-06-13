#!/bin/bash
# Script de configuración automática para Android Studio Flatpak
# Este script detecta y configura las variables de entorno necesarias

set -e

GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
NC='\033[0m'

echo -e "${GREEN}=== Configuración de Entorno Android (Flatpak) ===${NC}"

# Detectar Android Studio Flatpak
FLATPAK_SDK="$HOME/.var/app/com.google.AndroidStudio/data/Android/Sdk"

if [ ! -d "$FLATPAK_SDK" ]; then
    echo -e "${RED}Error: No se encontró Android Studio Flatpak${NC}"
    echo "Ruta esperada: $FLATPAK_SDK"
    echo ""
    echo "Si Android Studio está instalado en otra ubicación, configura manualmente:"
    echo "  export ANDROID_HOME=/ruta/al/sdk"
    echo "  export NDK_HOME=/ruta/al/sdk/ndk/<versión>"
    exit 1
fi

echo -e "${GREEN}✓ Android SDK encontrado en: $FLATPAK_SDK${NC}"

# Buscar NDK
NDK_PATH=$(find "$FLATPAK_SDK/ndk" -maxdepth 1 -type d -name "*" | grep -E "ndk/[0-9]" | sort -V | tail -1)

if [ -z "$NDK_PATH" ]; then
    echo -e "${RED}Error: No se encontró NDK${NC}"
    echo "Instala NDK desde Android Studio:"
    echo "  flatpak run com.google.AndroidStudio"
    echo "  Tools → SDK Manager → SDK Tools → NDK (Side by side)"
    exit 1
fi

echo -e "${GREEN}✓ NDK encontrado en: $NDK_PATH${NC}"

# Configurar variables de entorno
export ANDROID_HOME="$FLATPAK_SDK"
export NDK_HOME="$NDK_PATH"
export PATH="$PATH:$ANDROID_HOME/platform-tools"
export PATH="$PATH:$ANDROID_HOME/cmdline-tools/latest/bin"

echo ""
echo -e "${YELLOW}Variables de entorno configuradas:${NC}"
echo "  ANDROID_HOME=$ANDROID_HOME"
echo "  NDK_HOME=$NDK_HOME"
echo ""

# Verificar herramientas
echo -e "${YELLOW}Verificando herramientas...${NC}"

if command -v adb &> /dev/null; then
    echo -e "${GREEN}✓ adb disponible${NC}"
else
    echo -e "${YELLOW}⚠ adb no encontrado (se instalará con platform-tools)${NC}"
fi

if command -v cargo-ndk &> /dev/null; then
    echo -e "${GREEN}✓ cargo-ndk disponible${NC}"
else
    echo -e "${YELLOW}⚠ cargo-ndk no encontrado${NC}"
    echo "Instalar con: cargo install cargo-ndk"
fi

# Verificar targets de Rust
echo ""
echo -e "${YELLOW}Verificando targets de Rust...${NC}"
TARGETS="aarch64-linux-android armv7-linux-androideabi x86_64-linux-android i686-linux-android"
for target in $TARGETS; do
    if rustup target list --installed | grep -q "$target"; then
        echo -e "${GREEN}✓ $target instalado${NC}"
    else
        echo -e "${YELLOW}⚠ $target no instalado${NC}"
        echo "  Instalar con: rustup target add $target"
    fi
done

# Guardar configuración
ENV_FILE="$HOME/.grafito-android-env"
cat > "$ENV_FILE" << EOF
# Variables de entorno para Grafito Android (generado automáticamente)
# Añade esta línea a tu ~/.bashrc o ~/.zshrc:
#   source $ENV_FILE

export ANDROID_HOME="$ANDROID_HOME"
export NDK_HOME="$NDK_HOME"
export PATH="\$PATH:$ANDROID_HOME/platform-tools"
export PATH="\$PATH:$ANDROID_HOME/cmdline-tools/latest/bin"
EOF

echo ""
echo -e "${GREEN}=== Configuración completada ===${NC}"
echo ""
echo -e "${YELLOW}Para usar estas variables permanentemente, añade a tu ~/.bashrc:${NC}"
echo "  source $ENV_FILE"
echo ""
echo -e "${YELLOW}O ejecuta antes de compilar:${NC}"
echo "  source $ENV_FILE"
echo "  ./scripts/build-android.sh"
