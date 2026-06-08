export interface CanGoNextDataGridPageOptions {
  hasMore?: boolean;
  rowCount: number;
  pageSize: number;
  pageOffset?: number;
  currentPage?: number;
  totalRowCount?: number;
}

export function canGoNextDataGridPage(options: CanGoNextDataGridPageOptions): boolean {
  if (options.hasMore === true) return true;

  const pageSize = Math.max(1, options.pageSize);
  const totalRowCount = options.totalRowCount;
  if (typeof totalRowCount === "number" && Number.isFinite(totalRowCount) && totalRowCount >= 0) {
    const currentOffset =
      typeof options.pageOffset === "number" && options.pageOffset >= 0
        ? options.pageOffset
        : Math.max(0, (options.currentPage ?? 1) - 1) * pageSize;
    return currentOffset + pageSize < totalRowCount;
  }

  return options.rowCount >= pageSize;
}
