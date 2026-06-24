import { describe, expect, it } from "vitest";
import { combineDataTypeForDatabase, splitDataType } from "../tableStructureEditorState";

describe("tableStructureEditorState", () => {
  it("keeps mysql unsigned attributes in the editable base type", () => {
    expect(splitDataType("int(11) unsigned")).toEqual({ baseType: "int unsigned", params: "11" });
    expect(splitDataType("bigint(20) unsigned zerofill")).toEqual({
      baseType: "bigint unsigned zerofill",
      params: "20",
    });
  });

  it("combines mysql unsigned type choices with the length field", () => {
    expect(combineDataTypeForDatabase("mysql", "int unsigned", "11")).toBe("int(11) unsigned");
    expect(combineDataTypeForDatabase("mysql", "bigint unsigned zerofill", "20")).toBe("bigint(20) unsigned zerofill");
  });
});
