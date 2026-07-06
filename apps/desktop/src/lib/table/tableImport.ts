export const IMPORT_SKIP_TARGET = "";

export interface ImportColumnMappingLike {
  sourceColumn: string;
  targetColumn: string;
}

export interface ImportMappingValidationResult {
  valid: boolean;
  errors: string[];
  duplicateTargets: string[];
}

export interface ImportTargetColumnLike {
  name: string;
  is_nullable?: boolean;
  column_default?: string | null;
  extra?: string | null;
  is_primary_key?: boolean;
}

export type TableImportWizardStep = "source" | "options" | "mapping" | "review" | "execution";

export const TABLE_IMPORT_WIZARD_STEPS: TableImportWizardStep[] = ["source", "options", "mapping", "review", "execution"];

export function normalizeImportColumnName(name: string): string {
  return name.trim().toLowerCase().replace(/[_-]+/g, " ").replace(/\s+/g, " ");
}

export function autoMapImportColumns(sourceColumns: string[], targetColumns: string[]): Record<string, string> {
  const exactTargets = new Map(targetColumns.map((column) => [column, column]));
  const normalizedTargets = new Map(targetColumns.map((column) => [normalizeImportColumnName(column), column]));

  return Object.fromEntries(sourceColumns.map((source) => [source, exactTargets.get(source) ?? normalizedTargets.get(normalizeImportColumnName(source)) ?? IMPORT_SKIP_TARGET]));
}

export function validateImportMappings(mappings: ImportColumnMappingLike[]): ImportMappingValidationResult {
  const activeMappings = mappings.filter((mapping) => mapping.targetColumn.trim());
  const errors: string[] = [];
  const duplicateTargets: string[] = [];
  if (activeMappings.length === 0) {
    errors.push("No columns mapped for import");
  }

  const seen = new Set<string>();
  for (const mapping of activeMappings) {
    const key = mapping.targetColumn.trim().toLowerCase();
    if (seen.has(key) && !duplicateTargets.includes(mapping.targetColumn)) {
      duplicateTargets.push(mapping.targetColumn);
    }
    seen.add(key);
  }
  if (duplicateTargets.length) {
    errors.push(`Target column mapped more than once: ${duplicateTargets.join(", ")}`);
  }

  return { valid: errors.length === 0, errors, duplicateTargets };
}

export function requiredImportTargetColumns(columns: ImportTargetColumnLike[], mappedTargetColumns: string[]): string[] {
  const mapped = new Set(mappedTargetColumns.map((column) => column.toLowerCase()));
  return columns
    .filter((column) => !mapped.has(column.name.toLowerCase()))
    .filter(
      (column) =>
        column.is_nullable === false &&
        !column.column_default &&
        !String(column.extra || "")
          .toLowerCase()
          .includes("auto"),
    )
    .map((column) => column.name);
}

export function nextTableImportWizardStep(step: TableImportWizardStep): TableImportWizardStep {
  const index = TABLE_IMPORT_WIZARD_STEPS.indexOf(step);
  return TABLE_IMPORT_WIZARD_STEPS[Math.min(TABLE_IMPORT_WIZARD_STEPS.length - 1, Math.max(0, index) + 1)];
}

export function previousTableImportWizardStep(step: TableImportWizardStep): TableImportWizardStep {
  const index = TABLE_IMPORT_WIZARD_STEPS.indexOf(step);
  return TABLE_IMPORT_WIZARD_STEPS[Math.max(0, index - 1)];
}
