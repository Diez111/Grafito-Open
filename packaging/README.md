# Grafito - Empaquetado

Este directorio contiene scripts para empaquetar Grafito en diferentes formatos.

## Archivos generados

- `grafito-icon.svg` - Icono vectorial (fuente)
- `grafito-icon-{16,32,48,64,128,256,512}.png` - Iconos rasterizados

## Empaquetado para Linux (.deb)

### Requisitos
- `dpkg-deb` (incluido en Debian/Ubuntu)
- Binario compilado en `target/release/grafito`

### Construir el paquete
```bash
cd packaging
./build-deb.sh
```

El paquete se generará en `packaging/build/grafito_0.9.0-beta.2_amd64.deb`

### Instalar
```bash
sudo dpkg -i build/grafito_0.9.0-beta.2_amd64.deb
```

### Desinstalar
```bash
sudo dpkg -r grafito
```

## Empaquetado para Windows (.exe)

### Requisitos
- `mingw-w64` - Compilador cruzado para Windows
- `rustup` - Gestor de toolchains de Rust

### Instalar mingw-w64
```bash
sudo apt-get install mingw-w64
```

### Construir el ejecutable
```bash
cd packaging
chmod +x build-exe.sh
./build-exe.sh
```

El ejecutable se generará en `target/x86_64-pc-windows-gnu/release/grafito.exe`

### Notas
- El ejecutable de Windows es autónomo (no requiere instalación)
- Incluye todas las dependencias de GTK3 embebidas
- Tamaño aproximado: ~30 MB

## Estructura del paquete .deb

```
/usr/bin/grafito                          - Binario principal
/usr/share/applications/grafito.desktop   - Archivo de launcher
/usr/share/icons/hicolor/*/apps/grafito.png - Iconos en diferentes tamaños
```

## Icono

El icono fue diseñado con estilo GNOME moderno:
- Fondo azul con gradiente
- Curva de función amarilla
- Ejes y puntos destacados
- Bordes redondeados

Archivos fuente: `grafito-icon.svg`

## Versionado

Versión actual: `0.9.0-beta.2`

Para actualizar la versión:
1. Editar `packaging/debian/control` (campo Version)
2. Editar `packaging/build-deb.sh` (variable PKG_VERSION)
3. Reconstruir el paquete

## Licencia

Ver LICENSE en el directorio raíz del proyecto.
