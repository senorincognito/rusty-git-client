#!/usr/bin/env bash
# Builds Rusty Git Client into a standalone app and installers for the current OS.
#   Windows (Git Bash): exe + NSIS setup + MSI     macOS: .app + .dmg     Linux: deb / rpm / AppImage
# Windows users can use scripts/build-release.cmd instead. macOS builds must be made on a Mac.
#
# Usage: scripts/build-release.sh [--bundles <list>] [--no-bundle] [--skip-install] [--open] [--run]
#   --bundles <list>  only these bundle types, e.g. "nsis", "msi", "dmg" (default: all of this OS)
#   --no-bundle       only the standalone binary, no installers (fastest)
#   --skip-install    don't run "npm ci" when node_modules is missing
#   --open            open the output folder when done
#   --run             start the app when the build is done
set -euo pipefail

cd "$(dirname "${BASH_SOURCE[0]}")/.."

bundles=""
no_bundle=0
skip_install=0
open_folder=0
run_app=0
while [ $# -gt 0 ]; do
  case "$1" in
    --bundles)      bundles="${2:?--bundles needs a value}"; shift 2 ;;
    --no-bundle)    no_bundle=1; shift ;;
    --skip-install) skip_install=1; shift ;;
    --open)         open_folder=1; shift ;;
    --run)          run_app=1; shift ;;
    -h|--help)      sed -n '2,11p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
    *)              echo "Unknown option: $1 (try --help)" >&2; exit 2 ;;
  esac
done

step() { printf '\n==> %s\n' "$1"; }
need() { command -v "$1" >/dev/null 2>&1 || { echo "'$1' was not found. $2" >&2; exit 1; }; }

# rustup installs cargo into ~/.cargo/bin, which a shell opened before the install may not have yet.
case ":$PATH:" in *":$HOME/.cargo/bin:"*) ;; *) [ -d "$HOME/.cargo/bin" ] && export PATH="$PATH:$HOME/.cargo/bin" ;; esac

step "Checking prerequisites"
need node  "Install Node.js from https://nodejs.org"
need npm   "It comes with Node.js: https://nodejs.org"
need cargo "Install Rust from https://rustup.rs, then open a new terminal."
echo "node  $(node --version)"
echo "npm   $(npm --version)"
echo "rustc $(rustc --version)"
command -v git >/dev/null 2>&1 || echo "Warning: git not found. The build doesn't need it, but the app runs 'git' at runtime." >&2

if [ ! -d node_modules ]; then
  [ "$skip_install" -eq 1 ] && { echo 'node_modules is missing and --skip-install was given. Run "npm ci" first.' >&2; exit 1; }
  step "Installing npm dependencies (npm ci)"
  npm ci
fi

step "Building. The first build compiles all Rust dependencies and takes a few minutes."
args=(run tauri build)
if [ "$no_bundle" -eq 1 ]; then
  args+=(-- --no-bundle)
elif [ -n "$bundles" ]; then
  args+=(-- --bundles "$bundles")
fi
started=$(date +%s)
marker=$(mktemp)   # installers newer than this were made by this run
trap 'rm -f "$marker"' EXIT
npm "${args[@]}"

step "Done in $(( $(date +%s) - started )) seconds. Output:"
release="src-tauri/target/release"
{
  # "|| true": a missing folder (e.g. bundle/macos on Windows) must not abort the script under pipefail.
  find "$release" -maxdepth 1 -type f \( -name 'rusty-git-client' -o -name 'rusty-git-client.exe' \) 2>/dev/null || true
  if [ -d "$release/bundle" ]; then
    find "$release/bundle" -type f -newer "$marker" \( -name '*.exe' -o -name '*.msi' -o -name '*.dmg' -o -name '*.deb' -o -name '*.rpm' -o -name '*.AppImage' \) || true
    find "$release/bundle/macos" -maxdepth 1 -newer "$marker" -name '*.app' 2>/dev/null || true
  fi
} | while IFS= read -r f; do
  if [ -d "$f" ]; then size="    -"; else size=$(du -h "$f" | cut -f1); fi
  printf '  %6s  %s\n' "$size" "$PWD/$f"
done
echo
echo "The builds are unsigned: Windows SmartScreen and macOS Gatekeeper will warn the first time they run."

if [ "$open_folder" -eq 1 ]; then
  target="$release/bundle"; { [ "$no_bundle" -eq 1 ] || [ ! -d "$target" ]; } && target="$release"
  case "$(uname -s)" in
    Darwin) open "$target" ;;
    MINGW*|MSYS*|CYGWIN*) explorer.exe "$(cygpath -w "$target")" || true ;;
    *) xdg-open "$target" ;;
  esac
fi

if [ "$run_app" -eq 1 ]; then
  app="$release/rusty-git-client"; [ -f "$app.exe" ] && app="$app.exe"
  step "Starting $app"
  ( "$app" >/dev/null 2>&1 & )
fi
