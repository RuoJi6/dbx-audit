import { describe, expect, it } from "vitest";
import { normalizeEditorSettings } from "@/stores/settingsStore";

describe("normalizeEditorSettings", () => {
  it("enables automatic table aliases by default", () => {
    expect(normalizeEditorSettings({}).autoAliasTables).toBe(true);
  });

  it("preserves disabled automatic table aliases", () => {
    expect(normalizeEditorSettings({ autoAliasTables: false }).autoAliasTables).toBe(false);
  });
});
