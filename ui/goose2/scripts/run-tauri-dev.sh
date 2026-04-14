#!/usr/bin/env bash
set -euo pipefail

mode="${1:-dev}"
case "$mode" in
  dev|debug) ;;
  *)
    echo "usage: $0 [dev|debug]" >&2
    exit 1
    ;;
esac

profile="${GOOSE2_PROFILE:-}"
if [[ -n "$profile" && -z "${GOOSE_PATH_ROOT:-}" ]]; then
  export GOOSE_PATH_ROOT="${TMPDIR:-/tmp}goose2-${profile}"
fi

port_key="$(pwd)::${profile:-default}"
vite_port=$(
  PORT_KEY="$port_key" python3 -c \
    "import hashlib,os; h=int(hashlib.sha256(os.environ['PORT_KEY'].encode()).hexdigest(),16); print(10000 + h % 55000)"
)
export VITE_PORT="$vite_port"

project_dir="$(pwd)"
tauri_config=$(
  PROJECT_DIR="$project_dir" \
  VITE_PORT="$vite_port" \
  GOOSE2_PROFILE="$profile" \
  python3 -c $'import json, os\nprofile = os.environ["GOOSE2_PROFILE"]\nconfig = {\n    "build": {\n        "devUrl": f"http://localhost:{os.environ[\"VITE_PORT\"]}",\n        "beforeDevCommand": {\n            "script": f"cd {os.environ[\"PROJECT_DIR\"]} && exec pnpm exec vite --port {os.environ[\"VITE_PORT\"]} --strictPort",\n            "cwd": ".",\n            "wait": False,\n        },\n    },\n}\nif profile:\n    config["identifier"] = f"com.goose.app.dev.{profile}"\n    config["productName"] = f"Goose Dev ({profile})"\nprint(json.dumps(config))'
)

if git rev-parse --is-inside-work-tree &>/dev/null; then
  git_dir=$(git rev-parse --git-dir)
  if [[ "$git_dir" == *".git/worktrees/"* ]]; then
    branch_name=$(git rev-parse --abbrev-ref HEAD)
    worktree_label="${branch_name##*/}"

    icon_dir="${project_dir}/src-tauri/target/dev-icons"
    mkdir -p "$icon_dir"
    dev_icon="$icon_dir/icon.icns"

    if swift scripts/generate-dev-icon.swift src-tauri/icons/icon.icns "$dev_icon" "$worktree_label"; then
      echo "🌳 Worktree: ${worktree_label}"
      tauri_config=$(python3 -c "import json,sys; config=json.loads(sys.argv[1]); config['bundle']={'icon':['$dev_icon']}; print(json.dumps(config))" "$tauri_config")
    fi
  fi
fi

cmd=(pnpm tauri dev)
if [[ "$mode" == "debug" ]]; then
  cmd+=(--config src-tauri/tauri.dev.conf.json)
else
  cmd+=(--features app-test-driver)
fi
cmd+=(--config "$tauri_config")

exec "${cmd[@]}"
