#!/usr/bin/env bash
set -euo pipefail

umask 022

debian_version_from_cargo() {
    local cargo_version="$1"
    # Debian's tilde sorts before the corresponding final upstream version.
    printf '%s\n' "${cargo_version/-/\~}"
}

if [[ "${1:-}" == "--print-debian-version" ]]; then
    [[ $# -eq 2 ]] || {
        echo "usage: $0 --print-debian-version <cargo-version>" >&2
        exit 2
    }
    debian_version_from_cargo "$2"
    exit 0
fi

SCRIPT_DIR="$(CDPATH='' cd -- "$(dirname -- "$0")" && pwd)"
ROOT_DIR="$(CDPATH='' cd -- "$SCRIPT_DIR/.." && pwd)"
cd "$SCRIPT_DIR"

echo "Building Grafito .deb package..."

command -v dpkg-deb >/dev/null || { echo "ERROR: dpkg-deb is required." >&2; exit 1; }
command -v dpkg >/dev/null || { echo "ERROR: dpkg is required." >&2; exit 1; }
command -v dpkg-shlibdeps >/dev/null || { echo "ERROR: dpkg-shlibdeps from dpkg-dev is required." >&2; exit 1; }

PKG_ARCH="$(dpkg --print-architecture)"
case "$PKG_ARCH" in
    amd64|arm64) ;;
    *)
        echo "ERROR: unsupported Debian architecture: $PKG_ARCH" >&2
        exit 1
        ;;
esac

# Compilar el binario actualizado (grafito-app es el crate con el [[bin]] grafito)
echo "Compiling grafito-app..."
cargo build --manifest-path "$ROOT_DIR/Cargo.toml" --release -p grafito-app --locked

# Variables
PKG_NAME="grafito"
# Robust version extraction: prefer cargo metadata (locked, exact), fallback to grep
if command -v jq >/dev/null 2>&1; then
    CARGO_VERSION="$(cargo metadata --locked --format-version 1 --no-deps 2>/dev/null | jq -r '.packages[] | select(.name=="grafito-app") | .version' | head -1)"
elif command -v python3 >/dev/null 2>&1; then
    CARGO_VERSION="$(python3 -c "import json, subprocess; data=json.loads(subprocess.check_output(['cargo','metadata','--locked','--format-version','1','--no-deps'], text=True)); print(next((p['version'] for p in data['packages'] if p['name']=='grafito-app'), ''))" 2>/dev/null || true)"
else
    CARGO_VERSION=""
fi
if [[ -z "${CARGO_VERSION:-}" ]]; then
    CARGO_VERSION="$(grep -E '^version\s*=\s*"' "$ROOT_DIR/Cargo.toml" | head -1 | sed -E 's/^version\s*=\s*"([^"]+)"/\1/')"
fi
[[ -n "$CARGO_VERSION" ]] || { echo "ERROR: could not read the workspace version." >&2; exit 1; }
PKG_VERSION="$(debian_version_from_cargo "$CARGO_VERSION")"
dpkg --validate-version "$PKG_VERSION"
BUILD_DIR="build/${PKG_NAME}_${PKG_VERSION}_${PKG_ARCH}"
PACKAGE_PATH="build/${PKG_NAME}_${PKG_VERSION}_${PKG_ARCH}.deb"
DEBIAN_DIR="debian"
ASSETS_DIR="$ROOT_DIR/assets"
TARGET_DIR="$ROOT_DIR/target/release"

# Clean only the package being rebuilt; retain other local package artifacts.
rm -rf -- "$BUILD_DIR"
rm -f -- "$PACKAGE_PATH"

# Create directory structure
install -d -m 0755 "$BUILD_DIR/DEBIAN"
install -d -m 0755 "$BUILD_DIR/usr/bin"
install -d -m 0755 "$BUILD_DIR/usr/share/applications"
install -d -m 0755 "$BUILD_DIR/usr/share/doc/grafito"
for size in 16 32 48 64 128 256 512; do
    install -d -m 0755 "$BUILD_DIR/usr/share/icons/hicolor/${size}x${size}/apps"
done
# Scalable icon keeps the launcher crisp on HiDPI across desktop environments.
install -d -m 0755 "$BUILD_DIR/usr/share/icons/hicolor/scalable/apps"

# Copy binary
echo "Copying binary..."
install -m 0755 "$TARGET_DIR/grafito" "$BUILD_DIR/usr/bin/grafito"

# Copy license
echo "Copying license..."
install -m 0644 "$ROOT_DIR/LICENSE" "$BUILD_DIR/usr/share/doc/grafito/LICENSE"
install -m 0644 "$DEBIAN_DIR/copyright" "$BUILD_DIR/usr/share/doc/grafito/copyright"
gzip -9n -c "$ROOT_DIR/CHANGELOG.md" > "$BUILD_DIR/usr/share/doc/grafito/changelog.gz"
chmod 0644 "$BUILD_DIR/usr/share/doc/grafito/changelog.gz"

# Copy icons
# The Grafito logo is source of truth for the desktop icon; a missing raster
# must abort the build instead of shipping an empty launcher icon.
for icon_size in 16 32 48 64 128 256 512; do
    [[ -f "$ASSETS_DIR/grafito-icon-${icon_size}x${icon_size}.png" ]] || {
        echo "ERROR: missing icon asset grafito-icon-${icon_size}x${icon_size}.png" >&2
        exit 1
    }
done
[[ -f "$ASSETS_DIR/grafito-icon.svg" ]] || {
    echo "ERROR: missing scalable icon asset grafito-icon.svg" >&2
    exit 1
}
echo "Copying icons..."
for size in 16 32 48 64 128 256 512; do
    install -m 0644 "$ASSETS_DIR/grafito-icon-${size}x${size}.png" \
        "$BUILD_DIR/usr/share/icons/hicolor/${size}x${size}/apps/grafito.png"
done
install -m 0644 "$ASSETS_DIR/grafito-icon.svg" \
    "$BUILD_DIR/usr/share/icons/hicolor/scalable/apps/grafito.svg"

# Copy desktop file
echo "Copying desktop file..."
install -m 0644 "$DEBIAN_DIR/grafito.desktop" "$BUILD_DIR/usr/share/applications/grafito.desktop"

# Install default assistant plugins (e.g. j-space) for the launcher
if [[ -d "$ROOT_DIR/plugins" ]]; then
    echo "Copying default plugins..."
    install -d -m 0755 "$BUILD_DIR/usr/share/grafito/plugins"
    cp -a "$ROOT_DIR/plugins/." "$BUILD_DIR/usr/share/grafito/plugins/"
    find "$BUILD_DIR/usr/share/grafito/plugins" -type d -exec chmod 0755 {} +
    find "$BUILD_DIR/usr/share/grafito/plugins" -type f -exec chmod 0644 {} +
fi

# Copy control files
echo "Copying control files..."
SHLIBS_SUBSTVAR="$(dpkg-shlibdeps -O "$BUILD_DIR/usr/bin/grafito")"
case "$SHLIBS_SUBSTVAR" in
    shlibs:Depends=*) RUNTIME_DEPENDS="${SHLIBS_SUBSTVAR#shlibs:Depends=}" ;;
    *) echo "ERROR: dpkg-shlibdeps did not produce shlibs:Depends." >&2; exit 1 ;;
esac
[[ -n "$RUNTIME_DEPENDS" ]] || { echo "ERROR: generated runtime dependency list is empty." >&2; exit 1; }
awk -v version="$PKG_VERSION" -v architecture="$PKG_ARCH" -v depends="$RUNTIME_DEPENDS" '
    /^Package: grafito$/ {
        package = 1
        print
        print "Version: " version
        next
    }
    package && /^Architecture:/ { print "Architecture: " architecture; next }
    package && /^Depends:/ { print "Depends: " depends; next }
    package { print }
' "$DEBIAN_DIR/control" > "$BUILD_DIR/DEBIAN/control"
chmod 0644 "$BUILD_DIR/DEBIAN/control"
grep -Fx "Architecture: ${PKG_ARCH}" "$BUILD_DIR/DEBIAN/control" >/dev/null
grep -Fx "Depends: ${RUNTIME_DEPENDS}" "$BUILD_DIR/DEBIAN/control" >/dev/null
install -m 0755 "$DEBIAN_DIR/postinst" "$BUILD_DIR/DEBIAN/postinst"
install -m 0755 "$DEBIAN_DIR/prerm" "$BUILD_DIR/DEBIAN/prerm"
install -m 0755 "$DEBIAN_DIR/postrm" "$BUILD_DIR/DEBIAN/postrm"

# Calculate installed size
INSTALLED_SIZE=$(du -sk "$BUILD_DIR/usr" | cut -f1)
echo "Installed-Size: $INSTALLED_SIZE" >> "$BUILD_DIR/DEBIAN/control"

if [[ -n "${SOURCE_DATE_EPOCH:-}" ]]; then
    [[ "$SOURCE_DATE_EPOCH" =~ ^[0-9]+$ ]] || {
        echo "ERROR: SOURCE_DATE_EPOCH must be an integer Unix timestamp." >&2
        exit 1
    }
    find "$BUILD_DIR" -exec touch -h -d "@${SOURCE_DATE_EPOCH}" {} +
fi

# Build the package
echo "Building .deb package..."
dpkg-deb --root-owner-group --build "$BUILD_DIR" "$PACKAGE_PATH"
dpkg-deb --info "$PACKAGE_PATH" >/dev/null
[[ "$(dpkg-deb -f "$PACKAGE_PATH" Depends)" == "$RUNTIME_DEPENDS" ]]

if command -v desktop-file-validate >/dev/null; then
    desktop-file-validate "$DEBIAN_DIR/grafito.desktop"
fi

if command -v lintian >/dev/null; then
    lintian --fail-on error "$PACKAGE_PATH"
fi

echo ""
echo "Package built successfully!"
echo "Output: $PACKAGE_PATH"
echo ""
echo "To install:"
echo "  sudo apt install ./$PACKAGE_PATH"
echo ""
echo "To uninstall:"
echo "  sudo apt remove grafito"
