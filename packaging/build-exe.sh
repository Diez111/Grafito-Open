#!/bin/bash
set -e

echo "Building Grafito for Windows (.exe)..."
echo ""

# Check if mingw-w64 is installed
if ! command -v x86_64-w64-mingw32-gcc &> /dev/null; then
    echo "ERROR: mingw-w64 is not installed."
    echo ""
    echo "Please install it first:"
    echo "  sudo apt-get install mingw-w64"
    echo ""
    echo "Then run this script again."
    exit 1
fi

# Add Windows target if not already added
echo "Adding Windows target..."
rustup target add x86_64-pc-windows-gnu 2>/dev/null || true

# Create cargo config for Windows cross-compilation
echo "Configuring cargo for Windows cross-compilation..."
mkdir -p ../.cargo
cat > ../.cargo/config.toml << 'EOF'
[target.x86_64-pc-windows-gnu]
linker = "x86_64-w64-mingw32-gcc"
ar = "x86_64-w64-mingw32-ar"
EOF

# Build for Windows
echo "Building Windows executable..."
cd ..
cargo build --release --target x86_64-pc-windows-gnu

# Check if build succeeded
if [ -f "target/x86_64-pc-windows-gnu/release/grafito.exe" ]; then
    echo ""
    echo "Windows executable built successfully!"
    echo "Output: target/x86_64-pc-windows-gnu/release/grafito.exe"
    echo ""
    echo "File size:"
    ls -lh target/x86_64-pc-windows-gnu/release/grafito.exe
else
    echo ""
    echo "ERROR: Build failed. Check the output above for errors."
    exit 1
fi
