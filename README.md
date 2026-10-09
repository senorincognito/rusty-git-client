# Rusty Git Client

A desktop Git GUI inspired by GitKraken's interface, for Windows and macOS. Built with
[Tauri 2](https://tauri.app) (a Rust backend using [libgit2](https://libgit2.org) and the system `git`) and
React + TypeScript + Vite.

**What it can do: [FEATURES.md](FEATURES.md).**

## Requirements

| Tool | Notes |
| --- | --- |
| [Node.js](https://nodejs.org) | 22 LTS (what CI uses) with npm |
| [Rust](https://rustup.rs) | stable, installed with rustup. On Windows use the MSVC toolchain |
| [Git](https://git-scm.com) | on your `PATH`. The app runs the system `git` for fetch, pull and push, and the backend tests need it |

Platform extras:

- **Windows**: the Visual Studio C++ Build Tools ("Desktop development with C++"). The WebView2 runtime ships with
  Windows 11; Windows 10 may need it from [Microsoft](https://developer.microsoft.com/microsoft-edge/webview2/).
- **macOS**: the Xcode Command Line Tools (`xcode-select --install`).

Restart your terminal (or editor) after installing Rust so `cargo` is on the `PATH`.

## Run it locally

```sh
npm install
npm run tauri dev
```

`npm run tauri dev` starts the Vite dev server and compiles the Rust backend, then opens the app window. Frontend
changes reload instantly; a change to the Rust code restarts the app. The first start compiles every Rust
dependency and takes a few minutes.

To try the optimised release build instead of the debug one, build it and start it in one go (no installers, so it is quicker than a full build):

```sh
npm run build:run                  # build, then start it
npm run build:run -- --skip-build  # start the release build that is already there
```

It works the same on Windows and macOS, and the app is started detached, so the command returns once it is running.

Don't use `npm run dev` for this: it only starts the Vite web server on port 1420 and opens no window, and the
page can't work in a browser because it talks to the Rust backend.

Checks you can run any time (the CI pipeline runs the same ones):

```sh
npm run lint                                                   # ESLint
npm run build                                                  # typecheck + bundle the frontend (and compile the SCSS)
cargo test --lib --manifest-path src-tauri/Cargo.toml          # backend tests
cargo fmt --manifest-path src-tauri/Cargo.toml --check         # formatting
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
```

## Build it

Builds a standalone app and installers for the OS you are on. A Mac is needed for the macOS build.

**Windows**: double-click `scripts\build-release.cmd`, or from a terminal:

```bat
scripts\build-release.cmd
```

**macOS / Linux / Git Bash**:

```sh
scripts/build-release.sh
```

Both scripts check the requirements, run `npm ci` if `node_modules` is missing, build, and print where the results
are. The first build takes a few minutes, later ones about a minute and a half. By hand it is
`npm ci` then `npm run tauri build`.

| Option (Windows / shell) | Effect |
| --- | --- |
| `-Bundles all\|nsis\|msi\|none` / `--bundles <list>` | Which installers to build (default: all for your OS) |
| `-Bundles none` / `--no-bundle` | Only the standalone app, no installers (fastest) |
| `-SkipInstall` / `--skip-install` | Don't run `npm ci` when `node_modules` is missing |
| `-Open` / `--open` | Open the output folder when done |
| `-Run` / `--run` | Start the app when the build is done |

Results are in `src-tauri/target/release/`: `rusty-git-client.exe` (runs standalone), `bundle/nsis/*-setup.exe` (setup
installer) and `bundle/msi/*.msi`. On macOS they are an `.app` and a `.dmg` under `bundle/`.

Good to know:

- The builds are **unsigned**: Windows SmartScreen shows "unknown publisher" (*More info*, then *Run anyway*) and macOS
  needs right-click, then Open, the first time.
- The icons are still the Tauri defaults; replace them with `npx tauri icon <your-logo.png>`.
- The version is set in `package.json`, `src-tauri/Cargo.toml` and `src-tauri/tauri.conf.json`; change all three together.

### Releases from CI

`.github/workflows/ci.yml` runs lint and tests on every push to `main` or a `release/*` branch and on every pull
request. Pushing a branch named `release/<version>` (for example `release/0.2.0`, matching the version in
`package.json`) also builds the Windows installers and a universal macOS `.dmg` and creates a draft GitHub release
`v<version>` with them attached.
