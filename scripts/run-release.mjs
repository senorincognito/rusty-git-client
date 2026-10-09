// Builds a release build of the app (no installers, so it is quicker than a full build) and starts it.
//   npm run build:run                 build, then run
//   npm run build:run -- --skip-build run the release build that is already there
// Works on Windows, macOS and Linux. The app is started detached, so this command returns once it is running.
import { spawn, spawnSync } from "node:child_process";
import { existsSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
process.chdir(root);

const args = process.argv.slice(2);
if (args.includes("--help") || args.includes("-h")) {
  console.log("Usage: npm run build:run [-- --skip-build]\n  --skip-build  don't build, just start the existing release build");
  process.exit(0);
}

const exe = path.join(root, "src-tauri", "target", "release", process.platform === "win32" ? "rusty-git-client.exe" : "rusty-git-client");

// shell: true so that "npm" resolves to npm.cmd on Windows.
const run = (cmd, cmdArgs) => spawnSync(cmd, cmdArgs, { stdio: "inherit", shell: true });

if (!args.includes("--skip-build")) {
  if (!existsSync(path.join(root, "node_modules"))) {
    console.log("\n==> Installing npm dependencies (npm ci)");
    if (run("npm", ["ci"]).status !== 0) process.exit(1);
  }
  console.log("\n==> Building the release build (no installers). The first build compiles all Rust dependencies and takes a few minutes.");
  const built = run("npm", ["run", "tauri", "build", "--", "--no-bundle"]);
  if (built.status !== 0) {
    console.error("\nThe build failed. The messages above say why.");
    process.exit(built.status ?? 1);
  }
}

if (!existsSync(exe)) {
  console.error(`\nNo release build found at ${exe}. Run without --skip-build first.`);
  process.exit(1);
}

console.log(`\n==> Starting ${exe}`);
spawn(exe, [], { detached: true, stdio: "ignore" }).unref();
