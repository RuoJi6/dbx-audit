import assert from "node:assert/strict";
import { test } from "vitest";
import { buildMongoInsertDocument, buildMongoUpdateDocument, formatMongoShellLiteral, parseMongoDocumentInputValue } from "../../apps/desktop/src/lib/mongoDocumentValues.ts";

test("parses Mongo shell ISODate literals as extended JSON dates", () => {
  assert.deepEqual(parseMongoDocumentInputValue('ISODate("2026-06-10T13:59:31.287Z")'), {
    $date: "2026-06-10T13:59:31.287Z",
  });
  assert.deepEqual(parseMongoDocumentInputValue('"ISODate(\\"2026-06-10T13:59:31.287Z\\")"'), {
    $date: "2026-06-10T13:59:31.287Z",
  });
});

test("parses legacy Mongo date display values as UTC dates", () => {
  assert.deepEqual(parseMongoDocumentInputValue("2025-08-14 02:25:43.718"), {
    $date: "2025-08-14T02:25:43.718Z",
  });
  assert.equal(parseMongoDocumentInputValue('"2025-08-14 02:25:43.718"'), "2025-08-14 02:25:43.718");
});

test("builds Mongo grid updates with set and unset operators", () => {
  const changes = new Map<number, string | number | boolean | null>([
    [1, "Ada"],
    [2, 'ISODate("2026-06-10T13:59:31.287Z")'],
    [3, null],
  ]);

  assert.deepEqual(buildMongoUpdateDocument(changes, ["_id", "name", "createdAt", "archivedAt"]), {
    $set: {
      name: "Ada",
      createdAt: { $date: "2026-06-10T13:59:31.287Z" },
    },
    $unset: {
      archivedAt: "",
    },
  });
});

test("builds Mongo inserts with parsed date values", () => {
  assert.deepEqual(buildMongoInsertDocument(["ignored", 'new Date("2026-06-10T13:59:31.287Z")'], ["_id", "createdAt"]), {
    createdAt: { $date: "2026-06-10T13:59:31.287Z" },
  });
});

test("formats extended JSON dates as Mongo shell ISODate literals", () => {
  assert.equal(
    formatMongoShellLiteral({
      $set: {
        createdAt: { $date: "2026-06-10T13:59:31.287Z" },
      },
    }),
    '{"$set":{"createdAt":ISODate("2026-06-10T13:59:31.287Z")}}',
  );
});
