import type { AuditFinding, AuditTableEvidence } from "@/lib/tauri";

export type AuditRisk = "high" | "medium" | "low";

export type AuditTableHit = {
  key: string;
  connectionId?: string;
  connectionName?: string;
  dbType?: string;
  sourceType?: string;
  database: string;
  schema?: string;
  table: string;
  columns: string[];
  rowCount: number;
  risk: AuditRisk;
};

type AuditTableIdentity = Pick<AuditFinding | AuditTableEvidence | AuditTableHit, "connectionId" | "connectionName" | "dbType" | "database" | "schema" | "table">;

export function buildAuditTableHits(findings: AuditFinding[], tableResults: AuditTableEvidence[] = []): AuditTableHit[] {
  const byTable = new Map<string, AuditTableHit>();
  for (const finding of findings) {
    const key = auditTableKey(finding);
    const existing =
      byTable.get(key) ||
      ({
        key,
        connectionId: finding.connectionId,
        connectionName: finding.connectionName,
        dbType: finding.dbType,
        sourceType: finding.sourceType,
        database: finding.database,
        schema: finding.schema,
        table: finding.table,
        columns: [],
        rowCount: 0,
        risk: "low",
      } satisfies AuditTableHit);
    if (!existing.columns.includes(finding.column)) existing.columns.push(finding.column);
    existing.rowCount = Math.max(existing.rowCount, auditFindingCount(finding));
    existing.risk = highestAuditRisk(existing.risk, auditRisk(finding.level));
    byTable.set(key, existing);
  }

  for (const hit of byTable.values()) {
    const tableRowCount = matchingTableRowCount(hit, tableResults);
    if (tableRowCount > 0) hit.rowCount = tableRowCount;
  }

  return Array.from(byTable.values());
}

export function auditFindingTableRowCount(findings: AuditFinding[]): number {
  const byTable = new Map<string, number>();
  for (const finding of findings) {
    const key = auditTableKey(finding);
    byTable.set(key, Math.max(byTable.get(key) || 0, auditFindingCount(finding)));
  }
  return Array.from(byTable.values()).reduce((total, count) => total + count, 0);
}

export function auditTableResultRowCount(tableResults: AuditTableEvidence[], findings: AuditFinding[] = []): number {
  const findingCounts = new Map<string, { identity: AuditTableIdentity; count: number }>();
  for (const finding of findings) {
    const key = auditTableKey(finding);
    const existing = findingCounts.get(key);
    findingCounts.set(key, {
      identity: finding,
      count: Math.max(existing?.count || 0, auditFindingCount(finding)),
    });
  }

  let total = 0;
  const consumedFindingKeys = new Set<string>();
  for (const table of tableResults) {
    const matchingFindingKey = matchingFindingCountKey(table, findingCounts);
    if (matchingFindingKey) consumedFindingKeys.add(matchingFindingKey);
    const fallbackCount = matchingFindingKey ? findingCounts.get(matchingFindingKey)?.count || 0 : 0;
    total += Math.max(Number(table.rowCount || 0), fallbackCount);
  }

  for (const [key, item] of findingCounts) {
    if (!consumedFindingKeys.has(key)) total += item.count;
  }

  return total;
}

function matchingTableRowCount(identity: AuditTableIdentity, tableResults: AuditTableEvidence[]): number {
  const strictKey = auditTableKey(identity);
  const strictMatch = tableResults.find((table) => auditTableKey(table) === strictKey);
  if (strictMatch) return Number(strictMatch.rowCount || 0);

  const candidates = tableResults.filter((table) => sameTable(identity, table) && compatibleSource(identity, table));
  return candidates.length === 1 ? Number(candidates[0].rowCount || 0) : 0;
}

function matchingFindingCountKey(table: AuditTableEvidence, findingCounts: Map<string, { identity: AuditTableIdentity; count: number }>): string | undefined {
  const strictKey = auditTableKey(table);
  if (findingCounts.has(strictKey)) return strictKey;

  const candidates = Array.from(findingCounts.entries()).filter(([, item]) => sameTable(table, item.identity) && compatibleSource(table, item.identity));
  return candidates.length === 1 ? candidates[0][0] : undefined;
}

function auditFindingCount(finding: AuditFinding): number {
  return Number(finding.count || finding.samples?.length || 1);
}

function auditTableKey(identity: AuditTableIdentity): string {
  const source = identity.connectionId || identity.connectionName || identity.dbType || "";
  return [source, identity.database, identity.schema || "", identity.table].join("\u001f");
}

function sameTable(left: AuditTableIdentity, right: AuditTableIdentity): boolean {
  return left.database === right.database && (left.schema || "") === (right.schema || "") && left.table === right.table;
}

function compatibleSource(left: AuditTableIdentity, right: AuditTableIdentity): boolean {
  if (left.connectionId && right.connectionId) return left.connectionId === right.connectionId;
  if (left.connectionName && right.connectionName) return left.connectionName === right.connectionName;
  if (!left.connectionId && !right.connectionId && !left.connectionName && !right.connectionName && left.dbType && right.dbType) {
    return left.dbType === right.dbType;
  }
  return true;
}

function auditRisk(level: string): AuditRisk {
  if (level === "high" || level === "medium") return level;
  return "low";
}

function highestAuditRisk(a: AuditRisk, b: AuditRisk): AuditRisk {
  if (a === "high" || b === "high") return "high";
  if (a === "medium" || b === "medium") return "medium";
  return "low";
}
