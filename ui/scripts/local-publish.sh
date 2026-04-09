#!/usr/bin/env bash
set -euo pipefail

###############################################################################
# local-publish.sh — Build all native packages and publish to npm locally.
#
# Builds goose CLI binaries for all platforms:
#   - macOS arm64 & x64: native cargo build
#   - Linux arm64 & x64: Docker containers (starts Docker Desktop if needed)
#
# Then builds the TypeScript packages and publishes everything.
#
# Usage:
#   ./ui/scripts/local-publish.sh                # dry-run (shows what would publish)
#   ./ui/scripts/local-publish.sh --publish       # publish for real
#   ./ui/scripts/local-publish.sh --publish --skip-build  # skip builds, just publish
#
# Prerequisites:
#   - Set NPM_PUBLISH_TOKEN in your environment, or place it in ~/.npmrc:
#       //registry.npmjs.org/:_authToken=npm_XXXXXXXXXXXX
#   - Docker Desktop installed (for Linux cross-compilation)
#   - Rust toolchain with targets: aarch64-apple-darwin, x86_64-apple-darwin
###############################################################################

REPO_ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
UI_DIR="${REPO_ROOT}/ui"
ACP_DIR="${UI_DIR}/acp"
TEXT_DIR="${UI_DIR}/text"
NATIVE_DIR="${UI_DIR}/goose-binary"
ACP_CRATE="${REPO_ROOT}/crates/goose-acp"

PUBLISH=false
SKIP_BUILD=false
SKIP_LINUX=false

for arg in "$@"; do
  case "$arg" in
    --publish) PUBLISH=true ;;
    --skip-build) SKIP_BUILD=true ;;
    --skip-linux) SKIP_LINUX=true ;;
    --help|-h)
      echo "Usage: $0 [--publish] [--skip-build] [--skip-linux]"
      echo ""
      echo "  --publish     Actually publish to npm (default is dry-run)"
      echo "  --skip-build  Skip all Rust/TS builds, just publish what's there"
      echo "  --skip-linux  Skip Linux builds (macOS-only, faster iteration)"
      exit 0
      ;;
    *)
      echo "Unknown argument: $arg"
      exit 1
      ;;
  esac
done

# ── Colors ───────────────────────────────────────────────────────────────────
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
BOLD='\033[1m'
NC='\033[0m'

step() { echo -e "\n${BLUE}${BOLD}==> $1${NC}"; }
ok()   { echo -e "    ${GREEN}✅ $1${NC}"; }
warn() { echo -e "    ${YELLOW}⚠️  $1${NC}"; }
fail() { echo -e "    ${RED}❌ $1${NC}"; exit 1; }

# ── Preflight ────────────────────────────────────────────────────────────────
step "Preflight checks"

if [[ "$PUBLISH" == true ]]; then
  if [[ -n "${NPM_PUBLISH_TOKEN:-}" ]]; then
    ok "NPM_PUBLISH_TOKEN is set in environment"
  elif grep -q "registry.npmjs.org/:_authToken" ~/.npmrc 2>/dev/null; then
    ok "Found npm token in ~/.npmrc"
  else
    fail "No npm credentials found. Either:\n       export NPM_PUBLISH_TOKEN=npm_XXXXXXXXXXXX\n       or add to ~/.npmrc:\n       //registry.npmjs.org/:_authToken=npm_XXXXXXXXXXXX"
  fi
else
  echo -e "    ${YELLOW}DRY RUN — pass --publish to publish for real${NC}"
fi

# Check Rust targets for macOS
if ! rustup target list --installed | grep -q "aarch64-apple-darwin"; then
  fail "Missing Rust target: aarch64-apple-darwin (run: rustup target add aarch64-apple-darwin)"
fi
if ! rustup target list --installed | grep -q "x86_64-apple-darwin"; then
  fail "Missing Rust target: x86_64-apple-darwin (run: rustup target add x86_64-apple-darwin)"
fi
ok "Rust macOS targets installed"

# ── Build native binaries ────────────────────────────────────────────────────
if [[ "$SKIP_BUILD" == false ]]; then

  # ── macOS builds (native) ──────────────────────────────────────────────────
  step "Building goose for darwin-arm64 (native)"
  cargo build --release --target aarch64-apple-darwin --bin goose --manifest-path "${REPO_ROOT}/Cargo.toml"
  mkdir -p "${NATIVE_DIR}/goose-binary-darwin-arm64/bin"
  cp "${REPO_ROOT}/target/aarch64-apple-darwin/release/goose" "${NATIVE_DIR}/goose-binary-darwin-arm64/bin/goose"
  chmod +x "${NATIVE_DIR}/goose-binary-darwin-arm64/bin/goose"
  ok "darwin-arm64 binary ready"

  step "Building goose for darwin-x64 (native cross-compile)"
  cargo build --release --target x86_64-apple-darwin --bin goose --manifest-path "${REPO_ROOT}/Cargo.toml"
  mkdir -p "${NATIVE_DIR}/goose-binary-darwin-x64/bin"
  cp "${REPO_ROOT}/target/x86_64-apple-darwin/release/goose" "${NATIVE_DIR}/goose-binary-darwin-x64/bin/goose"
  chmod +x "${NATIVE_DIR}/goose-binary-darwin-x64/bin/goose"
  ok "darwin-x64 binary ready"

  # ── Linux builds (Docker) ──────────────────────────────────────────────────
  if [[ "$SKIP_LINUX" == false ]]; then
    step "Checking Docker for Linux builds"

    if ! command -v docker &>/dev/null; then
      fail "Docker not installed. Install Docker Desktop or pass --skip-linux"
    fi

    # Try to start Docker Desktop if daemon isn't running
    if ! docker info &>/dev/null 2>&1; then
      warn "Docker daemon not running — attempting to start Docker Desktop..."
      open -a Docker 2>/dev/null || true
      echo -n "    Waiting for Docker"
      for i in $(seq 1 60); do
        if docker info &>/dev/null 2>&1; then
          echo ""
          ok "Docker is ready"
          break
        fi
        echo -n "."
        sleep 2
      done
      if ! docker info &>/dev/null 2>&1; then
        fail "Docker failed to start after 120s. Start Docker Desktop manually or use --skip-linux"
      fi
    else
      ok "Docker is running"
    fi

    # Build a reusable Rust builder image
    step "Building Docker image for Linux compilation"
    DOCKER_IMAGE="goose-linux-builder"

    # Use $HOME for temp dir — macOS $TMPDIR (/var/folders/...) is not shared with Docker Desktop
    DOCKER_CTX=$(mktemp -d "$HOME/.goose-docker-ctx.XXXXXX")
    cat > "${DOCKER_CTX}/Dockerfile" <<'DOCKERFILE'
FROM rust:1.87-bookworm

RUN apt-get update && apt-get install -y \
    gcc-aarch64-linux-gnu \
    g++-aarch64-linux-gnu \
    libc6-dev-arm64-cross \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

RUN rustup target add x86_64-unknown-linux-gnu aarch64-unknown-linux-gnu

# Configure cross-compilation linker for aarch64
RUN mkdir -p /root/.cargo && \
    printf '[target.aarch64-unknown-linux-gnu]\nlinker = "aarch64-linux-gnu-gcc"\n' > /root/.cargo/config.toml

WORKDIR /build
DOCKERFILE

    docker build --provenance=false --sbom=false -t "${DOCKER_IMAGE}" "${DOCKER_CTX}" 2>&1 || fail "Docker image build failed"
    rm -rf "${DOCKER_CTX}"

    # Verify the image actually exists before proceeding
    if ! docker image inspect "${DOCKER_IMAGE}" &>/dev/null; then
      fail "Docker build appeared to succeed but image '${DOCKER_IMAGE}' not found. Try: docker buildx use default"
    fi
    ok "Docker builder image ready"

    step "Building goose for linux-x64 (Docker)"
    docker run --rm \
      -v "${REPO_ROOT}:/build" \
      -w /build \
      "${DOCKER_IMAGE}" \
      cargo build --release --target x86_64-unknown-linux-gnu --bin goose
    mkdir -p "${NATIVE_DIR}/goose-binary-linux-x64/bin"
    cp "${REPO_ROOT}/target/x86_64-unknown-linux-gnu/release/goose" "${NATIVE_DIR}/goose-binary-linux-x64/bin/goose"
    chmod +x "${NATIVE_DIR}/goose-binary-linux-x64/bin/goose"
    ok "linux-x64 binary ready"

    step "Building goose for linux-arm64 (Docker cross-compile)"
    docker run --rm \
      -v "${REPO_ROOT}:/build" \
      -w /build \
      -e PKG_CONFIG_ALLOW_CROSS=1 \
      -e OPENSSL_NO_VENDOR=0 \
      "${DOCKER_IMAGE}" \
      cargo build --release --target aarch64-unknown-linux-gnu --bin goose
    mkdir -p "${NATIVE_DIR}/goose-binary-linux-arm64/bin"
    cp "${REPO_ROOT}/target/aarch64-unknown-linux-gnu/release/goose" "${NATIVE_DIR}/goose-binary-linux-arm64/bin/goose"
    chmod +x "${NATIVE_DIR}/goose-binary-linux-arm64/bin/goose"
    ok "linux-arm64 binary ready"

  else
    warn "Skipping Linux builds (--skip-linux)"
  fi

  # ── Generate ACP schema ────────────────────────────────────────────────────
  step "Generating ACP schema from Rust"
  cargo build --release --bin generate-acp-schema --manifest-path "${REPO_ROOT}/Cargo.toml"
  (cd "${ACP_CRATE}" && cargo run --release --bin generate-acp-schema)
  ok "acp-schema.json and acp-meta.json generated"

  # ── Build TypeScript packages ──────────────────────────────────────────────
  step "Installing npm dependencies"
  (cd "${UI_DIR}" && pnpm install --frozen-lockfile)
  ok "Dependencies installed"

  step "Generating TypeScript types from ACP schema"
  (cd "${ACP_DIR}" && npx tsx generate-schema.ts)
  ok "TypeScript types generated"

  step "Building @aaif/goose-acp"
  (cd "${ACP_DIR}" && pnpm run build:ts)
  ok "@aaif/goose-acp built"

  step "Building @aaif/goose"
  (cd "${TEXT_DIR}" && pnpm run build)
  ok "@aaif/goose built"

else
  warn "Skipping all builds (--skip-build)"
fi

# ── Summary ──────────────────────────────────────────────────────────────────
step "Package summary"
echo ""
printf "    %-45s %-10s %s\n" "PACKAGE" "VERSION" "BINARY"
printf "    %-45s %-10s %s\n" "───────" "───────" "──────"

for pkg_json in \
  "${ACP_DIR}/package.json" \
  "${TEXT_DIR}/package.json" \
  "${NATIVE_DIR}/goose-binary-darwin-arm64/package.json" \
  "${NATIVE_DIR}/goose-binary-darwin-x64/package.json" \
  "${NATIVE_DIR}/goose-binary-linux-arm64/package.json" \
  "${NATIVE_DIR}/goose-binary-linux-x64/package.json" \
  "${NATIVE_DIR}/goose-binary-win32-x64/package.json"; do

  name=$(jq -r '.name' "$pkg_json")
  version=$(jq -r '.version' "$pkg_json")

  pkg_dir=$(dirname "$pkg_json")
  bin_status="-"
  if [[ -f "${pkg_dir}/bin/goose" ]]; then
    bin_status="✅ $(du -h "${pkg_dir}/bin/goose" | cut -f1 | xargs)"
  elif [[ -f "${pkg_dir}/bin/goose.exe" ]]; then
    bin_status="✅ $(du -h "${pkg_dir}/bin/goose.exe" | cut -f1 | xargs)"
  elif echo "$name" | grep -q "binary"; then
    bin_status="❌ missing"
  fi

  printf "    %-45s %-10s %s\n" "$name" "$version" "$bin_status"
done
echo ""

# ── Publish ──────────────────────────────────────────────────────────────────
if [[ "$PUBLISH" == true ]]; then
  step "Publishing to npm"

  # Set up .npmrc for this session if token is in env
  if [[ -n "${NPM_PUBLISH_TOKEN:-}" ]]; then
    export NPM_CONFIG_REGISTRY="https://registry.npmjs.org/"
    echo "//registry.npmjs.org/:_authToken=${NPM_PUBLISH_TOKEN}" > "${UI_DIR}/.npmrc"
    ok "Configured npm auth from NPM_PUBLISH_TOKEN"
  fi

  # Publish order matters: dependencies first

  # 1. ACP (no deps on other workspace packages)
  echo -e "\n    Publishing @aaif/goose-acp..."
  (cd "${ACP_DIR}" && npm publish --access public)
  ok "@aaif/goose-acp published"

  # 2. Native binary packages (no deps)
  NATIVE_PACKAGES=(
    "goose-binary-darwin-arm64"
    "goose-binary-darwin-x64"
    "goose-binary-linux-arm64"
    "goose-binary-linux-x64"
  )

  for pkg in "${NATIVE_PACKAGES[@]}"; do
    pkg_dir="${NATIVE_DIR}/${pkg}"
    if [[ -f "${pkg_dir}/bin/goose" ]] || [[ -f "${pkg_dir}/bin/goose.exe" ]]; then
      echo -e "\n    Publishing @aaif/${pkg}..."
      (cd "${pkg_dir}" && npm publish --access public)
      ok "@aaif/${pkg} published"
    else
      warn "Skipping @aaif/${pkg} (no binary)"
    fi
  done

  # 3. Text/TUI package (depends on acp + binary packages)
  # Rewrite workspace:* references to actual versions for publishing
  echo -e "\n    Preparing @aaif/goose for publish..."
  ACP_VERSION=$(jq -r '.version' "${ACP_DIR}/package.json")
  BINARY_VERSION=$(jq -r '.version' "${NATIVE_DIR}/goose-binary-darwin-arm64/package.json")

  # Create a temp copy with resolved workspace references
  cp "${TEXT_DIR}/package.json" "${TEXT_DIR}/package.json.bak"

  jq --arg acp_ver "$ACP_VERSION" --arg bin_ver "$BINARY_VERSION" '
    .dependencies["@aaif/goose-acp"] = $acp_ver |
    .optionalDependencies["@aaif/goose-binary-darwin-arm64"] = $bin_ver |
    .optionalDependencies["@aaif/goose-binary-darwin-x64"] = $bin_ver |
    .optionalDependencies["@aaif/goose-binary-linux-arm64"] = $bin_ver |
    .optionalDependencies["@aaif/goose-binary-linux-x64"] = $bin_ver |
    .optionalDependencies["@aaif/goose-binary-win32-x64"] = $bin_ver
  ' "${TEXT_DIR}/package.json.bak" > "${TEXT_DIR}/package.json"

  echo -e "\n    Publishing @aaif/goose..."
  (cd "${TEXT_DIR}" && npm publish --access public)
  ok "@aaif/goose published"

  # Restore original package.json
  mv "${TEXT_DIR}/package.json.bak" "${TEXT_DIR}/package.json"

  # Clean up session .npmrc
  rm -f "${UI_DIR}/.npmrc"

  step "🎉 All packages published!"

else
  step "Dry run complete"
  echo -e "    Run with ${BOLD}--publish${NC} to publish for real."
  echo -e "    Run with ${BOLD}--skip-linux${NC} to skip Linux builds."
  echo ""
fi
