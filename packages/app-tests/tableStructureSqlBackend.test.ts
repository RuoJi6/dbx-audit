import { readFileSync } from "node:fs";
import { strict as assert } from "node:assert";
import test from "node:test";

const apiSource = readFileSync("apps/desktop/src/lib/api.ts", "utf8");
const tauriSource = readFileSync("apps/desktop/src/lib/tauri.ts", "utf8");
const httpSource = readFileSync("apps/desktop/src/lib/http.ts", "utf8");
const dialogSource = readFileSync("apps/desktop/src/components/structure/TableStructureEditor.vue", "utf8");
const tableStructureTypesSource = readFileSync("apps/desktop/src/lib/tableStructureEditorSql.ts", "utf8");
const rustCoreLibSource = readFileSync("crates/dbx-core/src/lib.rs", "utf8");
const rustTableStructureSqlSource = readFileSync("crates/dbx-core/src/table_structure_sql.rs", "utf8");
const tauriLibSource = readFileSync("src-tauri/src/lib.rs", "utf8");
const webMainSource = readFileSync("crates/dbx-web/src/main.rs", "utf8");

test("shared API exposes backend table structure SQL builders", () => {
  assert.match(apiSource, /export const buildTableStructureChangeSql = forward\("buildTableStructureChangeSql"\)/);
  assert.match(apiSource, /export const buildCreateTableSql = forward\("buildCreateTableSql"\)/);

  assert.match(tauriSource, /invoke\("build_table_structure_change_sql"/);
  assert.match(tauriSource, /invoke\("build_create_table_sql"/);

  assert.match(httpSource, /\/api\/query\/build-table-structure-change-sql/);
  assert.match(httpSource, /\/api\/query\/build-create-table-sql/);
});

test("table structure editor delegates SQL preview generation to backend APIs", () => {
  assert.match(dialogSource, /await api\.buildCreateTableSql\(options\)/);
  assert.match(dialogSource, /await api\.buildTableStructureChangeSql\(options\)/);
  assert.doesNotMatch(dialogSource, /buildCreateTableSql,\s*buildTableStructureChangeSql/);
});

test("frontend keeps table structure SQL file as types only", () => {
  assert.match(tableStructureTypesSource, /export interface EditableStructureColumn/);
  assert.match(tableStructureTypesSource, /export interface BuildTableStructureChangeSqlOptions/);
  assert.doesNotMatch(tableStructureTypesSource, /export function buildTableStructureChangeSql/);
  assert.doesNotMatch(tableStructureTypesSource, /export function buildCreateTableSql/);
  assert.doesNotMatch(tableStructureTypesSource, /function quoteIdent/);
  assert.doesNotMatch(tableStructureTypesSource, /function columnDefinition/);
});

test("Rust backends register table structure SQL builders", () => {
  assert.match(rustCoreLibSource, /pub mod table_structure_sql/);
  assert.match(rustTableStructureSqlSource, /pub fn build_table_structure_change_sql/);
  assert.match(rustTableStructureSqlSource, /pub fn build_create_table_sql/);
  assert.match(tauriLibSource, /commands::query::build_table_structure_change_sql/);
  assert.match(tauriLibSource, /commands::query::build_create_table_sql/);
  assert.match(webMainSource, /\/query\/build-table-structure-change-sql/);
  assert.match(webMainSource, /\/query\/build-create-table-sql/);
});
