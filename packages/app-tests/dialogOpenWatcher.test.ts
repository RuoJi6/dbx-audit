import { readFileSync } from "node:fs";
import assert from "node:assert/strict";
import test from "node:test";

const mountedOpenDialogs = [
  "apps/desktop/src/components/connection/ConnectionDialog.vue",
  "apps/desktop/src/components/transfer/DataTransferDialog.vue",
  "apps/desktop/src/components/diff/SchemaDiffDialog.vue",
  "apps/desktop/src/components/diff/DataCompareDialog.vue",
  "apps/desktop/src/components/sql-file/SqlFileExecutionDialog.vue",
  "apps/desktop/src/components/diagram/SchemaDiagramDialog.vue",
  "apps/desktop/src/components/import/TableImportDialog.vue",
  "apps/desktop/src/components/lineage/FieldLineageDialog.vue",
  "apps/desktop/src/components/search/DatabaseSearchDialog.vue",
  "apps/desktop/src/components/export/DatabaseExportDialog.vue",
  "apps/desktop/src/components/config/ConfigPassphraseDialog.vue",
] as const;

test("dialogs initialized through v-if run their open watcher on mount", () => {
  for (const filePath of mountedOpenDialogs) {
    const source = readFileSync(filePath, "utf8");
    assert.match(
      source,
      /watch\(\s*(open|dialogOpen),[\s\S]*?\{\s*immediate:\s*true\s*\},?\s*\)/,
      `${filePath} should use an immediate open watcher`,
    );
  }
});
