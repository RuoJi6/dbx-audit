<script setup lang="ts">
import { computed, onUnmounted, ref, watch } from "vue";
import { useI18n } from "vue-i18n";
import {
  ArrowLeft,
  Clipboard,
  Database,
  Download,
  FileJson,
  FileSpreadsheet,
  FolderOpen,
  ListChecks,
  Play,
  Plus,
  RefreshCw,
  Search,
  ShieldCheck,
  Square,
  Trash2,
  Upload,
} from "@lucide/vue";
import { Button } from "@/components/ui/button";
import DatabaseIcon from "@/components/icons/DatabaseIcon.vue";
import { Input } from "@/components/ui/input";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import * as api from "@/lib/api";
import { connectionIconType } from "@/lib/connectionPresentation";
import { safeLocalStorageGet, safeLocalStorageRemove } from "@/lib/safeStorage";
import { isTauriRuntime } from "@/lib/tauriRuntime";
import type { AuditFinding, AuditJobState, AuditLevelFilter, AuditMode, ParsedFscanTarget } from "@/lib/tauri";
import type { ConnectionConfig } from "@/types/database";

type AuditTaskKind = "single" | "fscan" | "sql";
type AuditTaskStatus = "draft" | "running" | "completed" | "failed" | "cancelled";
type DetailTab = "hits" | "fields" | "targets" | "sql" | "samples" | "logs";
type WizardStep = 1 | 2 | 3 | 4;

type AuditTask = {
  id: string;
  name: string;
  description: string;
  kind: AuditTaskKind;
  status: AuditTaskStatus;
  progress: number;
  message: string;
  connectionId: string;
  connectionIds: string[];
  database: string;
  schema: string;
  tables: string;
  sql: string;
  proxy: string;
  mode: AuditMode;
  level: AuditLevelFilter;
  limit: number;
  workers: number;
  timeoutSecs: number;
  mask: boolean;
  includeSystem: boolean;
  splitOutput: boolean;
  textEncoding: string;
  outputPath: string;
  fscanText: string;
  targets: ParsedFscanTarget[];
  jobId?: string;
  job?: AuditJobState;
  errors: string[];
  createdAt: string;
  updatedAt: string;
  startedAt?: string;
  finishedAt?: string;
};

type RiskTotals = {
  high: number;
  medium: number;
  low: number;
};

type TableHit = {
  key: string;
  connectionId?: string;
  connectionName?: string;
  dbType?: string;
  database: string;
  schema?: string;
  table: string;
  columns: string[];
  rowCount: number;
  risk: "high" | "medium" | "low";
};

type FieldHit = {
  key: string;
  connectionId?: string;
  connectionName?: string;
  dbType?: string;
  database: string;
  schema?: string;
  table: string;
  column: string;
  kind: string;
  level: "high" | "medium" | "low";
  count: number;
  samples: string[];
};

type SampleGroup = {
  key: string;
  connectionId?: string;
  connectionName?: string;
  dbType?: string;
  database: string;
  table: string;
  fields: FieldHit[];
  rows: Record<string, string>[];
};

const props = defineProps<{
  connections: ConnectionConfig[];
}>();

const { locale } = useI18n();

const AUDIT_STORE_LOCATION = "dbx.db / app_settings.audit_task_store";
const LEGACY_TASK_STORE_KEY = "dbx-audit-tasks-v1";
const LEGACY_LAST_DRAFT_KEY = "dbx-audit-task-draft-v1";
const pollTimers = new Map<string, ReturnType<typeof setInterval>>();
const wizardSteps: WizardStep[] = [1, 2, 3, 4];
const taskKinds: AuditTaskKind[] = ["single", "fscan", "sql"];

const zh = {
  title: "安全审计",
  subtitle: "任务工作台、敏感字段扫描、样例取证与报告导出",
  overview: "任务工作台",
  overviewHeadline: "新建任务后再设置扫描目标，结果进入任务详情查看。",
  newTask: "新建任务",
  newTaskName: "新建审计任务",
  dataManager: "数据管理",
  dataManagerTitle: "审计数据管理",
  dataManagerHint: "任务、配置和扫描结果保存到 DBX 官方 dbx.db。可在这里导出或导入 JSON 备份。",
  storageKey: "存储 Key",
  backupData: "备份数据",
  exportBackup: "导出备份",
  importBackup: "导入备份",
  clearTasks: "清空任务",
  close: "关闭",
  search: "搜索任务 / 数据库 / 表",
  allTasks: "全部任务",
  running: "运行中",
  completed: "已完成",
  failed: "失败",
  taskList: "任务列表",
  noTasks: "暂无任务，点击新建任务开始配置审计目标。",
  noDescription: "未填写任务详情",
  notStarted: "未开始",
  queued: "审计扫描已加入任务队列",
  scanning: "扫描中",
  scanningConnection: "正在扫描 {name}",
  startScanConnection: "开始扫描连接：{name}",
  batchScanFailed: "多连接扫描存在失败项",
  batchScanCompleted: "多连接扫描已完成",
  scanCompletedMessage: "审计扫描已完成",
  connectionTestPassed: "连接测试成功",
  testedConnections: "已测试 {count} 个连接",
  cancelExport: "已取消导出",
  cancelImport: "已取消导入",
  exportedTasks: "已导出 {count} 个任务到 {path}",
  downloadedBackup: "已下载 {count} 个任务的 JSON 备份",
  importedTasks: "已导入 {count} 个任务",
  clearedTasks: "已清空 dbx.db 中的审计任务",
  exportFailed: "导出失败：{error}",
  importFailed: "导入失败：{error}",
  invalidBackup: "备份内容中没有 tasks 数组",
  nameRequired: "{name}不能为空",
  connectionRequired: "请选择连接",
  sqlRequired: "请填写 SQL 内容",
  targetRequired: "请选择至少一个连接或填写目标",
  limitRequired: "样例数量必须大于 0",
  runTaskFirst: "请先运行任务",
  jobNotFound: "未找到扫描任务：{id}",
  copiedTaskInfo: "已复制任务信息",
  copyFailed: "复制失败：{error}",
  backList: "返回任务列表",
  copy: "复制",
  config: "配置",
  detail: "详情",
  start: "开始",
  stop: "停止",
  refresh: "刷新",
  delete: "删除",
  save: "保存任务",
  next: "下一步",
  previous: "上一步",
  taskInfo: "任务信息",
  taskType: "任务类型",
  target: "扫描目标",
  params: "扫描参数",
  name: "任务名称",
  description: "任务描述",
  statusLabel: "状态",
  single: "单目标扫描",
  fscan: "多目标加载",
  sql: "SQL 结果扫描",
  connection: "连接",
  database: "数据库",
  schema: "Schema / 模式",
  tables: "表",
  proxy: "代理",
  sqlText: "SQL 内容",
  mode: "扫描模式",
  level: "风险级别",
  limit: "样例数量",
  workers: "并发数",
  timeout: "超时秒数",
  encoding: "内容编码",
  output: "输出路径",
  mask: "报告中脱敏样例",
  includeSystem: "包含系统库",
  splitOutput: "按目标拆分输出",
  fscanText: "fscan 内容",
  parse: "解析",
  parsed: "已解析 {count} 个目标",
  testConnection: "测试连接",
  hits: "命中结果",
  fields: "高危字段",
  targets: "批量目标",
  samples: "样例数据",
  logs: "日志输出",
  sqlResult: "SQL 结果",
  table: "表",
  databaseType: "数据库类型",
  connectionSource: "连接来源",
  column: "字段 / Key",
  kind: "类型",
  risk: "风险",
  basis: "依据",
  rows: "存在行数",
  noFindings: "暂无扫描结果",
  noSamples: "暂无样例数据",
  noTargets: "暂无批量目标",
  json: "JSON",
  xlsx: "XLSX",
  openFolder: "打开目录",
  chooseOutput: "选择输出文件",
  chooseOutputUnavailable: "Web 开发模式无法选择本地输出路径，请在桌面 App 中使用或手动填写路径。",
  exportReport: "报告导出",
  fieldSearch: "phone / id_card / token",
  contentSearch: "手机号 / 邮箱 / token 值",
  riskAll: "全部",
  riskHigh: "高危",
  riskMedium: "中危",
  riskLow: "低危",
  riskHighShort: "高",
  riskMediumShort: "中",
  riskLowShort: "低",
  targetSinglePrefix: "单目标扫描",
  targetMultiPrefix: "多目标加载",
  targetSqlPrefix: "SQL 结果扫描",
  dbxConnectionCount: "{count} 个 DBX 连接",
  importedTargetCount: "{count} 个导入目标",
  noConnection: "未选择连接",
  proxyPrefix: "代理",
  direct: "直连",
  kindDescription: {
    single: "复用 DBX 连接扫描一个库或指定表。",
    fscan: "从 DBX 连接、fscan 文本或手动列表加载多个目标。",
    sql: "执行 SQL 后识别结果集敏感数据。",
  },
  multiConnection: "连接（可多选）",
  selectedConnectionHint: "已选择 {count} 个连接；启动后会逐个扫描并汇总结果。",
  databasePlaceholderLong: "audit_demo；留空时枚举可访问数据库",
  schemaPlaceholderLong: "public / dbo / 留空",
  tablesPlaceholderLong: "users, orders；留空扫描全部表",
  hitTables: "命中表",
  sensitiveFields: "敏感字段",
  highRiskHits: "高危命中",
  currentProgress: "当前进度",
  createdAt: "创建时间",
  updatedAt: "更新时间",
  fieldCount: "{count} 个字段",
  fieldDetail: "字段详情",
  sampleValues: "样例值",
  noFieldSamplesHint: "当前扫描结果没有返回样例值；内容扫描接入后会在这里显示脱敏样例。",
  rowHits: "命中行数 {count}",
  type: "类型",
  address: "地址",
  usernameLabel: "账号",
  source: "来源",
  sqlPlaceholder: "SQL 结果扫描任务会在这里展示原始 SQL 和结果集敏感命中。",
  matchSearch: "高敏感 / 密码 / token / 邮箱",
  sampleCount: "{count} 条样例",
  sampleSummary: "{groups} 个分组 / {rows} 条样例行",
  databaseName: "数据库名",
  tableName: "表名",
  sampleRows: "样例行",
  noSampleRowsHint: "当前任务只有字段命中结果，没有返回内容样例行；后端内容扫描接入后会在这里显示真实样例数据。",
  fieldEvidence: "字段证据",
  openFolderUnavailable: "Web 开发模式无法直接打开本地目录，请在桌面 App 中使用或手动打开：{path}",
  openedFolder: "已打开目录：{path}",
  status: {
    draft: "草稿",
    running: "运行中",
    completed: "已完成",
    failed: "失败",
    cancelled: "已取消",
  },
  modeLabel: {
    "field-name": "字段名",
    "field-content": "字段名+内容",
    content: "内容",
    all: "全部",
  },
  levelLabel: {
    all: "全部",
    high: "高",
    medium: "中",
    low: "低",
  },
  kindLabel: {
    phone: "手机号",
    email: "邮箱",
    "id-card": "身份证",
    "bank-card": "银行卡",
    "password-secret": "密码/密钥",
    "token-secret": "令牌/Token",
    address: "地址",
    username: "用户名",
    account: "账号",
  },
};

const en = {
  ...zh,
  title: "Security Audit",
  subtitle: "Task workspace, sensitive field discovery, evidence samples, and reports",
  overview: "Audit Workspace",
  overviewHeadline: "Create a task, configure scan targets, then inspect results in task detail.",
  newTask: "New Task",
  newTaskName: "New audit task",
  dataManager: "Data Manager",
  dataManagerTitle: "Audit Data Manager",
  dataManagerHint: "Tasks, configuration, and scan results are saved in DBX's official dbx.db. Export or import JSON backups here.",
  storageKey: "Storage key",
  backupData: "Backup data",
  exportBackup: "Export backup",
  importBackup: "Import backup",
  clearTasks: "Clear tasks",
  close: "Close",
  search: "Search tasks / database / table",
  allTasks: "All tasks",
  running: "Running",
  completed: "Completed",
  failed: "Failed",
  taskList: "Task list",
  noTasks: "No tasks yet. Create a task to configure audit targets.",
  noDescription: "No task description",
  notStarted: "Not started",
  queued: "Audit scan queued",
  scanning: "Scanning",
  scanningConnection: "Scanning {name}",
  startScanConnection: "Starting connection scan: {name}",
  batchScanFailed: "Some connection scans failed",
  batchScanCompleted: "Multi-connection scan completed",
  scanCompletedMessage: "Audit scan completed",
  connectionTestPassed: "Connection test passed",
  testedConnections: "Tested {count} connections",
  cancelExport: "Export cancelled",
  cancelImport: "Import cancelled",
  exportedTasks: "Exported {count} tasks to {path}",
  downloadedBackup: "Downloaded JSON backup for {count} tasks",
  importedTasks: "Imported {count} tasks",
  clearedTasks: "Cleared audit tasks from dbx.db",
  exportFailed: "Export failed: {error}",
  importFailed: "Import failed: {error}",
  invalidBackup: "Backup content does not include a tasks array",
  nameRequired: "{name} is required",
  connectionRequired: "Please select a connection",
  sqlRequired: "Please enter SQL",
  targetRequired: "Please select at least one connection or enter targets",
  limitRequired: "Sample limit must be greater than 0",
  runTaskFirst: "Run the task first",
  jobNotFound: "Scan job not found: {id}",
  copiedTaskInfo: "Task info copied",
  copyFailed: "Copy failed: {error}",
  backList: "Back to tasks",
  copy: "Copy",
  config: "Configure",
  detail: "Detail",
  start: "Start",
  stop: "Stop",
  refresh: "Refresh",
  delete: "Delete",
  save: "Save task",
  next: "Next",
  previous: "Previous",
  taskInfo: "Task info",
  taskType: "Task type",
  target: "Target",
  params: "Scan parameters",
  name: "Task name",
  description: "Description",
  statusLabel: "Status",
  single: "Single target",
  fscan: "Multi-target load",
  sql: "SQL result scan",
  connection: "Connection",
  database: "Database",
  schema: "Schema",
  tables: "Tables",
  proxy: "Proxy",
  sqlText: "SQL",
  mode: "Mode",
  level: "Risk level",
  limit: "Sample limit",
  workers: "Workers",
  timeout: "Timeout seconds",
  encoding: "Encoding",
  output: "Output path",
  mask: "Mask samples in report",
  includeSystem: "Include system databases",
  splitOutput: "Split output by target",
  fscanText: "fscan text",
  parse: "Parse",
  parsed: "Parsed {count} targets",
  testConnection: "Test connection",
  hits: "Hits",
  fields: "Risk fields",
  targets: "Batch targets",
  samples: "Samples",
  logs: "Logs",
  sqlResult: "SQL result",
  table: "Table",
  databaseType: "Database type",
  connectionSource: "Connection",
  column: "Column / Key",
  kind: "Kind",
  risk: "Risk",
  basis: "Basis",
  rows: "Rows",
  noFindings: "No findings yet",
  noSamples: "No samples yet",
  noTargets: "No batch targets",
  exportReport: "Export report",
  chooseOutput: "Choose output file",
  chooseOutputUnavailable: "Web dev mode cannot choose local output paths. Use the desktop app or enter a path manually.",
  fieldSearch: "phone / id_card / token",
  contentSearch: "phone / email / token value",
  riskHighShort: "High",
  riskMediumShort: "Med",
  riskLowShort: "Low",
  targetSinglePrefix: "Single target scan",
  targetMultiPrefix: "Multi-target load",
  targetSqlPrefix: "SQL result scan",
  dbxConnectionCount: "{count} DBX connections",
  importedTargetCount: "{count} imported targets",
  noConnection: "No connection selected",
  proxyPrefix: "Proxy",
  direct: "Direct",
  kindDescription: {
    single: "Scan one database or selected tables through a DBX connection.",
    fscan: "Load multiple targets from DBX connections, fscan text, or a manual list.",
    sql: "Run SQL and detect sensitive data in the result set.",
  },
  multiConnection: "Connections (multi-select)",
  selectedConnectionHint: "{count} connections selected; scans run one by one and are merged into this task.",
  databasePlaceholderLong: "audit_demo; empty scans accessible databases",
  schemaPlaceholderLong: "public / dbo / empty",
  tablesPlaceholderLong: "users, orders; empty scans all tables",
  hitTables: "Hit tables",
  sensitiveFields: "Sensitive fields",
  highRiskHits: "High-risk hits",
  currentProgress: "Current progress",
  createdAt: "Created",
  updatedAt: "Updated",
  fieldCount: "{count} fields",
  fieldDetail: "Field detail",
  sampleValues: "Sample values",
  noFieldSamplesHint: "This scan result did not return sample values. Masked samples will appear here after content scanning is connected.",
  rowHits: "{count} row hits",
  type: "Type",
  address: "Address",
  usernameLabel: "Account",
  source: "Source",
  sqlPlaceholder: "SQL result scan tasks show the original SQL and sensitive result-set hits here.",
  matchSearch: "high risk / password / token / email",
  sampleCount: "{count} samples",
  sampleSummary: "{groups} groups / {rows} sample rows",
  databaseName: "Database",
  tableName: "Table",
  sampleRows: "Sample rows",
  noSampleRowsHint: "This task only has field findings and did not return content sample rows. Real samples will appear here after backend content scanning is connected.",
  fieldEvidence: "Field evidence",
  openFolderUnavailable: "Web dev mode cannot open local folders directly. Use the desktop app or open manually: {path}",
  openedFolder: "Opened folder: {path}",
  status: {
    draft: "Draft",
    running: "Running",
    completed: "Completed",
    failed: "Failed",
    cancelled: "Cancelled",
  },
  modeLabel: {
    "field-name": "Field name",
    "field-content": "Field + content",
    content: "Content",
    all: "All",
  },
  levelLabel: {
    all: "All",
    high: "High",
    medium: "Medium",
    low: "Low",
  },
  kindLabel: {
    phone: "Phone",
    email: "Email",
    "id-card": "ID card",
    "bank-card": "Bank card",
    "password-secret": "Password/secret",
    "token-secret": "Token/secret",
    address: "Address",
    username: "Username",
    account: "Account",
  },
};

const ui = computed(() => (String(locale.value).toLowerCase().startsWith("zh") ? zh : en));
const view = ref<"overview" | "wizard" | "detail">("overview");
const wizardStep = ref<WizardStep>(1);
const activeTab = ref<DetailTab>("hits");
const searchQuery = ref("");
const fieldQuery = ref("");
const sampleQuery = ref("");
const matchQuery = ref("");
const riskFilter = ref<"all" | "high" | "medium" | "low">("all");
const selectedTaskId = ref("");
const draft = ref<AuditTask>(newTask());
const tasks = ref<AuditTask[]>([]);
const exportMessage = ref("");
const error = ref("");
const showDataManager = ref(false);
const dataManagerMessage = ref("");
const selectedFieldKey = ref("");
const auditStoreLoaded = ref(false);
let auditStoreSaveTimer: ReturnType<typeof setTimeout> | undefined;

const selectedTask = computed(() => tasks.value.find((task) => task.id === selectedTaskId.value) || null);
const selectedConnection = computed(() => props.connections.find((connection) => connection.id === draft.value.connectionId));
const selectedConnections = computed(() =>
  props.connections.filter((connection) => activeConnectionIds(draft.value).includes(connection.id)),
);

const filteredTasks = computed(() => {
  const query = searchQuery.value.trim().toLowerCase();
  if (!query) return tasks.value;
  return tasks.value.filter((task) => {
    const haystack = [task.name, task.description, task.database, task.tables, task.kind, task.message, connectionLabel(task)]
      .join(" ")
      .toLowerCase();
    return haystack.includes(query);
  });
});

const stats = computed(() => ({
  all: tasks.value.length,
  running: tasks.value.filter((task) => task.status === "running").length,
  completed: tasks.value.filter((task) => task.status === "completed").length,
  failed: tasks.value.filter((task) => task.status === "failed").length,
}));

const detailFindings = computed(() => (selectedTask.value ? findingsWithConnectionMeta(selectedTask.value) : []));
const detailTotals = computed(() => riskTotals(detailFindings.value));
const detailTables = computed(() => tableHits(detailFindings.value));
const detailFields = computed(() => fieldHits(detailFindings.value));
const detailTabs = computed<DetailTab[]>(() => {
  const task = selectedTask.value;
  const tabs: DetailTab[] = ["hits", "fields"];
  if (task?.kind === "fscan") tabs.push("targets");
  if (task?.kind === "sql" || !!task?.sql.trim()) tabs.push("sql");
  tabs.push("samples", "logs");
  return tabs;
});
const filteredTables = computed(() => {
  const query = fieldQuery.value.trim().toLowerCase();
  return detailTables.value.filter((item) => {
    if (riskFilter.value !== "all" && item.risk !== riskFilter.value) return false;
    if (!query) return true;
    return [item.dbType, item.connectionName, item.database, item.table, item.columns.join(" ")]
      .join(" ")
      .toLowerCase()
      .includes(query);
  });
});
const filteredFields = computed(() => {
  return detailFields.value.filter((field) => riskFilter.value === "all" || field.level === riskFilter.value);
});
const sampleGroups = computed(() => buildSampleGroups(detailFindings.value));
const filteredSampleGroups = computed(() => {
  const valueQuery = sampleQuery.value.trim().toLowerCase();
  const sensitiveQuery = matchQuery.value.trim().toLowerCase();
  return sampleGroups.value.filter((group) => {
    const text = JSON.stringify(group).toLowerCase();
    return (!valueQuery || text.includes(valueQuery)) && (!sensitiveQuery || text.includes(sensitiveQuery));
  });
});
const filteredSampleRowCount = computed(() => filteredSampleGroups.value.reduce((total, group) => total + group.rows.length, 0));
const filteredSampleSummary = computed(() =>
  ui.value.sampleSummary
    .replace("{groups}", String(filteredSampleGroups.value.length))
    .replace("{rows}", String(filteredSampleRowCount.value)),
);

watch(
  () => props.connections,
  (connections) => {
    if (!draft.value.connectionId && connections.length) {
      draft.value.connectionId = connections[0].id;
    }
    if (draft.value.connectionIds.length === 0 && draft.value.connectionId) {
      draft.value.connectionIds = [draft.value.connectionId];
    }
  },
  { immediate: true },
);

watch(
  tasks,
  () => scheduleAuditTaskStoreSave(),
  { deep: true },
);

watch(
  draft,
  () => scheduleAuditTaskStoreSave(),
  { deep: true },
);

watch(
  detailTabs,
  (tabs) => {
    if (!tabs.includes(activeTab.value)) activeTab.value = "hits";
  },
  { immediate: true },
);

type AuditTaskStore = {
  app?: string;
  version?: number;
  tasks?: AuditTask[];
  draft?: Partial<AuditTask>;
};

async function loadAuditTaskStore() {
  try {
    const store = (await api.auditLoadTaskStore()) as AuditTaskStore | null;
    let loadedTasks = Array.isArray(store?.tasks) ? store.tasks : [];
    let loadedDraft = store?.draft;

    if (loadedTasks.length === 0 && !loadedDraft) {
      const legacy = loadLegacyAuditTaskStore();
      loadedTasks = legacy.tasks;
      loadedDraft = legacy.draft;
      if (loadedTasks.length || loadedDraft) {
        await saveAuditTaskStore(loadedTasks, loadedDraft);
        safeLocalStorageRemove(LEGACY_TASK_STORE_KEY);
        safeLocalStorageRemove(LEGACY_LAST_DRAFT_KEY);
      }
    }

    tasks.value = normalizeTasks(loadedTasks);
    if (loadedDraft) draft.value = newTask(loadedDraft);
  } catch (err) {
    error.value = String(err);
  } finally {
    auditStoreLoaded.value = true;
  }
}

function loadLegacyAuditTaskStore(): { tasks: AuditTask[]; draft?: Partial<AuditTask> } {
  try {
    const rawTasks = safeLocalStorageGet(LEGACY_TASK_STORE_KEY);
    const rawDraft = safeLocalStorageGet(LEGACY_LAST_DRAFT_KEY);
    return {
      tasks: rawTasks ? (JSON.parse(rawTasks) as AuditTask[]) : [],
      draft: rawDraft ? (JSON.parse(rawDraft) as Partial<AuditTask>) : undefined,
    };
  } catch {
    return { tasks: [] };
  }
}

function normalizeTasks(value: AuditTask[]) {
  return value.map((task) => normalizeTask({ ...newTask(), ...task, targets: task.targets || [], errors: task.errors || [] }));
}

function scheduleAuditTaskStoreSave() {
  if (!auditStoreLoaded.value) return;
  if (auditStoreSaveTimer) clearTimeout(auditStoreSaveTimer);
  auditStoreSaveTimer = setTimeout(() => {
    void saveAuditTaskStore(tasks.value, draft.value);
  }, 250);
}

async function saveAuditTaskStore(nextTasks: AuditTask[], nextDraft?: Partial<AuditTask>) {
  try {
    await api.auditSaveTaskStore({
      app: "dbx-audit",
      version: 1,
      storage: AUDIT_STORE_LOCATION,
      updatedAt: new Date().toISOString(),
      tasks: nextTasks,
      draft: nextDraft,
    });
  } catch (err) {
    error.value = String(err);
  }
}

function newTask(seed?: Partial<AuditTask>): AuditTask {
  const now = new Date().toISOString();
  const fallbackConnectionId = seed?.connectionId || props.connections[0]?.id || "";
  return {
    id: seed?.id || `audit-${Date.now()}-${Math.random().toString(16).slice(2)}`,
    name: seed?.name || ui.value.newTaskName,
    description: seed?.description || "",
    kind: seed?.kind || "single",
    status: seed?.status || "draft",
    progress: seed?.progress || 0,
    message: seed?.message || ui.value.notStarted,
    connectionId: fallbackConnectionId,
    connectionIds: seed?.connectionIds?.length ? seed.connectionIds : fallbackConnectionId ? [fallbackConnectionId] : [],
    database: seed?.database || "",
    schema: seed?.schema || "",
    tables: seed?.tables || "",
    sql: seed?.sql || "",
    proxy: seed?.proxy || "",
    mode: seed?.mode || "field-content",
    level: seed?.level || "all",
    limit: seed?.limit || 15,
    workers: seed?.workers || 1,
    timeoutSecs: seed?.timeoutSecs || 15,
    mask: seed?.mask ?? false,
    includeSystem: seed?.includeSystem || false,
    splitOutput: seed?.splitOutput || false,
    textEncoding: seed?.textEncoding || "auto",
    outputPath: seed?.outputPath || "/tmp/dbx-audit-report.xlsx",
    fscanText: seed?.fscanText || "",
    targets: seed?.targets || [],
    jobId: seed?.jobId,
    job: seed?.job,
    errors: seed?.errors || [],
    createdAt: seed?.createdAt || now,
    updatedAt: now,
    startedAt: seed?.startedAt,
    finishedAt: seed?.finishedAt,
  };
}

function normalizeTask(task: AuditTask) {
  const connectionIds = task.connectionIds?.length ? task.connectionIds : task.connectionId ? [task.connectionId] : [];
  return {
    ...task,
    connectionId: task.connectionId || connectionIds[0] || "",
    connectionIds,
  };
}

void loadAuditTaskStore();

function persistTask(task: AuditTask) {
  const next = { ...task, updatedAt: new Date().toISOString() };
  const index = tasks.value.findIndex((item) => item.id === task.id);
  if (index >= 0) tasks.value.splice(index, 1, next);
  else tasks.value.unshift(next);
  selectedTaskId.value = next.id;
  return next;
}

function createTask() {
  error.value = "";
  wizardStep.value = 1;
  draft.value = newTask(draft.value);
  draft.value.id = `audit-${Date.now()}-${Math.random().toString(16).slice(2)}`;
  draft.value.status = "draft";
  draft.value.progress = 0;
  draft.value.mask = false;
  draft.value.job = undefined;
  draft.value.jobId = undefined;
  view.value = "wizard";
}

function configureTask(task: AuditTask) {
  error.value = "";
  draft.value = newTask(task);
  wizardStep.value = 1;
  view.value = "wizard";
}

function saveDraft() {
  const validation = validateTask(draft.value);
  if (validation) {
    error.value = validation;
    return;
  }
  error.value = "";
  const saved = persistTask({ ...draft.value, status: draft.value.status === "running" ? "draft" : draft.value.status });
  selectedTaskId.value = saved.id;
  view.value = "detail";
}

function removeTask(task: AuditTask) {
  stopPolling(task.id);
  tasks.value = tasks.value.filter((item) => item.id !== task.id);
  if (selectedTaskId.value === task.id) {
    selectedTaskId.value = "";
    view.value = "overview";
  }
}

function toggleDataManager() {
  showDataManager.value = !showDataManager.value;
  dataManagerMessage.value = "";
}

function buildBackupJson() {
  return JSON.stringify(
    {
      app: "dbx-audit",
      version: 1,
      exportedAt: new Date().toISOString(),
      storageKey: AUDIT_STORE_LOCATION,
      tasks: tasks.value,
    },
    null,
    2,
  );
}

async function exportTaskBackup() {
  dataManagerMessage.value = "";
  error.value = "";
  try {
    const content = buildBackupJson();
    const fileName = `dbx-audit-backup-${new Date().toISOString().slice(0, 10)}.json`;
    if (isTauriRuntime()) {
      const [{ save }, { writeTextFile }] = await Promise.all([
        import("@tauri-apps/plugin-dialog"),
        import("@tauri-apps/plugin-fs"),
      ]);
      const path = await save({
        defaultPath: fileName,
        filters: [{ name: "JSON", extensions: ["json"] }],
      });
      if (!path) {
        dataManagerMessage.value = ui.value.cancelExport;
        return;
      }
      await writeTextFile(path, content);
      dataManagerMessage.value = ui.value.exportedTasks
        .replace("{count}", String(tasks.value.length))
        .replace("{path}", path);
      return;
    }
    downloadBackupFile(fileName, content);
    dataManagerMessage.value = ui.value.downloadedBackup.replace("{count}", String(tasks.value.length));
  } catch (err) {
    error.value = ui.value.exportFailed.replace("{error}", String(err));
  }
}

async function importTaskBackup() {
  dataManagerMessage.value = "";
  error.value = "";
  try {
    const backupText = await readBackupFile();
    if (!backupText?.trim()) {
      dataManagerMessage.value = ui.value.cancelImport;
      return;
    }
    const parsed = JSON.parse(backupText) as { tasks?: AuditTask[] } | AuditTask[];
    const imported = Array.isArray(parsed) ? parsed : parsed.tasks;
    if (!Array.isArray(imported)) {
      throw new Error(ui.value.invalidBackup);
    }
    tasks.value = imported.map((task) => ({ ...newTask(), ...task, targets: task.targets || [], errors: task.errors || [] }));
    selectedTaskId.value = "";
    dataManagerMessage.value = ui.value.importedTasks.replace("{count}", String(tasks.value.length));
  } catch (err) {
    error.value = ui.value.importFailed.replace("{error}", String(err));
  }
}

function downloadBackupFile(fileName: string, content: string) {
  const blob = new Blob([content], { type: "application/json;charset=utf-8" });
  const url = URL.createObjectURL(blob);
  const link = document.createElement("a");
  link.href = url;
  link.download = fileName;
  document.body.appendChild(link);
  link.click();
  link.remove();
  URL.revokeObjectURL(url);
}

async function readBackupFile() {
  if (isTauriRuntime()) {
    const [{ open }, { readTextFile }] = await Promise.all([
      import("@tauri-apps/plugin-dialog"),
      import("@tauri-apps/plugin-fs"),
    ]);
    const selected = await open({
      multiple: false,
      filters: [{ name: "JSON", extensions: ["json"] }],
    });
    const path = Array.isArray(selected) ? selected[0] : selected;
    return path ? await readTextFile(path) : "";
  }
  const file = await new Promise<File | null>((resolve) => {
    const input = document.createElement("input");
    input.type = "file";
    input.accept = ".json,application/json";
    input.onchange = () => resolve(input.files?.[0] ?? null);
    input.click();
  });
  return file ? await file.text() : "";
}

function clearAuditTasks() {
  for (const taskId of pollTimers.keys()) stopPolling(taskId);
  tasks.value = [];
  selectedTaskId.value = "";
  dataManagerMessage.value = ui.value.clearedTasks;
}

function viewTask(task: AuditTask, tab: DetailTab = "hits") {
  selectedTaskId.value = task.id;
  activeTab.value = tab;
  view.value = "detail";
}

function validateTask(task: AuditTask) {
  if (!task.name.trim()) return ui.value.nameRequired.replace("{name}", ui.value.name);
  if (activeConnectionIds(task).length === 0) return ui.value.connectionRequired;
  if (task.kind === "sql" && !task.sql.trim()) return ui.value.sqlRequired;
  if (task.kind === "fscan" && activeConnectionIds(task).length === 0 && !task.fscanText.trim() && task.targets.length === 0) {
    return ui.value.targetRequired;
  }
  if (task.limit < 1) return ui.value.limitRequired;
  return "";
}

async function parseFscanForDraft() {
  error.value = "";
  try {
    const parsed = await api.auditParseFscan(draft.value.fscanText);
    draft.value.targets = parsed.targets;
    draft.value.message = ui.value.parsed.replace("{count}", String(parsed.total));
  } catch (err) {
    error.value = String(err);
  }
}

async function testCurrentConnection() {
  const connections = draft.value.kind === "fscan" ? selectedConnections.value : selectedConnection.value ? [selectedConnection.value] : [];
  if (connections.length === 0) {
    error.value = ui.value.connectionRequired;
    return;
  }
  error.value = "";
  try {
    for (const connection of connections) {
      await api.testConnection(connection);
    }
    draft.value.message = connections.length > 1
      ? ui.value.testedConnections.replace("{count}", String(connections.length))
      : ui.value.connectionTestPassed;
  } catch (err) {
    error.value = String(err);
  }
}

function setWizardStep(step: WizardStep) {
  wizardStep.value = step;
}

function nextWizardStep() {
  if (wizardStep.value < 4) wizardStep.value = (wizardStep.value + 1) as WizardStep;
}

function previousWizardStep() {
  if (wizardStep.value > 1) wizardStep.value = (wizardStep.value - 1) as WizardStep;
}

function setTaskKind(kind: AuditTaskKind) {
  draft.value.kind = kind;
  if (kind === "fscan" && draft.value.connectionIds.length === 0 && draft.value.connectionId) {
    draft.value.connectionIds = [draft.value.connectionId];
  }
  if (kind !== "fscan" && !draft.value.connectionId) {
    draft.value.connectionId = draft.value.connectionIds[0] || props.connections[0]?.id || "";
  }
}

function selectField(field: FieldHit) {
  selectedFieldKey.value = field.key;
}

function activeConnectionIds(task: Pick<AuditTask, "kind" | "connectionId" | "connectionIds">) {
  if (task.kind === "fscan") {
    return task.connectionIds?.length ? task.connectionIds : task.connectionId ? [task.connectionId] : [];
  }
  return task.connectionId ? [task.connectionId] : [];
}

function isDraftConnectionSelected(connectionId: string) {
  return activeConnectionIds(draft.value).includes(connectionId);
}

function toggleDraftConnection(connectionId: string) {
  const selected = new Set(draft.value.connectionIds);
  if (selected.has(connectionId)) selected.delete(connectionId);
  else selected.add(connectionId);
  draft.value.connectionIds = Array.from(selected);
  draft.value.connectionId = draft.value.connectionIds[0] || "";
}

async function startTask(task: AuditTask) {
  const validation = validateTask(task);
  if (validation) {
    error.value = validation;
    return;
  }
  stopPolling(task.id);
  error.value = "";
  const running = persistTask({
    ...task,
    status: "running",
    progress: 5,
    message: ui.value.queued,
    errors: [],
    startedAt: new Date().toISOString(),
    finishedAt: undefined,
  });
  try {
    const connectionIds = activeConnectionIds(running);
    if (running.kind === "fscan" && connectionIds.length > 1) {
      await startConnectionBatchTask(running, connectionIds);
      return;
    }
    const jobId = await api.auditStartScan({
      connectionId: connectionIds[0] || running.connectionId,
      database: running.database.trim() || undefined,
      schema: running.schema.trim() || undefined,
      tables: splitList(running.tables),
      mode: running.mode,
      level: running.level,
      limit: Number(running.limit) || 15,
      mask: running.mask,
      workers: Number(running.workers) || 1,
      timeoutSecs: Number(running.timeoutSecs) || 15,
    });
    persistTask({ ...running, jobId, message: ui.value.scanning });
    await refreshTaskJob(running.id, jobId);
    const timer = setInterval(() => void refreshTaskJob(running.id, jobId), 1000);
    pollTimers.set(running.id, timer);
  } catch (err) {
    persistTask({
      ...running,
      status: "failed",
      progress: 0,
      message: String(err),
      errors: [String(err)],
      finishedAt: new Date().toISOString(),
    });
  }
}

async function startConnectionBatchTask(task: AuditTask, connectionIds: string[]) {
  const startedAt = new Date().toISOString();
  const aggregate: AuditJobState = {
    jobId: `batch-${task.id}`,
    status: "running",
    progress: 0,
    request: {
      connectionId: connectionIds[0] || task.connectionId,
      database: task.database.trim() || undefined,
      schema: task.schema.trim() || undefined,
      tables: splitList(task.tables),
      mode: task.mode,
      level: task.level,
      limit: Number(task.limit) || 15,
      mask: task.mask,
      workers: Number(task.workers) || 1,
      timeoutSecs: Number(task.timeoutSecs) || 15,
    },
    logs: [],
    findings: [],
    errors: [],
    startedAt,
  };

  for (const [index, connectionId] of connectionIds.entries()) {
    const connection = props.connections.find((item) => item.id === connectionId);
    const label = connection?.name || connectionId;
    try {
      aggregate.logs.push({ time: new Date().toLocaleTimeString(), level: "info", message: ui.value.startScanConnection.replace("{name}", label) });
      persistTask({
        ...task,
        job: { ...aggregate, progress: Math.round((index / connectionIds.length) * 100) },
        status: "running",
        progress: Math.round((index / connectionIds.length) * 100),
        message: ui.value.scanningConnection.replace("{name}", label),
      });
      const jobId = await api.auditStartScan({
        ...aggregate.request,
        connectionId,
      });
      const job = await waitForAuditJob(jobId, task.id, label, index, connectionIds.length);
      aggregate.findings.push(
        ...(job.findings || []).map((finding) => ({
          ...finding,
          connectionId,
          connectionName: label,
          dbType: connection?.db_type,
        })),
      );
      aggregate.logs.push(...(job.logs || []).map((entry) => ({ ...entry, message: `${label}: ${entry.message}` })));
      aggregate.errors.push(...(job.errors || []).map((entry) => `${label}: ${entry}`));
    } catch (err) {
      aggregate.errors.push(`${label}: ${String(err)}`);
      aggregate.logs.push({ time: new Date().toLocaleTimeString(), level: "error", message: `${label}: ${String(err)}` });
    }
  }

  aggregate.status = aggregate.errors.length ? "failed" : "completed";
  aggregate.progress = 100;
  aggregate.finishedAt = new Date().toISOString();
  persistTask({
    ...task,
    jobId: aggregate.jobId,
    job: aggregate,
    status: aggregate.errors.length ? "failed" : "completed",
    progress: 100,
    message: aggregate.errors.length ? ui.value.batchScanFailed : ui.value.batchScanCompleted,
    errors: aggregate.errors,
    finishedAt: aggregate.finishedAt,
  });
}

async function waitForAuditJob(jobId: string, taskId: string, label: string, index: number, total: number) {
  for (;;) {
    const job = await api.auditGetJob(jobId);
    if (!job) throw new Error(ui.value.jobNotFound.replace("{id}", jobId));
    const baseProgress = (index / total) * 100;
    const currentProgress = Math.min(99, Math.round(baseProgress + (job.progress / total)));
    const task = tasks.value.find((item) => item.id === taskId);
    if (task) {
      persistTask({
        ...task,
        jobId,
        job,
        status: "running",
        progress: currentProgress,
        message: ui.value.scanningConnection.replace("{name}", label),
      });
    }
    if (job.status !== "running") return job;
    await new Promise((resolve) => setTimeout(resolve, 1000));
  }
}

async function stopTask(task: AuditTask) {
  if (!task.jobId) return;
  await api.auditCancelScan(task.jobId);
  await refreshTaskJob(task.id, task.jobId);
}

async function refreshTaskJob(taskId: string, jobId?: string) {
  const task = tasks.value.find((item) => item.id === taskId);
  const effectiveJobId = jobId || task?.jobId;
  if (!task || !effectiveJobId) return;
  try {
    const job = await api.auditGetJob(effectiveJobId);
    if (!job) return;
    const status = mapJobStatus(job.status);
    const next = persistTask({
      ...task,
      jobId: effectiveJobId,
      job,
      status,
      progress: job.progress,
      message: lastLog(job) || ui.value.status[status],
      errors: job.errors || [],
      finishedAt: job.finishedAt || task.finishedAt,
    });
    if (job.status !== "running") {
      stopPolling(next.id);
    }
  } catch (err) {
    persistTask({ ...task, status: "failed", message: String(err), errors: [String(err)] });
    stopPolling(task.id);
  }
}

async function exportReport(task: AuditTask, format: "json" | "xlsx") {
  if (!task.jobId) {
    error.value = ui.value.runTaskFirst;
    return;
  }
  error.value = "";
  exportMessage.value = "";
  try {
    const base = task.outputPath.trim() || `/tmp/${task.name.replace(/\s+/g, "-")}.${format}`;
    const path = format === "json" ? base.replace(/\.xlsx$/i, ".json") : base.replace(/\.json$/i, ".xlsx");
    const result = await api.auditExportReport(task.jobId, format, path);
    exportMessage.value = `${ui.value.exportReport}: ${result.path}`;
  } catch (err) {
    error.value = String(err);
  }
}

async function chooseOutputPath() {
  error.value = "";
  if (!isTauriRuntime()) {
    error.value = ui.value.chooseOutputUnavailable;
    return;
  }
  try {
    const { save } = await import("@tauri-apps/plugin-dialog");
    const path = await save({
      defaultPath: draft.value.outputPath.trim() || "/tmp/dbx-audit-report.xlsx",
      filters: [
        { name: "Audit report", extensions: ["xlsx", "json"] },
        { name: "Excel", extensions: ["xlsx"] },
        { name: "JSON", extensions: ["json"] },
      ],
    });
    if (path) draft.value.outputPath = path;
  } catch (err) {
    error.value = String(err);
  }
}

async function openOutputDirectory(task: AuditTask) {
  const directory = outputDirectory(task.outputPath || "/tmp/dbx-audit-report.xlsx");
  exportMessage.value = "";
  error.value = "";
  if (!isTauriRuntime()) {
    exportMessage.value = ui.value.openFolderUnavailable.replace("{path}", directory);
    return;
  }
  try {
    await api.auditOpenOutputDirectory(directory);
    exportMessage.value = ui.value.openedFolder.replace("{path}", directory);
  } catch (err) {
    error.value = String(err);
  }
}

function outputDirectory(path: string) {
  const normalized = path.trim().replace(/\\/g, "/");
  if (!normalized) return "/tmp";
  if (normalized.endsWith("/")) return normalized;
  const index = normalized.lastIndexOf("/");
  return index <= 0 ? "." : normalized.slice(0, index);
}

function stopPolling(taskId: string) {
  const timer = pollTimers.get(taskId);
  if (timer) clearInterval(timer);
  pollTimers.delete(taskId);
}

function splitList(text: string) {
  return text
    .split(/[,;\n]/)
    .map((item) => item.trim())
    .filter(Boolean);
}

function mapJobStatus(status: AuditJobState["status"]): AuditTaskStatus {
  if (status === "running") return "running";
  if (status === "completed") return "completed";
  if (status === "cancelled") return "cancelled";
  return "failed";
}

function lastLog(job?: AuditJobState) {
  return job?.logs?.[job.logs.length - 1]?.message || "";
}

function connectionFor(task: AuditTask) {
  return props.connections.find((connection) => connection.id === activeConnectionIds(task)[0]);
}

function draftConnectionIcon() {
  return connectionIconType(selectedConnection.value);
}

function taskConnectionIcon(task: AuditTask) {
  return connectionIconType(connectionFor(task));
}

function connectionLabel(task: AuditTask) {
  const connectionIds = activeConnectionIds(task);
  if (task.kind === "fscan" && connectionIds.length > 1) {
    const types = connectionTypesLabel(task);
    const count = ui.value.dbxConnectionCount.replace("{count}", String(connectionIds.length));
    return types ? `${count} · ${types}` : count;
  }
  const connection = connectionFor(task);
  if (!connection) return ui.value.noConnection;
  return `${connection.db_type}://${connection.host || "localhost"}:${connection.port || ""}`;
}

function targetSummary(task: AuditTask) {
  if (task.kind === "fscan") {
    const parsedTargets = task.targets.length || splitList(task.fscanText).length;
    return `${ui.value.targetMultiPrefix} · ${connectionLabel(task)}${
      parsedTargets ? ` · ${ui.value.importedTargetCount.replace("{count}", String(parsedTargets))}` : ""
    }`;
  }
  if (task.kind === "sql") return `${ui.value.targetSqlPrefix} · ${connectionLabel(task)}`;
  return `${ui.value.targetSinglePrefix} · ${connectionLabel(task)}`;
}

function connectionTypesLabel(task: Pick<AuditTask, "kind" | "connectionId" | "connectionIds">) {
  const types = activeConnectionIds(task)
    .map((connectionId) => props.connections.find((connection) => connection.id === connectionId)?.db_type)
    .filter(Boolean)
    .map(String);
  return Array.from(new Set(types)).join(" / ");
}

function localizedTaskMessage(task: AuditTask) {
  const message = task.message || "";
  if (!message) return ui.value.status[task.status];
  const map: Record<string, string> = {
    未开始: ui.value.notStarted,
    审计扫描已加入任务队列: ui.value.queued,
    扫描中: ui.value.scanning,
    多连接扫描存在失败项: ui.value.batchScanFailed,
    多连接扫描已完成: ui.value.batchScanCompleted,
    审计扫描已完成: ui.value.scanCompletedMessage,
  };
  return map[message] || message;
}

function riskTotals(findings: AuditFinding[]): RiskTotals {
  return findings.reduce(
    (totals, finding) => {
      if (finding.level === "high") totals.high += 1;
      else if (finding.level === "medium") totals.medium += 1;
      else totals.low += 1;
      return totals;
    },
    { high: 0, medium: 0, low: 0 },
  );
}

function taskTotals(task: AuditTask) {
  return riskTotals(task.job?.findings || []);
}

function findingsWithConnectionMeta(task: AuditTask): AuditFinding[] {
  const fallbackConnection = connectionFor(task);
  return (task.job?.findings || []).map((finding) => ({
    ...finding,
    connectionId: finding.connectionId || fallbackConnection?.id,
    connectionName: finding.connectionName || fallbackConnection?.name,
    dbType: finding.dbType || fallbackConnection?.db_type,
  }));
}

function tableHits(findings: AuditFinding[]): TableHit[] {
  const byTable = new Map<string, TableHit>();
  for (const finding of findings) {
    const key = `${finding.connectionId || finding.dbType || ""}/${finding.database}/${finding.schema || ""}/${finding.table}`;
    const existing =
      byTable.get(key) ||
      ({
        key,
        connectionId: finding.connectionId,
        connectionName: finding.connectionName,
        dbType: finding.dbType,
        database: finding.database,
        schema: finding.schema,
        table: finding.table,
        columns: [],
        rowCount: 0,
        risk: "low",
      } satisfies TableHit);
    if (!existing.columns.includes(finding.column)) existing.columns.push(finding.column);
    existing.rowCount += Number(finding.count || finding.samples?.length || 1);
    existing.risk = highestRisk(existing.risk, finding.level as "high" | "medium" | "low");
    byTable.set(key, existing);
  }
  return Array.from(byTable.values());
}

function fieldHits(findings: AuditFinding[]): FieldHit[] {
  return findings.map((finding, index) => ({
    key: `${finding.connectionId || finding.dbType || ""}/${finding.database}/${finding.table}/${finding.column}/${index}`,
    connectionId: finding.connectionId,
    connectionName: finding.connectionName,
    dbType: finding.dbType,
    database: finding.database,
    schema: finding.schema,
    table: finding.table,
    column: finding.column,
    kind: finding.kind,
    level: finding.level as "high" | "medium" | "low",
    count: Number(finding.count || finding.samples?.length || 1),
    samples: (finding.samples || []).map((sample) => sample.value),
}));
}

function buildSampleGroups(findings: AuditFinding[]) {
  const groups = new Map<string, SampleGroup>();
  for (const field of fieldHits(findings)) {
    const key = `${field.connectionId || field.dbType || ""}/${field.database}/${field.table}`;
    const group =
      groups.get(key) ||
      ({
        key,
        connectionId: field.connectionId,
        connectionName: field.connectionName,
        dbType: field.dbType,
        database: field.database,
        table: field.table,
        fields: [],
        rows: [],
      } satisfies SampleGroup);
    group.fields.push(field);
    field.samples.forEach((value, index) => {
      group.rows[index] = { ...group.rows[index], [field.column]: value };
    });
    groups.set(key, group);
  }
  return Array.from(groups.values());
}

function databaseScopeText(item: Pick<TableHit | FieldHit | SampleGroup, "connectionName" | "dbType">) {
  return [item.dbType, item.connectionName].filter(Boolean).join(" · ");
}

function highestRisk(a: "high" | "medium" | "low", b: "high" | "medium" | "low") {
  if (a === "high" || b === "high") return "high";
  if (a === "medium" || b === "medium") return "medium";
  return "low";
}

function riskClass(risk: string) {
  if (risk === "high") return "bg-red-50 text-red-700 border-red-100 dark:bg-red-950/30 dark:text-red-300";
  if (risk === "medium") return "bg-amber-50 text-amber-700 border-amber-100 dark:bg-amber-950/30 dark:text-amber-300";
  return "bg-emerald-50 text-emerald-700 border-emerald-100 dark:bg-emerald-950/30 dark:text-emerald-300";
}

function riskTextClass(risk: string) {
  if (risk === "high") return "text-red-600 dark:text-red-300";
  if (risk === "medium") return "text-amber-600 dark:text-amber-300";
  return "text-emerald-600 dark:text-emerald-300";
}

function kindName(kind: string) {
  const labels = ui.value.kindLabel as Record<string, string>;
  return labels[kind] || kind;
}

async function copyTask(task: AuditTask) {
  const totals = taskTotals(task);
  const text = [
    `${ui.value.name}: ${task.name}`,
    `${ui.value.statusLabel}: ${ui.value.status[task.status]}`,
    `${ui.value.connection}: ${connectionLabel(task)}`,
    `${ui.value.mode}: ${ui.value.modeLabel[task.mode]}`,
    task.database ? `${ui.value.database}: ${task.database}` : "",
    task.tables ? `${ui.value.tables}: ${task.tables}` : "",
    `${ui.value.riskHigh}: ${totals.high}`,
    `${ui.value.riskMedium}: ${totals.medium}`,
    `${ui.value.riskLow}: ${totals.low}`,
    `${ui.value.currentProgress}: ${task.progress}%`,
    task.outputPath ? `${ui.value.output}: ${task.outputPath}` : "",
  ]
    .filter(Boolean)
    .join("\n");
  try {
    await writeClipboardText(text);
    dataManagerMessage.value = ui.value.copiedTaskInfo;
    exportMessage.value = ui.value.copiedTaskInfo;
    error.value = "";
  } catch (err) {
    error.value = ui.value.copyFailed.replace("{error}", String(err));
  }
}

async function writeClipboardText(text: string) {
  if (isTauriRuntime()) {
    const { writeText } = await import("@tauri-apps/plugin-clipboard-manager");
    await writeText(text);
    return;
  }
  if (!navigator.clipboard?.writeText) {
    throw new Error("Clipboard API is unavailable");
  }
  await navigator.clipboard.writeText(text);
}

function formatTime(value?: string) {
  if (!value) return "-";
  return new Date(value).toLocaleString();
}

onUnmounted(() => {
  for (const taskId of pollTimers.keys()) stopPolling(taskId);
  if (auditStoreSaveTimer) {
    clearTimeout(auditStoreSaveTimer);
    if (auditStoreLoaded.value) void saveAuditTaskStore(tasks.value, draft.value);
  }
});
</script>

<template>
  <section class="flex h-full min-h-0 flex-col bg-background">
    <header class="flex h-12 shrink-0 items-center justify-between border-b px-4">
      <div class="flex items-center gap-2">
        <ShieldCheck class="h-4 w-4 text-primary" />
        <h2 class="text-sm font-semibold">{{ ui.title }}</h2>
        <span class="text-xs text-muted-foreground">{{ ui.subtitle }}</span>
      </div>
      <div class="flex items-center gap-2">
        <Button v-if="view !== 'overview'" variant="outline" size="sm" class="gap-1" @click="view = 'overview'">
          <ArrowLeft class="h-3.5 w-3.5" />
          {{ ui.backList }}
        </Button>
        <Button size="sm" class="gap-1" @click="createTask">
          <Plus class="h-3.5 w-3.5" />
          {{ ui.newTask }}
        </Button>
      </div>
    </header>

    <main v-if="view === 'overview'" class="min-h-0 flex-1 overflow-auto bg-muted/20 p-5">
      <div class="mb-4 rounded-md border bg-background p-5">
        <div class="flex flex-wrap items-center justify-between gap-3">
          <div>
            <div class="text-xs font-medium text-primary">{{ ui.overview }}</div>
            <h3 class="mt-1 text-xl font-semibold">{{ ui.overviewHeadline }}</h3>
          </div>
          <div class="flex gap-2">
            <Button variant="outline" class="gap-1" :class="showDataManager ? 'border-primary text-primary' : ''" @click="toggleDataManager">
              <Database class="h-4 w-4" />
              {{ ui.dataManager }}
            </Button>
            <Button class="gap-1" @click="createTask">
              <Plus class="h-4 w-4" />
              {{ ui.newTask }}
            </Button>
          </div>
        </div>
      </div>

      <div v-if="showDataManager" class="mb-4 rounded-md border bg-background p-4">
        <div class="flex flex-wrap items-start justify-between gap-3">
          <div>
            <div class="text-sm font-semibold">{{ ui.dataManagerTitle }}</div>
            <p class="mt-1 text-xs text-muted-foreground">{{ ui.dataManagerHint }}</p>
          </div>
          <Button variant="outline" size="sm" @click="showDataManager = false">{{ ui.close }}</Button>
        </div>
        <div class="mt-4 grid gap-4 lg:grid-cols-[260px_minmax(0,1fr)]">
          <div class="rounded-md border bg-muted/20 p-3 text-xs">
            <div class="text-muted-foreground">{{ ui.storageKey }}</div>
            <div class="mt-1 font-mono">{{ AUDIT_STORE_LOCATION }}</div>
            <div class="mt-3 text-muted-foreground">{{ ui.allTasks }}</div>
            <div class="mt-1 text-2xl font-semibold">{{ tasks.length }}</div>
          </div>
          <div class="grid gap-2 sm:grid-cols-3">
            <Button variant="outline" class="justify-start gap-2" @click="exportTaskBackup">
              <Download class="h-4 w-4" />
              {{ ui.exportBackup }}
            </Button>
            <Button variant="outline" class="justify-start gap-2" @click="importTaskBackup">
              <Upload class="h-4 w-4" />
              {{ ui.importBackup }}
            </Button>
            <Button variant="outline" class="justify-start gap-2 text-destructive" @click="clearAuditTasks">
              <Trash2 class="h-4 w-4" />
              {{ ui.clearTasks }}
            </Button>
          </div>
        </div>
        <p v-if="dataManagerMessage" class="mt-3 text-xs text-muted-foreground">{{ dataManagerMessage }}</p>
        <p v-if="error" class="mt-3 text-xs text-destructive">{{ error }}</p>
      </div>

      <div class="mb-4 grid gap-3 md:grid-cols-4">
        <div class="rounded-md border bg-background p-4">
          <div class="text-xs text-muted-foreground">{{ ui.allTasks }}</div>
          <div class="mt-2 text-3xl font-semibold">{{ stats.all }}</div>
        </div>
        <div class="rounded-md border bg-background p-4">
          <div class="text-xs text-muted-foreground">{{ ui.running }}</div>
          <div class="mt-2 text-3xl font-semibold">{{ stats.running }}</div>
        </div>
        <div class="rounded-md border bg-background p-4">
          <div class="text-xs text-muted-foreground">{{ ui.completed }}</div>
          <div class="mt-2 text-3xl font-semibold">{{ stats.completed }}</div>
        </div>
        <div class="rounded-md border bg-background p-4">
          <div class="text-xs text-muted-foreground">{{ ui.failed }}</div>
          <div class="mt-2 text-3xl font-semibold">{{ stats.failed }}</div>
        </div>
      </div>
      <p v-if="dataManagerMessage && !showDataManager" class="mb-3 text-xs text-muted-foreground">{{ dataManagerMessage }}</p>

      <div class="rounded-md border bg-background">
        <div class="flex flex-wrap items-center justify-between gap-3 border-b px-4 py-3">
          <div class="text-sm font-semibold">{{ ui.taskList }}</div>
          <Input v-model="searchQuery" class="h-8 w-72" :placeholder="ui.search" />
        </div>
        <div v-if="filteredTasks.length === 0" class="p-10 text-center text-sm text-muted-foreground">
          {{ ui.noTasks }}
        </div>
        <div v-for="task in filteredTasks" :key="task.id" class="grid gap-3 border-b p-4 lg:grid-cols-[1fr_160px_1.4fr_auto]">
          <div>
            <button class="text-left text-base font-semibold hover:text-primary" @click="viewTask(task)">
              {{ task.name }}
            </button>
            <div class="mt-1 text-xs text-muted-foreground">{{ task.description || ui.noDescription }}</div>
          </div>
          <div>
            <span class="rounded-full px-3 py-1 text-xs" :class="task.status === 'completed' ? 'bg-emerald-50 text-emerald-700' : task.status === 'running' ? 'bg-blue-50 text-blue-700' : task.status === 'failed' ? 'bg-red-50 text-red-700' : 'bg-muted text-muted-foreground'">
              {{ ui.status[task.status] }}
            </span>
          </div>
          <div class="space-y-1 text-xs text-muted-foreground">
            <div class="flex items-center gap-1.5">
              <DatabaseIcon :db-type="taskConnectionIcon(task)" class="h-3.5 w-3.5 shrink-0" />
              <span>{{ targetSummary(task) }}</span>
            </div>
            <div v-if="task.proxy">{{ ui.proxyPrefix }} {{ task.proxy }}</div>
            <div class="flex flex-wrap items-center gap-2">
              <span class="rounded-full border px-2 py-0.5" :class="riskClass('high')">{{ ui.riskHighShort }} {{ taskTotals(task).high }}</span>
              <span class="rounded-full border px-2 py-0.5" :class="riskClass('medium')">{{ ui.riskMediumShort }} {{ taskTotals(task).medium }}</span>
              <span class="rounded-full border px-2 py-0.5" :class="riskClass('low')">{{ ui.riskLowShort }} {{ taskTotals(task).low }}</span>
            </div>
            <div class="h-1.5 overflow-hidden rounded bg-muted">
              <div class="h-full bg-primary" :style="{ width: `${task.progress}%` }" />
            </div>
          </div>
          <div class="flex flex-wrap items-center gap-2">
            <Button variant="outline" size="sm" @click="copyTask(task)">{{ ui.copy }}</Button>
            <Button variant="outline" size="sm" @click="configureTask(task)">{{ ui.config }}</Button>
            <Button variant="outline" size="sm" @click="viewTask(task)">{{ ui.detail }}</Button>
            <Button v-if="task.status !== 'running'" size="sm" @click="startTask(task)">{{ ui.start }}</Button>
            <Button v-else variant="outline" size="sm" @click="stopTask(task)">{{ ui.stop }}</Button>
            <Button variant="outline" size="sm" class="gap-1 text-destructive" @click="removeTask(task)">
              <Trash2 class="h-3.5 w-3.5" />
              {{ ui.delete }}
            </Button>
          </div>
        </div>
      </div>
    </main>

    <main v-else-if="view === 'wizard'" class="min-h-0 flex-1 overflow-auto bg-muted/20 p-5">
      <div class="mx-auto max-w-5xl rounded-md border bg-background">
        <div class="border-b p-4">
          <div class="flex items-center gap-2 text-sm font-semibold">
            <ListChecks class="h-4 w-4" />
            {{ ui.newTask }}
          </div>
          <div class="mt-3 grid gap-2 sm:grid-cols-4">
            <button v-for="step in wizardSteps" :key="step" class="rounded-md border px-3 py-2 text-left text-xs" :class="wizardStep === step ? 'border-primary bg-primary/5 text-primary' : 'text-muted-foreground'" @click="setWizardStep(step)">
              {{ step }}. {{ [ui.taskInfo, ui.taskType, ui.target, ui.params][step - 1] }}
            </button>
          </div>
        </div>

        <div class="space-y-4 p-4">
          <div v-if="wizardStep === 1" class="grid gap-4 md:grid-cols-2">
            <label class="space-y-1 text-xs font-medium">
              {{ ui.name }}
              <Input v-model="draft.name" class="h-9" />
            </label>
            <label class="space-y-1 text-xs font-medium">
              {{ ui.description }}
              <Input v-model="draft.description" class="h-9" :placeholder="ui.noDescription" />
            </label>
          </div>

          <div v-else-if="wizardStep === 2" class="grid gap-3 md:grid-cols-3">
            <button v-for="kind in taskKinds" :key="kind" class="rounded-md border p-4 text-left" :class="draft.kind === kind ? 'border-primary bg-primary/5' : ''" @click="setTaskKind(kind)">
              <div class="font-semibold">{{ kind === 'single' ? ui.single : kind === 'fscan' ? ui.fscan : ui.sql }}</div>
              <div class="mt-2 text-xs text-muted-foreground">
                {{ kind === 'single' ? ui.kindDescription.single : kind === 'fscan' ? ui.kindDescription.fscan : ui.kindDescription.sql }}
              </div>
            </button>
          </div>

          <div v-else-if="wizardStep === 3" class="grid gap-4 md:grid-cols-2">
            <div v-if="draft.kind === 'fscan'" class="space-y-2 md:col-span-2">
              <div class="text-xs font-medium">{{ ui.multiConnection }}</div>
              <div class="grid gap-2 md:grid-cols-2 xl:grid-cols-3">
                <button
                  v-for="connection in props.connections"
                  :key="connection.id"
                  class="flex items-center gap-2 rounded-md border p-3 text-left text-xs transition hover:border-primary"
                  :class="isDraftConnectionSelected(connection.id) ? 'border-primary bg-primary/5' : 'bg-background'"
                  @click="toggleDraftConnection(connection.id)"
                >
                  <input
                    class="pointer-events-none"
                    type="checkbox"
                    :checked="isDraftConnectionSelected(connection.id)"
                    tabindex="-1"
                  />
                  <DatabaseIcon :db-type="connectionIconType(connection)" class="h-4 w-4 shrink-0" />
                  <span class="min-w-0">
                    <span class="block truncate font-medium">{{ connection.name }}</span>
                    <span class="block truncate text-muted-foreground">{{ connection.db_type }} · {{ connection.host }}:{{ connection.port }}</span>
                  </span>
                </button>
              </div>
              <div class="text-xs text-muted-foreground">{{ ui.selectedConnectionHint.replace("{count}", String(activeConnectionIds(draft).length)) }}</div>
            </div>
            <label v-else class="space-y-1 text-xs font-medium">
              {{ ui.connection }}
              <Select v-model="draft.connectionId">
                <SelectTrigger class="h-9">
                  <div class="flex min-w-0 items-center gap-2">
                    <DatabaseIcon v-if="draft.connectionId" :db-type="draftConnectionIcon()" class="h-3.5 w-3.5 shrink-0" />
                    <SelectValue />
                  </div>
                </SelectTrigger>
                <SelectContent>
                  <SelectItem v-for="connection in props.connections" :key="connection.id" :value="connection.id">
                    <div class="flex items-center gap-2">
                      <DatabaseIcon :db-type="connectionIconType(connection)" class="h-3.5 w-3.5 shrink-0" />
                      <span>{{ connection.name }}</span>
                    </div>
                  </SelectItem>
                </SelectContent>
              </Select>
            </label>
            <label class="space-y-1 text-xs font-medium">
              {{ ui.database }}
              <Input v-model="draft.database" class="h-9" :placeholder="ui.databasePlaceholderLong" />
            </label>
            <label class="space-y-1 text-xs font-medium">
              {{ ui.schema }}
              <Input v-model="draft.schema" class="h-9" :placeholder="ui.schemaPlaceholderLong" />
            </label>
            <label class="space-y-1 text-xs font-medium">
              {{ ui.tables }}
              <Input v-model="draft.tables" class="h-9" :placeholder="ui.tablesPlaceholderLong" />
            </label>
            <label class="space-y-1 text-xs font-medium">
              {{ ui.proxy }}
              <Input v-model="draft.proxy" class="h-9" placeholder="direct / socks5://127.0.0.1:1080" />
            </label>
            <div class="flex items-end">
              <Button variant="outline" class="gap-1" @click="testCurrentConnection">
                <RefreshCw class="h-3.5 w-3.5" />
                {{ ui.testConnection }}
              </Button>
            </div>
            <label v-if="draft.kind === 'sql'" class="space-y-1 text-xs font-medium md:col-span-2">
              {{ ui.sqlText }}
              <textarea v-model="draft.sql" class="h-32 w-full resize-none rounded-md border bg-background p-2 font-mono text-xs" placeholder="select * from users limit 100" />
            </label>
            <div v-if="draft.kind === 'fscan'" class="space-y-2 md:col-span-2">
              <label class="block space-y-1 text-xs font-medium">
                {{ ui.fscanText }}
                <textarea v-model="draft.fscanText" class="h-32 w-full resize-none rounded-md border bg-background p-2 font-mono text-xs" />
              </label>
              <div class="flex items-center gap-2">
                <Button variant="outline" size="sm" class="gap-1" @click="parseFscanForDraft">
                  <Search class="h-3.5 w-3.5" />
                  {{ ui.parse }}
                </Button>
                <span class="text-xs text-muted-foreground">{{ ui.parsed.replace("{count}", String(draft.targets.length)) }}</span>
              </div>
            </div>
          </div>

          <div v-else class="grid gap-4 md:grid-cols-3">
            <label class="space-y-1 text-xs font-medium">
              {{ ui.mode }}
              <Select v-model="draft.mode">
                <SelectTrigger class="h-9"><SelectValue /></SelectTrigger>
                <SelectContent>
                  <SelectItem value="field-name">{{ ui.modeLabel["field-name"] }}</SelectItem>
                  <SelectItem value="field-content">{{ ui.modeLabel["field-content"] }}</SelectItem>
                  <SelectItem value="content">{{ ui.modeLabel.content }}</SelectItem>
                  <SelectItem value="all">{{ ui.modeLabel.all }}</SelectItem>
                </SelectContent>
              </Select>
            </label>
            <label class="space-y-1 text-xs font-medium">
              {{ ui.level }}
              <Select v-model="draft.level">
                <SelectTrigger class="h-9"><SelectValue /></SelectTrigger>
                <SelectContent>
                  <SelectItem value="all">{{ ui.levelLabel.all }}</SelectItem>
                  <SelectItem value="high">{{ ui.levelLabel.high }}</SelectItem>
                  <SelectItem value="medium">{{ ui.levelLabel.medium }}</SelectItem>
                  <SelectItem value="low">{{ ui.levelLabel.low }}</SelectItem>
                </SelectContent>
              </Select>
            </label>
            <label class="space-y-1 text-xs font-medium">
              {{ ui.limit }}
              <Input v-model.number="draft.limit" class="h-9" min="1" type="number" />
            </label>
            <label class="space-y-1 text-xs font-medium">
              {{ ui.workers }}
              <Input v-model.number="draft.workers" class="h-9" min="1" type="number" />
            </label>
            <label class="space-y-1 text-xs font-medium">
              {{ ui.timeout }}
              <Input v-model.number="draft.timeoutSecs" class="h-9" min="1" type="number" />
            </label>
            <label class="space-y-1 text-xs font-medium">
              {{ ui.encoding }}
              <Select v-model="draft.textEncoding">
                <SelectTrigger class="h-9"><SelectValue /></SelectTrigger>
                <SelectContent>
                  <SelectItem value="auto">auto</SelectItem>
                  <SelectItem value="utf-8">utf-8</SelectItem>
                  <SelectItem value="gbk">gbk</SelectItem>
                </SelectContent>
              </Select>
            </label>
            <label class="space-y-1 text-xs font-medium md:col-span-3">
              {{ ui.output }}
              <div class="flex gap-2">
                <Input v-model="draft.outputPath" class="h-9 min-w-0" />
                <Button
                  type="button"
                  variant="outline"
                  class="h-9 w-9 shrink-0 p-0"
                  :title="ui.chooseOutput"
                  :aria-label="ui.chooseOutput"
                  @click="chooseOutputPath"
                >
                  <FolderOpen class="h-4 w-4" />
                </Button>
              </div>
            </label>
            <label class="flex items-center gap-2 text-xs"><input v-model="draft.mask" type="checkbox" />{{ ui.mask }}</label>
            <label class="flex items-center gap-2 text-xs"><input v-model="draft.includeSystem" type="checkbox" />{{ ui.includeSystem }}</label>
            <label class="flex items-center gap-2 text-xs"><input v-model="draft.splitOutput" type="checkbox" />{{ ui.splitOutput }}</label>
          </div>

          <p v-if="error" class="text-xs text-destructive">{{ error }}</p>
        </div>

        <div class="flex justify-between border-t p-4">
          <Button variant="outline" :disabled="wizardStep === 1" @click="previousWizardStep">{{ ui.previous }}</Button>
          <div class="flex gap-2">
            <Button v-if="wizardStep < 4" @click="nextWizardStep">{{ ui.next }}</Button>
            <Button v-else @click="saveDraft">{{ ui.save }}</Button>
          </div>
        </div>
      </div>
    </main>

    <main v-else-if="selectedTask" class="min-h-0 flex-1 overflow-auto bg-muted/20 p-5">
      <div class="mb-4 rounded-md border bg-background p-4">
        <div class="flex flex-wrap items-center justify-between gap-3">
          <div>
            <div class="text-xs text-muted-foreground">{{ targetSummary(selectedTask) }} · {{ ui.status[selectedTask.status] }}</div>
            <h3 class="text-lg font-semibold">{{ selectedTask.name }}</h3>
            <div class="text-xs text-muted-foreground">{{ selectedTask.description || ui.noDescription }}</div>
          </div>
          <div class="flex flex-wrap gap-2">
            <Button variant="outline" size="sm" class="gap-1" @click="copyTask(selectedTask)"><Clipboard class="h-3.5 w-3.5" />{{ ui.copy }}</Button>
            <Button variant="outline" size="sm" @click="configureTask(selectedTask)">{{ ui.config }}</Button>
            <Button v-if="selectedTask.status !== 'running'" size="sm" class="gap-1" @click="startTask(selectedTask)"><Play class="h-3.5 w-3.5" />{{ ui.start }}</Button>
            <Button v-else variant="outline" size="sm" class="gap-1" @click="stopTask(selectedTask)"><Square class="h-3.5 w-3.5" />{{ ui.stop }}</Button>
            <Button variant="outline" size="sm" class="gap-1" @click="refreshTaskJob(selectedTask.id)"><RefreshCw class="h-3.5 w-3.5" />{{ ui.refresh }}</Button>
            <Button variant="outline" size="sm" class="gap-1 text-destructive" @click="removeTask(selectedTask)"><Trash2 class="h-3.5 w-3.5" />{{ ui.delete }}</Button>
          </div>
        </div>
      </div>

      <div class="mb-4 grid gap-4 lg:grid-cols-[1fr_340px]">
        <div class="rounded-md border bg-background p-4">
          <div class="grid gap-4 md:grid-cols-4">
            <div><div class="text-xs text-muted-foreground">{{ ui.hitTables }}</div><div class="mt-1 text-3xl font-semibold">{{ detailTables.length }}</div></div>
            <div><div class="text-xs text-muted-foreground">{{ ui.sensitiveFields }}</div><div class="mt-1 text-3xl font-semibold">{{ detailFields.length }}</div></div>
            <div><div class="text-xs text-muted-foreground">{{ ui.highRiskHits }}</div><div class="mt-1 text-3xl font-semibold">{{ detailTotals.high }}</div></div>
            <div><div class="text-xs text-muted-foreground">{{ ui.currentProgress }}</div><div class="mt-1 text-3xl font-semibold">{{ selectedTask.progress }}%</div></div>
          </div>
          <div class="mt-4 flex flex-wrap gap-2">
            <span class="rounded-full border px-2 py-0.5 text-xs" :class="riskClass('high')">{{ ui.riskHigh }} {{ detailTotals.high }}</span>
            <span class="rounded-full border px-2 py-0.5 text-xs" :class="riskClass('medium')">{{ ui.riskMedium }} {{ detailTotals.medium }}</span>
            <span class="rounded-full border px-2 py-0.5 text-xs" :class="riskClass('low')">{{ ui.riskLow }} {{ detailTotals.low }}</span>
          </div>
          <div class="mt-4 h-2 overflow-hidden rounded bg-muted">
            <div class="h-full bg-primary" :style="{ width: `${selectedTask.progress}%` }" />
          </div>
          <div class="mt-3 flex justify-between text-xs text-muted-foreground">
            <span>{{ localizedTaskMessage(selectedTask) }}</span>
            <span class="flex items-center gap-1.5">
              <DatabaseIcon :db-type="taskConnectionIcon(selectedTask)" class="h-3.5 w-3.5 shrink-0" />
              {{ connectionLabel(selectedTask) }}
            </span>
          </div>
        </div>

        <div class="rounded-md border bg-background p-4">
          <div class="font-semibold">{{ ui.taskInfo }}</div>
          <div class="mt-4 grid grid-cols-2 gap-3 text-xs">
            <div><div class="text-muted-foreground">{{ ui.createdAt }}</div><div class="font-medium">{{ formatTime(selectedTask.createdAt) }}</div></div>
            <div><div class="text-muted-foreground">{{ ui.updatedAt }}</div><div class="font-medium">{{ formatTime(selectedTask.updatedAt) }}</div></div>
            <div><div class="text-muted-foreground">{{ ui.mode }}</div><div class="font-medium">{{ ui.modeLabel[selectedTask.mode] }}</div></div>
            <div><div class="text-muted-foreground">{{ ui.encoding }}</div><div class="font-medium">{{ selectedTask.textEncoding }}</div></div>
            <div><div class="text-muted-foreground">{{ ui.proxy }}</div><div class="font-medium">{{ selectedTask.proxy || ui.direct }}</div></div>
            <div><div class="text-muted-foreground">{{ ui.output }}</div><div class="truncate font-medium">{{ selectedTask.outputPath }}</div></div>
          </div>
          <div class="mt-4 flex flex-wrap gap-2">
            <Button variant="outline" size="sm" class="gap-1" :disabled="!selectedTask.jobId" @click="exportReport(selectedTask, 'json')"><FileJson class="h-3.5 w-3.5" />{{ ui.json }}</Button>
            <Button variant="outline" size="sm" class="gap-1" :disabled="!selectedTask.jobId" @click="exportReport(selectedTask, 'xlsx')"><FileSpreadsheet class="h-3.5 w-3.5" />{{ ui.xlsx }}</Button>
            <Button variant="outline" size="sm" class="gap-1" @click="openOutputDirectory(selectedTask)"><FolderOpen class="h-3.5 w-3.5" />{{ ui.openFolder }}</Button>
          </div>
        </div>
      </div>

      <div class="rounded-md border bg-background">
        <div class="flex flex-wrap gap-1 border-b px-3 pt-3">
          <button v-for="tab in detailTabs" :key="tab" class="rounded-t-md px-4 py-2 text-sm" :class="activeTab === tab ? 'border border-b-background bg-background text-primary' : 'text-muted-foreground'" @click="activeTab = tab">
            {{ tab === 'hits' ? ui.hits : tab === 'fields' ? ui.fields : tab === 'targets' ? ui.targets : tab === 'sql' ? ui.sqlResult : tab === 'samples' ? ui.samples : ui.logs }}
          </button>
        </div>

        <div v-if="activeTab === 'hits'" class="p-4">
          <div class="mb-3 grid gap-3 md:grid-cols-[1fr_180px]">
            <Input v-model="fieldQuery" class="h-9" :placeholder="ui.fieldSearch" />
            <Select v-model="riskFilter">
              <SelectTrigger class="h-9"><SelectValue /></SelectTrigger>
              <SelectContent>
                <SelectItem value="all">{{ ui.riskAll }}</SelectItem>
                <SelectItem value="high">{{ ui.riskHigh }}</SelectItem>
                <SelectItem value="medium">{{ ui.riskMedium }}</SelectItem>
                <SelectItem value="low">{{ ui.riskLow }}</SelectItem>
              </SelectContent>
            </Select>
          </div>
          <table class="w-full text-xs">
            <thead class="bg-muted/60 text-muted-foreground">
              <tr><th class="px-3 py-2 text-left">{{ ui.connectionSource }}</th><th class="px-3 py-2 text-left">{{ ui.database }}</th><th class="px-3 py-2 text-left">{{ ui.table }}</th><th class="px-3 py-2 text-left">{{ ui.sensitiveFields }}</th><th class="px-3 py-2 text-left">{{ ui.rows }}</th><th class="px-3 py-2 text-left">{{ ui.risk }}</th></tr>
            </thead>
            <tbody>
              <tr v-for="hit in filteredTables" :key="hit.key" class="border-t">
                <td class="px-3 py-2">
                  <span class="flex items-center gap-1.5">
                    <DatabaseIcon :db-type="hit.dbType || ''" class="h-3.5 w-3.5 shrink-0" />
                    <span class="truncate">{{ databaseScopeText(hit) || "-" }}</span>
                  </span>
                </td>
                <td class="px-3 py-2 font-mono">{{ hit.database }}</td>
                <td class="px-3 py-2 font-mono">{{ hit.table }}</td>
                <td class="px-3 py-2">{{ hit.columns.join(", ") }}</td>
                <td class="px-3 py-2">{{ hit.rowCount }}</td>
                <td class="px-3 py-2"><span class="rounded-full border px-2 py-0.5" :class="riskClass(hit.risk)">{{ hit.risk }}</span></td>
              </tr>
              <tr v-if="filteredTables.length === 0"><td class="px-3 py-8 text-center text-muted-foreground" colspan="6">{{ ui.noFindings }}</td></tr>
            </tbody>
          </table>
        </div>

        <div v-else-if="activeTab === 'fields'" class="p-4">
          <div class="mb-3 flex items-center justify-between">
            <Select v-model="riskFilter">
              <SelectTrigger class="h-9 w-44"><SelectValue /></SelectTrigger>
              <SelectContent>
                <SelectItem value="all">{{ ui.riskAll }}</SelectItem>
                <SelectItem value="high">{{ ui.riskHigh }}</SelectItem>
                <SelectItem value="medium">{{ ui.riskMedium }}</SelectItem>
                <SelectItem value="low">{{ ui.riskLow }}</SelectItem>
              </SelectContent>
            </Select>
            <span class="text-xs text-muted-foreground">{{ ui.fieldCount.replace('{count}', String(filteredFields.length)) }}</span>
          </div>
          <div class="grid gap-3 md:grid-cols-2 xl:grid-cols-3">
            <div
              v-for="field in filteredFields"
              :key="field.key"
              class="relative rounded-md border p-4 text-left transition hover:border-primary"
              :class="[riskClass(field.level), selectedFieldKey === field.key ? 'ring-2 ring-primary' : '']"
              role="button"
              tabindex="0"
              @click="selectField(field)"
              @keydown.enter="selectField(field)"
            >
              <div class="font-mono text-base font-semibold">{{ field.column }}</div>
              <div class="mt-2 flex items-center gap-1.5 text-xs">
                <DatabaseIcon :db-type="field.dbType || ''" class="h-3.5 w-3.5 shrink-0" />
                <span>{{ databaseScopeText(field) || "-" }}</span>
              </div>
              <div class="mt-2 text-xs">{{ field.database }} / {{ field.table }}</div>
              <div class="mt-2 text-xs">{{ kindName(field.kind) }} · {{ ui.levelLabel[field.level] }}</div>
              <div class="mt-2 text-xs font-semibold">{{ ui.rowHits.replace('{count}', String(field.count)) }}</div>
              <div v-if="field.samples.length" class="mt-3 max-h-20 overflow-auto rounded bg-background/70 p-2 font-mono text-xs">
                <div v-for="sample in field.samples" :key="sample">{{ sample }}</div>
              </div>
              <div
                v-if="selectedFieldKey === field.key"
                class="absolute left-0 top-full z-30 mt-2 w-80 rounded-md border bg-background p-4 text-foreground shadow-xl md:left-full md:top-0 md:ml-3 md:mt-0"
                @click.stop
              >
                <div class="flex items-start justify-between gap-3">
                  <div>
                    <div class="text-xs text-muted-foreground">{{ ui.fieldDetail }}</div>
                    <div class="mt-1 break-all font-mono text-base font-semibold">{{ field.column }}</div>
                  </div>
                  <Button variant="outline" size="sm" @click.stop="selectedFieldKey = ''">{{ ui.close }}</Button>
                </div>
                <div class="mt-3 space-y-2 text-xs">
                  <div class="flex items-center gap-1.5">
                    <DatabaseIcon :db-type="field.dbType || ''" class="h-3.5 w-3.5 shrink-0" />
                    <span>{{ databaseScopeText(field) || "-" }}</span>
                  </div>
                  <div><span class="text-muted-foreground">{{ ui.database }}：</span><span class="font-mono">{{ field.database }}</span></div>
                  <div><span class="text-muted-foreground">{{ ui.table }}：</span><span class="font-mono">{{ field.table }}</span></div>
                  <div><span class="text-muted-foreground">{{ ui.kind }}：</span>{{ kindName(field.kind) }}</div>
                  <div><span class="text-muted-foreground">{{ ui.risk }}：</span><span class="rounded-full border px-2 py-0.5" :class="riskClass(field.level)">{{ ui.levelLabel[field.level] }}</span></div>
                  <div><span class="text-muted-foreground">{{ ui.rows }}：</span><b>{{ field.count }}</b></div>
                </div>
                <div class="mt-3">
                  <div class="text-xs text-muted-foreground">{{ ui.sampleValues }}</div>
                  <div v-if="field.samples.length" class="mt-2 max-h-32 overflow-auto rounded-md border bg-muted/20 p-3 font-mono text-xs">
                    <div v-for="sample in field.samples" :key="sample">{{ sample }}</div>
                  </div>
                  <div v-else class="mt-2 rounded-md border bg-muted/20 p-3 text-xs text-muted-foreground">
                    {{ ui.noFieldSamplesHint }}
                  </div>
                </div>
              </div>
            </div>
          </div>
        </div>

        <div v-else-if="activeTab === 'targets'" class="p-4">
          <table v-if="selectedTask.targets.length" class="w-full text-xs">
            <thead class="bg-muted/60 text-muted-foreground"><tr><th class="px-3 py-2 text-left">{{ ui.type }}</th><th class="px-3 py-2 text-left">{{ ui.address }}</th><th class="px-3 py-2 text-left">{{ ui.usernameLabel }}</th><th class="px-3 py-2 text-left">{{ ui.source }}</th></tr></thead>
            <tbody>
              <tr v-for="target in selectedTask.targets" :key="`${target.line}-${target.raw}`" class="border-t">
                <td class="px-3 py-2">{{ target.dbType }}</td>
                <td class="px-3 py-2 font-mono">{{ target.host }}:{{ target.port }}</td>
                <td class="px-3 py-2">{{ target.username || "-" }}</td>
                <td class="px-3 py-2 text-muted-foreground">line {{ target.line }}</td>
              </tr>
            </tbody>
          </table>
          <div v-else class="p-8 text-center text-sm text-muted-foreground">{{ ui.noTargets }}</div>
        </div>

        <div v-else-if="activeTab === 'sql'" class="p-4">
          <textarea class="h-44 w-full resize-none rounded-md border bg-muted/30 p-3 font-mono text-xs" readonly :value="selectedTask.sql || ui.sqlPlaceholder" />
        </div>

        <div v-else-if="activeTab === 'samples'" class="space-y-4 p-4">
          <div class="grid gap-3 md:grid-cols-[1fr_1fr_160px]">
            <Input v-model="sampleQuery" class="h-9" :placeholder="ui.contentSearch" />
            <Input v-model="matchQuery" class="h-9" :placeholder="ui.matchSearch" />
            <div class="text-xs text-muted-foreground">{{ filteredSampleSummary }}</div>
          </div>
          <div v-for="group in filteredSampleGroups" :key="group.key" class="overflow-hidden rounded-md border">
            <div class="flex flex-wrap items-center gap-x-4 gap-y-1 bg-blue-50 px-3 py-2 text-xs dark:bg-blue-950/20">
              <span class="inline-flex items-center gap-1.5">
                <DatabaseIcon :db-type="group.dbType || ''" class="h-3.5 w-3.5 shrink-0" />
                <span class="text-muted-foreground">{{ ui.connectionSource }}</span>
                <b>{{ databaseScopeText(group) || "-" }}</b>
              </span>
              <span><span class="text-muted-foreground">{{ ui.databaseName }}</span> <b>{{ group.database }}</b></span>
              <span><span class="text-muted-foreground">{{ ui.tableName }}</span> <b>{{ group.table }}</b></span>
              <span><span class="text-muted-foreground">{{ ui.fieldEvidence }}</span> <b>{{ group.fields.length }}</b></span>
              <span v-if="group.rows.length"><span class="text-muted-foreground">{{ ui.sampleRows }}</span> <b>{{ group.rows.length }}</b></span>
            </div>
            <div v-if="group.rows.length" class="max-w-full overflow-x-auto border-t">
              <table class="min-w-max table-fixed text-xs">
                <thead>
                  <tr>
                    <th
                      v-for="field in group.fields"
                      :key="field.key"
                      class="w-48 min-w-[12rem] px-3 py-2 text-left align-top font-mono"
                      :class="riskTextClass(field.level)"
                    >
                      <div class="whitespace-normal break-words">{{ field.column }}</div>
                      <div class="whitespace-normal break-words font-sans text-[11px]">{{ ui.levelLabel[field.level] }} · {{ kindName(field.kind) }}</div>
                    </th>
                  </tr>
                </thead>
                <tbody>
                  <tr v-for="(row, index) in group.rows" :key="index" class="border-t">
                    <td
                      v-for="field in group.fields"
                      :key="field.key"
                      class="w-48 min-w-[12rem] px-3 py-2 align-top font-mono"
                      :class="riskTextClass(field.level)"
                    >
                      <div class="whitespace-normal break-words">{{ row[field.column] || "" }}</div>
                    </td>
                  </tr>
                </tbody>
              </table>
            </div>
            <div v-else class="border-t p-4">
              <div class="flex flex-wrap gap-2">
                <span v-for="field in group.fields" :key="field.key" class="rounded-full border px-2 py-1 font-mono text-xs" :class="riskClass(field.level)">
                  {{ field.column }} · {{ ui.levelLabel[field.level] }} · {{ kindName(field.kind) }}
                </span>
              </div>
              <div class="mt-3 text-xs text-muted-foreground">{{ ui.noSampleRowsHint }}</div>
            </div>
          </div>
          <div v-if="filteredSampleGroups.length === 0" class="rounded-md border bg-muted/20 p-8 text-center text-sm text-muted-foreground">
            <div>{{ ui.noSamples }}</div>
            <div class="mt-2 text-xs">
              {{ ui.noSampleRowsHint }}
            </div>
          </div>
        </div>

        <div v-else class="max-h-96 overflow-auto p-4 font-mono text-xs">
          <div v-for="(entry, index) in selectedTask.job?.logs || []" :key="index">
            {{ entry.time }} [{{ entry.level }}] {{ entry.message }}
          </div>
          <div v-for="(entry, index) in selectedTask.errors" :key="`error-${index}`" class="text-destructive">
            error {{ entry }}
          </div>
          <div v-if="!selectedTask.job?.logs?.length && !selectedTask.errors.length" class="font-sans text-sm text-muted-foreground">{{ ui.logs }}</div>
        </div>
      </div>

      <p v-if="error" class="mt-3 text-xs text-destructive">{{ error }}</p>
      <p v-if="exportMessage" class="mt-3 text-xs text-muted-foreground">{{ exportMessage }}</p>
    </main>
  </section>
</template>
