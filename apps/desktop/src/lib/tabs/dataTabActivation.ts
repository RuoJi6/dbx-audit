import type { QueryResult, QueryTab } from "@/types/database";

function isErrorResult(result: QueryResult | undefined): boolean {
  return result?.columns.length === 1 && result.columns[0] === "Error";
}

export function canActivateExistingDataTableTab(tab: QueryTab, options: { activateExecuting?: boolean } = {}): boolean {
  if (tab.isExecuting) return options.activateExecuting !== false;
  if (isErrorResult(tab.result)) return false;
  return !!tab.result || !!tab.results?.length;
}
