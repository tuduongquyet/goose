#!/bin/bash

# Build script for Goose 2 - mirrors GitHub Actions workflow
# Usage: ./build-goose2.sh [--test] [--e2e] [--help]

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Defaults (skip tests by default)
HELP=false
SKIP_TESTS=true
SKIP_E2E=true

# Parse arguments
while [[ $# -gt 0 ]]; do
  case $1 in
    --test)
      SKIP_TESTS=false
      shift
      ;;
    --e2e)
      SKIP_E2E=false
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
Goose 2 Build Script

Usage: ./build-goose2.sh [OPTIONS]

Options:
  --test                 Run unit tests (default: skip)
  --e2e                  Run E2E tests (default: skip)
  --help                 Show this help message

Examples:
  # Build without tests (default)
  ./build-goose2.sh

  # Build with unit tests
  ./build-goose2.sh --test

  # Build with E2E tests
  ./build-goose2.sh --e2e

  # Build with all tests
  ./build-goose2.sh --test --e2e

Note: This script builds the Goose 2 desktop application (frontend + Tauri backend).
It requires pnpm, Node.js 24+, Rust, and system dependencies (GTK, WebKit, etc.).
EOF
  exit 0
fi

echo -e "${GREEN}=== Goose 2 Build Script ===${NC}"

# Check if we're in the right directory
if [ ! -d "ui/goose2" ]; then
  echo -e "${RED}ERROR: ui/goose2 directory not found. Please run this script from the repository root.${NC}"
  exit 1
fi

cd ui/goose2

echo -e "${YELLOW}Working directory: $(pwd)${NC}"

# Detect OS
detect_os() {
  case "$OSTYPE" in
    linux*)   echo "linux" ;;
    darwin*)  echo "macos" ;;
    msys|mingw|cygwin) echo "windows" ;;
    *)        echo "unknown" ;;
  esac
}

DETECTED_OS=$(detect_os)
echo -e "${YELLOW}Detected OS: $DETECTED_OS${NC}"

# Install system dependencies if on Linux
if [ "$DETECTED_OS" = "linux" ]; then
  echo -e "${YELLOW}Installing system dependencies...${NC}"
  if command -v apt-get &> /dev/null; then
    sudo apt-get update
    sudo apt-get install -y \
      libgtk-3-dev \
      libwebkit2gtk-4.1-dev \
      libappindicator3-dev \
      librsvg2-dev \
      patchelf
    echo -e "${GREEN}✅ System dependencies installed${NC}"
  else
    echo -e "${YELLOW}⚠️  apt-get not found, skipping system dependency installation${NC}"
    echo -e "${YELLOW}   Please install: libgtk-3-dev, libwebkit2gtk-4.1-dev, libappindicator3-dev, librsvg2-dev, patchelf${NC}"
  fi
fi

# Check for Node.js
if ! command -v node &> /dev/null; then
  echo -e "${RED}ERROR: Node.js not found. Please install Node.js 24+.${NC}"
  exit 1
fi
NODE_VERSION=$(node -v)
echo -e "${YELLOW}Node.js version: $NODE_VERSION${NC}"

# Check for pnpm
if ! command -v pnpm &> /dev/null; then
  echo -e "${YELLOW}Installing pnpm...${NC}"
  npm install -g pnpm
fi
PNPM_VERSION=$(pnpm -v)
echo -e "${YELLOW}pnpm version: $PNPM_VERSION${NC}"

# Install Rust if needed
if ! command -v rustc &> /dev/null; then
  echo -e "${YELLOW}Installing Rust...${NC}"
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
  source "$HOME/.cargo/env"
fi
echo -e "${YELLOW}Rust version: $(rustc --version)${NC}"

# Install Rust components
echo -e "${YELLOW}Installing Rust components (rustfmt, clippy)...${NC}"
rustup component add rustfmt clippy

# Install dependencies
echo -e "${GREEN}Installing dependencies...${NC}"
pnpm install --frozen-lockfile

# Lint & Format checks
echo -e "${GREEN}Running lint and format checks...${NC}"
pnpm check
echo -e "${GREEN}✅ Lint check passed${NC}"

pnpm typecheck
echo -e "${GREEN}✅ Type check passed${NC}"

# Run unit tests
if [ "$SKIP_TESTS" = false ]; then
  echo -e "${GREEN}Running unit tests...${NC}"
  pnpm test
  echo -e "${GREEN}✅ Unit tests passed${NC}"
else
  echo -e "${YELLOW}⚠️  Skipping unit tests${NC}"
fi

# Build frontend
echo -e "${GREEN}Building frontend...${NC}"
pnpm build
echo -e "${GREEN}✅ Frontend built successfully${NC}"

# Check Rust backend
echo -e "${GREEN}Checking Tauri backend...${NC}"
cd src-tauri
cargo check
echo -e "${GREEN}✅ Tauri check passed${NC}"

# Rust format check
echo -e "${YELLOW}Checking Rust code format...${NC}"
cargo fmt --check
echo -e "${GREEN}✅ Rust format check passed${NC}"

# Clippy checks
echo -e "${YELLOW}Running Clippy lint...${NC}"
cargo clippy -- -D warnings
echo -e "${GREEN}✅ Clippy checks passed${NC}"

cd ..

# Run E2E tests
if [ "$SKIP_E2E" = false ]; then
  echo -e "${GREEN}Installing Playwright Chromium...${NC}"
  pnpm exec playwright install --with-deps chromium

  echo -e "${GREEN}Running E2E tests...${NC}"
  pnpm exec playwright test
  echo -e "${GREEN}✅ E2E tests passed${NC}"
else
  echo -e "${YELLOW}⚠️  Skipping E2E tests${NC}"
fi

echo -e "${GREEN}=== Build Complete ===${NC}"
echo -e "${GREEN}✅ Goose 2 build successful!${NC}"
