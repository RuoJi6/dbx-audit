import { strict as assert } from "node:assert";
import { test } from "vitest";
import { auditFindingTableRowCount, auditTableResultRowCount, buildAuditTableHits } from "../../apps/desktop/src/lib/auditAggregation.ts";
import type { AuditFinding, AuditTableEvidence } from "../../apps/desktop/src/lib/tauri.ts";

function finding(overrides: Partial<AuditFinding>): AuditFinding {
  return {
    connectionId: "conn-a",
    connectionName: "primary",
    dbType: "mysql",
    database: "app",
    table: "users",
    column: "phone",
    kind: "phone",
    level: "medium",
    mode: "field-name",
    basis: "field name",
    count: 0,
    samples: [],
    ...overrides,
  };
}

function tableEvidence(overrides: Partial<AuditTableEvidence>): AuditTableEvidence {
  return {
    connectionId: "conn-a",
    connectionName: "primary",
    dbType: "mysql",
    database: "app",
    table: "users",
    rowCount: 0,
    columns: [],
    fields: [],
    rows: [],
    ...overrides,
  };
}

test("uses table evidence row count instead of summing field counts", () => {
  const hits = buildAuditTableHits([finding({ column: "phone", count: 50_000 }), finding({ column: "email", kind: "email", count: 49_000 }), finding({ column: "id_card", kind: "id-card", level: "high", count: 45_000 })], [tableEvidence({ rowCount: 50_000 })]);

  assert.equal(hits.length, 1);
  assert.equal(hits[0].rowCount, 50_000);
  assert.deepEqual(hits[0].columns, ["phone", "email", "id_card"]);
  assert.equal(hits[0].risk, "high");
});

test("falls back to the maximum field count per table", () => {
  const findings = [finding({ column: "phone", count: 50_000 }), finding({ column: "email", kind: "email", count: 49_000 })];

  assert.equal(buildAuditTableHits(findings)[0].rowCount, 50_000);
  assert.equal(auditFindingTableRowCount(findings), 50_000);
});

test("connection row count combines table evidence with unmatched table fallbacks", () => {
  const findings = [finding({ table: "users", column: "phone", count: 50_000 }), finding({ table: "orders", column: "buyer_phone", count: 30_000 })];
  const tables = [tableEvidence({ table: "users", rowCount: 100_000 })];

  assert.equal(auditTableResultRowCount(tables, findings), 130_000);
});

test("does not apply another connection's table evidence to a same-name table", () => {
  const hits = buildAuditTableHits([finding({ connectionId: "conn-a", connectionName: "primary", count: 50_000 })], [tableEvidence({ connectionId: "conn-b", connectionName: "replica", rowCount: 90_000 })]);

  assert.equal(hits[0].rowCount, 50_000);
});
