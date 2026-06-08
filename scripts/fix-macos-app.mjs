import { existsSync, readdirSync, statSync } from "node:fs";
import path from "node:path";
import { spawnSync } from "node:child_process";

const repoRoot = process.cwd();
const appName = "DBX Audit.app";
const searchRoots = ["target", "src-tauri/target"].map((item) => path.join(repoRoot, item));

function run(command, args, options = {}) {
  const result = spawnSync(command, args, {
    cwd: repoRoot,
    encoding: "utf8",
    stdio: options.silent ? "pipe" : "inherit",
  });
  return result.status === 0;
}

function findApps(dir, apps = []) {
  if (!existsSync(dir)) return apps;

  for (const entry of readdirSync(dir)) {
    const fullPath = path.join(dir, entry);
    let stats;
    try {
      stats = statSync(fullPath);
    } catch {
      continue;
    }

    if (stats.isDirectory() && entry === appName) {
      apps.push(fullPath);
      continue;
    }

    if (stats.isDirectory() && entry.endsWith(".app")) continue;
    if (stats.isDirectory()) findApps(fullPath, apps);
  }

  return apps;
}

if (process.platform !== "darwin") {
  console.log("macOS app repair skipped: current platform is not macOS.");
  process.exit(0);
}

const apps = Array.from(new Set(searchRoots.flatMap((root) => findApps(root))));

if (apps.length === 0) {
  console.log("No DBX Audit.app bundle found. Run `pnpm tauri build` first.");
  process.exit(0);
}

let failed = false;

for (const appPath of apps) {
  console.log(`Repairing ${path.relative(repoRoot, appPath)}`);
  run("xattr", ["-dr", "com.apple.quarantine", appPath], { silent: true });
  run("xattr", ["-dr", "com.apple.provenance", appPath], { silent: true });

  if (!run("codesign", ["--force", "--deep", "--sign", "-", appPath])) {
    failed = true;
    continue;
  }

  if (!run("codesign", ["--verify", "--deep", "--strict", "--verbose=2", appPath])) {
    failed = true;
  }
}

process.exit(failed ? 1 : 0);
