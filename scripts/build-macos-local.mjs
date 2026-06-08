import { spawnSync } from "node:child_process";

function run(command, args) {
  const result = spawnSync(command, args, {
    stdio: "inherit",
    encoding: "utf8",
  });
  return result.status ?? 1;
}

if (process.platform !== "darwin") {
  console.error("`pnpm tauri:build:mac` is only intended for local macOS builds.");
  process.exit(1);
}

const localConfig = JSON.stringify({
  bundle: {
    createUpdaterArtifacts: false,
  },
});

let status = run("pnpm", ["exec", "tauri", "build", "--bundles", "app", "--config", localConfig]);
if (status !== 0) process.exit(status);

status = run("node", ["scripts/fix-macos-app.mjs"]);
process.exit(status);
