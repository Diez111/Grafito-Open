#!/bin/bash
set -e

echo "Building Grafito .deb package..."

# Variables
PKG_NAME="grafito"
PKG_VERSION="$(grep -E '^version\s*=\s*"' ../Cargo.toml | head -1 | sed -E 's/^version\s*=\s*"([^"]+)"/\1/')"
PKG_ARCH="amd64"
BUILD_DIR="build/${PKG_NAME}_${PKG_VERSION}_${PKG_ARCH}"
DEBIAN_DIR="debian"
ASSETS_DIR="../assets"
TARGET_DIR="../target/release"

# Clean previous build
rm -rf build
mkdir -p "$BUILD_DIR"

# Create directory structure
mkdir -p "$BUILD_DIR/DEBIAN"
mkdir -p "$BUILD_DIR/usr/bin"
mkdir -p "$BUILD_DIR/usr/share/applications"
for size in 16 32 48 64 128 256 512; do
    mkdir -p "$BUILD_DIR/usr/share/icons/hicolor/${size}x${size}/apps"
done

# Copy binary
echo "Copying binary..."
cp "$TARGET_DIR/grafito" "$BUILD_DIR/usr/bin/"
chmod 755 "$BUILD_DIR/usr/bin/grafito"

# Copy license
echo "Copying license..."
mkdir -p "$BUILD_DIR/usr/share/doc/grafito"
cp ../LICENSE "$BUILD_DIR/usr/share/doc/grafito/"
cp "$DEBIAN_DIR/copyright" "$BUILD_DIR/usr/share/doc/grafito/"

# Copy icons
echo "Copying icons..."
for size in 16 32 48 64 128 256 512; do
    cp "$ASSETS_DIR/grafito-icon-${size}x${size}.png" \
       "$BUILD_DIR/usr/share/icons/hicolor/${size}x${size}/apps/grafito.png"
done

# Copy desktop file
echo "Copying desktop file..."
cp "$DEBIAN_DIR/grafito.desktop" "$BUILD_DIR/usr/share/applications/"

# Copy control files
echo "Copying control files..."
cp "$DEBIAN_DIR/control" "$BUILD_DIR/DEBIAN/"
cp "$DEBIAN_DIR/postinst" "$BUILD_DIR/DEBIAN/"
cp "$DEBIAN_DIR/prerm" "$BUILD_DIR/DEBIAN/"

# Set permissions
chmod 755 "$BUILD_DIR/DEBIAN/postinst"
chmod 755 "$BUILD_DIR/DEBIAN/prerm"

# Calculate installed size
INSTALLED_SIZE=$(du -sk "$BUILD_DIR" | cut -f1)
echo "Installed-Size: $INSTALLED_SIZE" >> "$BUILD_DIR/DEBIAN/control"

# Build the package
echo "Building .deb package..."
dpkg-deb --build "$BUILD_DIR" "build/${PKG_NAME}_${PKG_VERSION}_${PKG_ARCH}.deb"

echo ""
echo "Package built successfully!"
echo "Output: build/${PKG_NAME}_${PKG_VERSION}_${PKG_ARCH}.deb"
echo ""
echo "To install:"
echo "  sudo dpkg -i build/${PKG_NAME}_${PKG_VERSION}_${PKG_ARCH}.deb"
echo ""
echo "To uninstall:"
echo "  sudo dpkg -r grafito"
