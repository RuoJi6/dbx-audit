import { describe, expect, it } from "vitest";
import { autoMapImportColumns, nextTableImportWizardStep, previousTableImportWizardStep, requiredImportTargetColumns, validateImportMappings } from "@/lib/table/tableImport";

describe("tableImport", () => {
  it("auto maps exact and normalized column names", () => {
    expect(autoMapImportColumns(["id", "user name", "ignored"], ["id", "user_name"])).toEqual({
      id: "id",
      "user name": "user_name",
      ignored: "",
    });
  });

  it("rejects empty mappings and duplicate target columns", () => {
    expect(validateImportMappings([])).toEqual({
      valid: false,
      errors: ["No columns mapped for import"],
      duplicateTargets: [],
    });

    const result = validateImportMappings([
      { sourceColumn: "a", targetColumn: "name" },
      { sourceColumn: "b", targetColumn: "NAME" },
    ]);

    expect(result.valid).toBe(false);
    expect(result.duplicateTargets).toEqual(["NAME"]);
    expect(result.errors[0]).toContain("Target column mapped more than once");
  });

  it("detects unmapped required target columns", () => {
    expect(
      requiredImportTargetColumns(
        [
          { name: "id", is_nullable: false, column_default: null, extra: "auto_increment" },
          { name: "name", is_nullable: false, column_default: null },
          { name: "created_at", is_nullable: false, column_default: "CURRENT_TIMESTAMP" },
        ],
        ["id"],
      ),
    ).toEqual(["name"]);
  });

  it("moves through wizard steps with bounds", () => {
    expect(nextTableImportWizardStep("source")).toBe("options");
    expect(nextTableImportWizardStep("execution")).toBe("execution");
    expect(previousTableImportWizardStep("review")).toBe("mapping");
    expect(previousTableImportWizardStep("source")).toBe("source");
  });
});
