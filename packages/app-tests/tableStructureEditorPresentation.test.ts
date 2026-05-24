import { readFileSync } from "node:fs";
import { strict as assert } from "node:assert";
import test from "node:test";

const source = readFileSync("apps/desktop/src/components/structure/TableStructureEditor.vue", "utf8");
const clickhouseSource = readFileSync("crates/dbx-core/src/db/clickhouse_driver.rs", "utf8");

test("column comments can be expanded into a multiline editor", () => {
  assert.match(source, /PopoverContent/);
  assert.match(source, /v-model="column\.comment"/);
  assert.match(source, /<textarea[\s\S]*v-model="column\.comment"/);
  assert.match(source, /t\("structureEditor\.editComment"\)/);
});

test("structure editor keeps columns when optional metadata fails", () => {
  assert.match(source, /const nextColumns = await api\.getColumns/);
  assert.match(source, /api\.listIndexes[\s\S]*?\.catch\(\(\) => \[\]\)/);
  assert.match(source, /api\.listForeignKeys[\s\S]*?\.catch\(\(\) => \[\]\)/);
  assert.match(source, /api\.listTriggers[\s\S]*?\.catch\(\(\) => \[\]\)/);
});

test("ClickHouse column metadata preserves comments for structure editing", () => {
  assert.match(clickhouseSource, /SELECT name, type, default_kind, default_expression, is_in_primary_key, comment/);
  assert.match(clickhouseSource, /comment:\s*row\.get\(5\)/);
});

test("structure editor loads table metadata on mount", () => {
  assert.match(source, /async function loadStructure/);
  assert.match(source, /api\.getColumns/);
  assert.match(source, /api\.listIndexes/);
  assert.match(source, /api\.listForeignKeys/);
  assert.match(source, /api\.listTriggers/);
});

test("structure editor gates controls through table structure capabilities", () => {
  assert.match(source, /getTableStructureCapabilities/);
  assert.match(source, /const structureCapabilities = computed/);
  assert.match(source, /function isColumnNameDisabled/);
  assert.match(source, /function isColumnTypeDisabled/);
  assert.match(source, /function isColumnDefaultDisabled/);
  assert.match(source, /function isColumnCommentDisabled/);
  assert.match(source, /function canDropColumn/);
  assert.match(source, /function canEditIndexDraft/);
  assert.match(source, /structureCapabilities\.value\.createIndex/);
  assert.match(source, /structureCapabilities\.value\.dropIndex/);
  assert.match(source, /structureCapabilities\.value\.indexInclude/);
  assert.match(source, /structureCapabilities\.value\.indexFilter/);
});

test("structure editor exposes column order controls", () => {
  assert.match(source, /function moveColumn/);
  assert.match(source, /@click="moveColumn\(index, -1\)"/);
  assert.match(source, /@click="moveColumn\(index, 1\)"/);
  assert.match(source, /t\(['"]structureEditor\.moveColumnUp['"]\)/);
  assert.match(source, /t\(['"]structureEditor\.moveColumnDown['"]\)/);
});

test("structure editor uses a dense wide layout for large tables", () => {
  assert.match(source, /grid-cols-\[minmax\(0,1fr\)_300px\]/);
  assert.match(source, /data-structure-density="compact"/);
  assert.match(source, /class="h-6 min-w-28 text-\[11px\]"/);
  assert.match(source, /w-36/);
  assert.match(source, /w-24/);
});
