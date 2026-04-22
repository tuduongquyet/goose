#!/bin/bash

# Build script for Goose CLI - mirrors GitHub Actions workflow
# Usage: ./build-cli.sh [--version VERSION] [--target TARGET] [--no-docker] [--help]

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Defaults
VERSION=""
TARGET=""
HELP=false
NO_DOCKER=false

# Parse arguments
while [[ $# -gt 0 ]]; do
  case $1 in
    --version)
      VERSION="$2"
      shift 2
      ;;
    --target)
      TARGET="$2"
      shift 2
      ;;
    --no-docker)
      NO_DOCKER=true
      shift
      ;;
    --help)
      HELP=true
      shift
      ;;
    *)
      echo -e "${RED}Unknown option: $1${NC}"
      HELP=true
      shift
      ;;
  esac
done

# Show help
if [ "$HELP" = true ]; then
  cat << 'EOF'
Goose CLI Build Script

Usage: ./build-cli.sh [OPTIONS]

Options:
  --version VERSION      Update version in Cargo.toml before building
  --target TARGET        Build for specific target (e.g., x86_64-unknown-linux-gnu)
  --no-docker            Build locally instead of using cross (Docker) - requires gcc-10
  --help                 Show this help message

Examples:
  # Build for current platform using cross (Docker)
  ./build-cli.sh

  # Build with version update
  ./build-cli.sh --version 0.2.0

  # Build for specific target (uses cross with Docker)
  ./build-cli.sh --target aarch64-unknown-linux-gnu

  # Build locally without Docker (requires gcc-10 installed)
  ./build-cli.sh --no-docker

Note: This script uses 'cross' with Docker for Linux/macOS targets to ensure
compatibility with aws-lc-sys. Docker must be running. If you don't have Docker,
use --no-docker and ensure gcc-10 is installed (apt-get install gcc-10).
EOF
  exit 0
fi

echo -e "${GREEN}=== Goose CLI Build Script ===${NC}"

# Update version if provided
if [ -n "$VERSION" ]; then
  echo -e "${YELLOW}Updating version to $VERSION in Cargo.toml...${NC}"
  sed -i.bak "s/^version = \".*/version = \"$VERSION\"/" Cargo.toml
  rm -f Cargo.toml.bak
fi

# Detect OS if not specified
detect_os() {
  case "$OSTYPE" in
    linux*)   echo "linux" ;;
    darwin*)  echo "macos" ;;
    msys|mingw|cygwin) echo "windows" ;;
    *)        echo "unknown" ;;
  esac
}

DETECTED_OS=$(detect_os)

# Determine build strategy
if [ -z "$TARGET" ]; then
  case "$DETECTED_OS" in
    linux)
      ARCH=$(uname -m)
      case "$ARCH" in
        x86_64)  TARGET="x86_64-unknown-linux-gnu" ;;
        aarch64) TARGET="aarch64-unknown-linux-gnu" ;;
        *)       echo -e "${RED}Unsupported Linux architecture: $ARCH${NC}"; exit 1 ;;
      esac
      USE_CROSS=true
      ;;
    macos)
      ARCH=$(uname -m)
      case "$ARCH" in
        x86_64)  TARGET="x86_64-apple-darwin" ;;
        arm64)   TARGET="aarch64-apple-darwin" ;;
        *)       echo -e "${RED}Unsupported macOS architecture: $ARCH${NC}"; exit 1 ;;
      esac
      USE_CROSS=true
      ;;
    windows)
      TARGET="x86_64-pc-windows-msvc"
      USE_CROSS=false
      ;;
    *)
      echo -e "${RED}Unsupported OS: $DETECTED_OS${NC}"
      exit 1
      ;;
  esac
else
  # If target specified, assume cross compilation needed for Linux/macOS targets
  if [[ "$TARGET" == *"linux"* ]] || [[ "$TARGET" == *"darwin"* ]]; then
    USE_CROSS=true
  else
    USE_CROSS=false
  fi
fi

echo -e "${GREEN}Building for target: $TARGET${NC}"
echo -e "${YELLOW}Detected OS: $DETECTED_OS${NC}"

# Add target
echo -e "${YELLOW}Adding Rust target: $TARGET${NC}"
rustup target add "$TARGET"

# Show toolchain info
echo -e "${YELLOW}Rust toolchain info:${NC}"
rustup show

# Build with aws-lc-sys workarounds via Docker
echo -e "${GREEN}Building CLI for target: $TARGET${NC}"

# Set environment variables for aws-lc-sys compatibility
export RUST_BACKTRACE=1

if [ "$NO_DOCKER" = true ]; then
  # Local build without Docker - requires gcc-10
  echo -e "${YELLOW}Building locally (without Docker)...${NC}"

  # Check for gcc-10
  if ! command -v gcc-10 &> /dev/null; then
    echo -e "${RED}ERROR: gcc-10 not found. Please install it with:${NC}"
    echo "  sudo apt-get install gcc-10"
    exit 1
  fi

  export CC=gcc-10
  echo -e "${YELLOW}Using gcc-10 compiler${NC}"

  if [ "$USE_CROSS" = true ]; then
    cross build --release --target "$TARGET" -p goose-cli
  else
    cargo build --release --target "$TARGET" -p goose-cli
  fi
else
  # Build with cross in Docker (recommended)
  echo -e "${YELLOW}Building with cross (Docker)...${NC}"

  if ! command -v docker &> /dev/null; then
    echo -e "${RED}ERROR: Docker not found. Please install Docker or use --no-docker with gcc-10 installed.${NC}"
    exit 1
  fi

  if ! docker info > /dev/null 2>&1; then
    echo -e "${RED}ERROR: Docker daemon is not running. Please start Docker.${NC}"
    exit 1
  fi

  if ! command -v cross &> /dev/null; then
    echo -e "${YELLOW}Installing cross...${NC}"
    if [ -f "./bin/activate-hermit" ]; then
      source ./bin/activate-hermit
    fi
    cargo install cross --git https://github.com/cross-rs/cross
  fi

  cross build --release --target "$TARGET" -p goose-cli
fi

# Verify build
BINARY_SUFFIX=""
if [[ "$TARGET" == *"windows"* ]]; then
  BINARY_SUFFIX=".exe"
fi

BINARY_PATH="target/$TARGET/release/goose$BINARY_SUFFIX"
if [ ! -f "$BINARY_PATH" ]; then
  echo -e "${RED}❌ Build failed: Binary not found at $BINARY_PATH${NC}"
  exit 1
fi

echo -e "${GREEN}✅ CLI binary built successfully!${NC}"
ls -lh "$BINARY_PATH"

# Package
echo -e "${YELLOW}Packaging CLI...${NC}"
mkdir -p "target/$TARGET/release/goose-package"

if [[ "$TARGET" == *"windows"* ]]; then
  cp "target/$TARGET/release/goose.exe" "target/$TARGET/release/goose-package/"
  cd "target/$TARGET/release"
  if command -v 7z &> /dev/null; then
    7z a -tzip "goose-${TARGET}.zip" goose-package/
    echo -e "${GREEN}✅ Packaged: goose-${TARGET}.zip${NC}"
    ls -lh "goose-${TARGET}.zip"
  else
    echo -e "${YELLOW}⚠️  7z not found, skipping zip creation${NC}"
  fi
else
  cp "target/$TARGET/release/goose" "target/$TARGET/release/goose-package/"
  cd "target/$TARGET/release"
  tar -cjf "goose-${TARGET}.tar.bz2" -C goose-package .
  tar -czf "goose-${TARGET}.tar.gz" -C goose-package .
  echo -e "${GREEN}✅ Packaged: goose-${TARGET}.tar.bz2${NC}"
  echo -e "${GREEN}✅ Packaged: goose-${TARGET}.tar.gz${NC}"
  ls -lh "goose-${TARGET}.tar.bz2" "goose-${TARGET}.tar.gz"
  cd - > /dev/null
fi

if [ -f "target/$TARGET/release/goose" ]; then
  cp "target/$TARGET/release/goose" "$HOME/.local/bin/goose" && chmod +x "$HOME/.local/bin/goose" && which goose && goose --version
else
  echo -e "${YELLOW}⚠️  Binary not found at target/$TARGET/release/goose, skipping local install${NC}"
fi

echo -e "${GREEN}=== Build Complete ===${NC}"
