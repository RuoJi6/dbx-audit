import { readFileSync } from "node:fs";
import { strict as assert } from "node:assert";
import test from "node:test";

test("settings about panel uses the app version prop instead of a hard-coded version", () => {
  const source = readFileSync("src/components/editor/EditorSettingsDialog.vue", "utf8");

  assert.equal(source.includes("v0.5.0"), false);
  assert.match(source, /appVersion/);
});

test("release workflow does not publish the default Tauri release body", () => {
  const source = readFileSync(".github/workflows/release.yml", "utf8");

  assert.equal(source.includes("See the assets below to download and install."), false);
});
