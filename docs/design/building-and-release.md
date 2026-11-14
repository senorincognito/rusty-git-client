# Building and release: design notes

Contributor notes moved out of CLAUDE.md: how it is built and why. What the user sees is described in `docs/features/`. Covers CI, the build scripts, the app icon and release notes.

**Decision: unsigned builds, no auto-updater.** The author builds locally with the scripts below, or takes the installers
from the CI workflow (next paragraph). Don't add signing or an updater unless asked; if distribution to others comes up, the options already discussed are:
`tauri-action` on GitHub Actions (builds Windows + macOS and attaches them to a Release), `tauri-plugin-updater`
(needs its own signing key pair, kept as a CI secret and never committed, plus a `latest.json` on the
Release), and a Windows code-signing certificate to avoid the SmartScreen "unknown publisher" warning.

- **CI** (`.github/workflows/ci.yml`): jobs `frontend` (ESLint, `npm run build`), `rust-lint` (rustfmt, clippy `-D warnings`),
  `rust-test` (Windows and macOS only, `fail-fast: false`) and, after all three, `build` (Windows NSIS+MSI; macOS universal
  DMG via `--target universal-apple-darwin`) uploading artifacts. Lint and tests run on every push and PR; `build` and
  `release` run **only on a `release/<version>` branch** (the name must equal the `package.json` version, checked in `build`),
  and `release` creates a **draft** release `v<version>` with `gh release create` targeting that commit. No third-party release action, no
  secrets beyond `GITHUB_TOKEN`. Builds are unsigned (macOS needs right-click > Open). Nothing here has run on GitHub yet when this
  was written: if a job fails, fix the workflow from the log (Linux needs the webkit2gtk/xdo/appindicator packages just to compile).
  ESLint (`eslint.config.js`) enables only `rules-of-hooks` and `exhaustive-deps` from the react-hooks plugin: its React Compiler
  rules flag deliberate patterns (state reset in effects on `path` change, ref sync during render).
  **The tests are not run on Linux**: that job hung (after the `terminal` chunker test; the PTY test forking a shell while
  other tests spawn git on other threads is the suspect, unconfirmed), so the Ubuntu leg was dropped. Re-enable it by adding `ubuntu-latest` to the matrix (it needs the webkit2gtk/xdo/appindicator packages). Every
  job has a timeout. Lint (`rust-lint`) and the frontend still run on Ubuntu.
- Build with the scripts in `scripts/`: `build-release.cmd` (Windows launcher, double-clickable; runs the
  `.ps1` with `-ExecutionPolicy Bypass` because the default policy blocks unsigned .ps1 files) and
  `build-release.sh` (macOS/Linux/Git Bash; tested here only under Git Bash on Windows). Both check the
  toolchain, run `npm ci` if needed, build and list the fresh artifacts; options: `-Bundles all|nsis|msi|none`
  / `--bundles`, `--no-bundle`, skip install, open folder. `.gitattributes` pins their line endings (sh = LF,
  cmd/ps1 = CRLF), and the `.sh` must be committed with the executable bit
  (`git update-index --chmod=+x scripts/build-release.sh`). Underneath it is `npm run tauri build` (verified
  working, about 2.5 minutes for a cold release build, about 1.5 minutes for a rebuild). It produces, under `src-tauri/target/release/`: `rusty-git-client.exe` (6.9 MB, runs
  standalone given WebView2), `bundle/nsis/Rusty Git Client_0.1.0_x64-setup.exe` (2.2 MB) and
  `bundle/msi/Rusty Git Client_0.1.0_x64_en-US.msi` (3.1 MB). The first build downloads the WiX and NSIS tools
  from GitHub. All of `target/` and `dist/` is git-ignored. Run it with `export PATH="$PATH:$HOME/.cargo/bin"`
  in the Bash tool; a cold build prints one line per crate, so redirect output to a log in the scratchpad and
  run it in the background.
- **Build and run**: `npm run build:run` (`scripts/run-release.mjs`, plain Node so it behaves the same everywhere) runs `npm run tauri build -- --no-bundle` and starts `src-tauri/target/release/rusty-git-client[.exe]` detached (`--skip-build` only starts it). The build scripts have the same as a flag: `-Run` (ps1/cmd) and `--run` (sh). Only `--help`, the syntax of the scripts and the build step (which is the normal `tauri build`) were checked; the launch itself was not run from here, since it opens a window.
- Gotchas from writing the scripts: the `.cmd` pauses at the end only when started with no arguments (the
  double-click case); set `NOPAUSE=1` when another program runs it. In the `.sh`, `set -e` + `pipefail`
  turns a failing `find` on a missing folder (`bundle/macos` on Windows) into a silent script failure: guard
  such commands with `|| true`. Installers are listed only if newer than the run's start, so stale ones from an
  earlier build aren't reported as results. Keep the `.ps1` ASCII-only (Windows PowerShell 5.1 misreads
  BOM-less UTF-8).
- **Release notes** live in `docs/releases/v<version>.md` (the name matches the tag `release/<version>` creates), one file per
  release, written for the people who download it: requirements, downloads, what is in it, known limitations. Write the next
  one when the version is bumped; copy the structure of the previous file and list only what changed.
- macOS builds can only be made on a Mac.
- **The app shells out to the system `git`**, so users need Git installed and on PATH; the installers cannot
  bundle it. The app reports "git executable not found" when it is missing.
- The app icon (a red long branch and a blue short branch on a dark tile) was drawn with GDI+ in PowerShell (no Python/ImageMagick here); `src-tauri/icons/icon-source.png` (1024 px) is the source, regenerate the set with `npx tauri icon src-tauri/icons/icon-source.png`. That also creates `android/`, `ios/` and `64x64.png`, which are not used (desktop only): delete them again.
- The version lives in `package.json`, `src-tauri/Cargo.toml` and `src-tauri/tauri.conf.json`; keep them in sync.
- The bundle identifier is `net.senorincognito.gitclient` (it keys the saved data), see "Persisted state". It replaced
  `com.gitclient.app`, which no longer ends in `.app` (that clashed with the macOS bundle extension).
