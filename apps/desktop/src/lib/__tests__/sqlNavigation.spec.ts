import { describe, expect, it } from "vitest";
import { matchTable } from "@/lib/sqlNavigation";

describe("matchTable", () => {
  it("matches schema-qualified table identifiers", () => {
    const table = { schema: "MAAC00", name: "Accounts" };

    expect(matchTable("maac00.accounts", [table])).toBe(table);
  });

  it("does not treat non-schema qualifiers as table matches", () => {
    expect(matchTable("u.users", [{ schema: "public", name: "users" }])).toBeNull();
  });
});
