import assert from "node:assert/strict";
import { test } from "vitest";
import { isSchemaAware, usesTreeSchemaMode } from "../../apps/desktop/src/lib/databaseCapabilities.ts";

test("TDengine uses database/catalog tree nodes without a schema layer", () => {
  assert.equal(isSchemaAware("tdengine"), false);
  assert.equal(usesTreeSchemaMode("tdengine"), false);
});
