# Grafito - Empaquetado

Este directorio contiene los empaquetadores locales de Grafito para Debian y
Windows. Ambos compilan el workspace bloqueado por `Cargo.lock`.

## Debian (`.deb`)

### Requisitos

- Rust 1.81 o posterior y `build-essential`.
- `dpkg-dev` y `dpkg-deb`.
- `libgmp-dev`, `libmpfr-dev`, `libmpc-dev`, `m4`, `pkg-config` y
  `libdbus-1-dev`.
- Opcionalmente, `desktop-file-utils` y `lintian` para validaciones adicionales.

```bash
sudo apt-get install build-essential dpkg-dev libgmp-dev libmpfr-dev \
  libmpc-dev m4 pkg-config libdbus-1-dev desktop-file-utils lintian
cd packaging
./build-deb.sh
```

El resultado es `packaging/build/grafito_<version>_<arquitectura>.deb`. Las
versiones preliminares usan `~` según el orden de versiones de Debian; por
ejemplo, Cargo `1.2.20-beta` produce Debian `1.2.20~beta`.

`dpkg-shlibdeps` calcula `Depends` desde el ELF terminado, incluidas las
versiones mínimas de bibliotecas realmente enlazadas. Esto hace honesto al
paquete, pero no vuelve antiguo al binario: un `.deb` local conserva el piso de
glibc y demás bibliotecas del host donde se compiló. Para soportar una
distribución antigua hay que compilar y probar en esa distribución o en una
base igual de antigua.

Instalación y desinstalación:

```bash
sudo apt install ./build/grafito_*.deb
sudo apt remove grafito
```

El paquete instala `/usr/bin/grafito`, el lanzador de escritorio, iconos hicolor
y documentación. `postinst` actualiza las cachés después de instalar y
`postrm` lo hace después de retirar los archivos.

## Windows (`.exe` GNU)

### Requisitos

- `rustup`.
- La cadena `mingw-w64`, incluidos `gcc`, `ar`, `windres` y `objdump`.
- Wine es opcional, pero si está instalado el script exige que
  `grafito.exe --help` cargue y termine correctamente bajo Wine.

```bash
sudo apt-get install mingw-w64 wine
cd packaging
./build-exe.sh
```

El resultado principal es
`target/x86_64-pc-windows-gnu/release/grafito.exe`. El script comprueba que el
PE use el subsistema GUI y tenga recursos, recorre sus importaciones y las de
cualquier DLL copiada, y coloca junto al EXE los runtimes MinGW no pertenecientes
al sistema. Una importación no resuelta hace fallar el empaquetado; no se declara
que un EXE sea autónomo sólo porque haya enlazado.

El icono, el manifiesto DPI y la información de versión se incrustan durante la
compilación tanto con GNU como con MSVC. El smoke `--help` valida carga y cierre,
no la creación de una ventana ni el funcionamiento de GPU en Windows real.

## Releases etiquetados

El workflow de release publica los archivos comprimidos, un
`grafito-windows-x64.exe` directo y un `grafito-linux-x64.deb` directo. El `.deb`
de release se compila en Ubuntu 22.04; no se promete compatibilidad con bases
anteriores. El EXE etiquetado se compila nativamente con MSVC en Windows, no con
el ABI GNU del script local; el workflow fuerza el CRT estático, rechaza
importaciones directas de `VCRUNTIME`, `MSVCP` o UCRT dinámico, y comprueba su
subsistema, recursos, metadatos y arranque `--help`. También publica un SBOM SPDX JSON y
`SHA256SUMS.txt`, que cubre los archivos directos, archivos comprimidos y SBOM.

Los artefactos actuales no tienen firma Authenticode, firma Debian ni firma del
manifiesto de checksums. El repositorio privado tampoco emite todavía una
atestación de procedencia firmada; el SBOM y los checksums son inspeccionables,
pero no sustituyen esas firmas.

## Iconos y versión

Los PNG se encuentran en `assets/grafito-icon-{16,32,48,64,128,256,512}.png`
y la fuente vectorial en `assets/grafito-icon.svg`. La versión se lee de
`[workspace.package]` en `Cargo.toml`; no se mantiene una segunda versión en
los scripts.

## Licencia

Ver [LICENSE](../LICENSE). Grafito se distribuye bajo GPL-3.0-or-later.
