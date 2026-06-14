import { describe, expect, it } from "vitest";
import { DBX_ROWID_COLUMN, editablePrimaryKeys, isTableDataEditable } from "@/lib/tableEditing";
import type { ColumnInfo } from "@/types/database";

function column(name: string, isPrimaryKey = false): ColumnInfo {
  return {
    name,
    data_type: "varchar",
    is_nullable: true,
    column_default: null,
    is_primary_key: isPrimaryKey,
    extra: null,
  };
}

describe("tableEditing", () => {
  it("does not synthesize Oracle ROWID for views", () => {
    expect(editablePrimaryKeys("oracle", [column("ID"), column("NAME")], "VIEW")).toEqual([]);
    expect(editablePrimaryKeys("oracle", [column("ID"), column("NAME")], "TABLE")).toEqual([DBX_ROWID_COLUMN]);
  });

  it("treats view data tabs as readonly", () => {
    expect(isTableDataEditable("oracle", [DBX_ROWID_COLUMN], "VIEW")).toBe(false);
  });
});
