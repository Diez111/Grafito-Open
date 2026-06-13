#!/bin/bash
set -e

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

echo -e "${GREEN}=== Grafito Android Install Script ===${NC}"

# Verificar que adb esté disponible
if ! command -v adb &> /dev/null; then
    echo -e "${RED}Error: adb no está en el PATH${NC}"
    echo "Asegúrate de tener Android SDK Platform Tools instalado"
    echo "Para Flatpak de Android Studio:"
    echo "  export PATH=\$PATH:\$HOME/.var/app/com.google.AndroidStudio/data/Android/Sdk/platform-tools"
    exit 1
fi

# Verificar dispositivos conectados
echo -e "${YELLOW}Buscando dispositivos...${NC}"
DEVICES=$(adb devices | grep -w "device" | wc -l)

if [ "$DEVICES" -eq 0 ]; then
    echo -e "${RED}No se encontraron dispositivos conectados${NC}"
    echo "Asegúrate de:"
    echo "  1. Tener USB debugging habilitado en tu dispositivo"
    echo "  2. Conectar el dispositivo por USB"
    echo "  3. Aceptar el mensaje de autorización en el dispositivo"
    exit 1
fi

echo -e "${GREEN}Dispositivos encontrados: $DEVICES${NC}"
adb devices | grep -w "device"

# Verificar que el APK exista
APK_PATH="android/app/build/outputs/apk/debug/app-debug.apk"
if [ ! -f "$APK_PATH" ]; then
    echo -e "${RED}Error: APK no encontrado en $APK_PATH${NC}"
    echo "Ejecuta primero: ./scripts/build-android.sh"
    exit 1
fi

# Instalar APK
echo -e "${YELLOW}Instalando APK...${NC}"
adb install -r "$APK_PATH"

if [ $? -eq 0 ]; then
    echo -e "${GREEN}✓ APK instalado exitosamente${NC}"
    echo -e "${GREEN}Puedes abrir Grafito desde el launcher de tu dispositivo${NC}"
else
    echo -e "${RED}✗ Error instalando APK${NC}"
    exit 1
fi
