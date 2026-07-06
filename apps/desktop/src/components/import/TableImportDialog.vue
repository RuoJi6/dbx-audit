<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { uuid } from "@/lib/common/utils";
import { useI18n } from "vue-i18n";
import { isTauriRuntime } from "@/lib/backend/tauriRuntime";
import { Dialog, DialogHeader, DialogTitle, DialogFooter, DialogScrollContent } from "@/components/ui/dialog";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { AlertTriangle, ArrowLeft, ArrowRight, Check, CheckCircle2, FileJson, FileSpreadsheet, FileText, FileUp, Loader2, RefreshCw, Square, Upload, X } from "@lucide/vue";
import { useConnectionStore } from "@/stores/connectionStore";
import { useToast } from "@/composables/useToast";
import { autoMapImportColumns, nextTableImportWizardStep, previousTableImportWizardStep, requiredImportTargetColumns, validateImportMappings, type TableImportWizardStep } from "@/lib/table/tableImport";
import type { ColumnInfo } from "@/types/database";
import * as api from "@/lib/backend/api";

const { t } = useI18n();
const store = useConnectionStore();
const { toast } = useToast();
const open = defineModel<boolean>("open", { default: false });

const props = defineProps<{
  prefillConnectionId?: string;
  prefillDatabase?: string;
  prefillSchema?: string;
  prefillTable?: string;
}>();

const SKIP_VALUE = "__skip__";
const targetColumns = ref<ColumnInfo[]>([]);
const selectedSource = ref<string | File | null>(null);
const sourceFormat = ref<api.TableImportSourceFormat>("csv");
const preview = ref<api.TableImportPreview | null>(null);
const columnMapping = ref<Record<string, string>>({});
const loadingTarget = ref(false);
const loadingPreview = ref(false);
const importMode = ref<api.TableImportMode>("append");
const batchSize = ref(500);
const running = ref(false);
const cancelling = ref(false);
const importId = ref("");
const progress = ref<api.TableImportProgress | null>(null);
const errorMessage = ref("");
const wizardStep = ref<TableImportWizardStep>("source");
const fileInput = ref<HTMLInputElement | null>(null);
const delimiter = ref(",");
const hasHeader = ref(true);
const trimValues = ref(false);
const emptyStringAsNull = ref(true);
const selectedSheet = ref("");
const jsonShape = ref<api.TableImportJsonShape>("auto");
const previewLimit = ref(50);
let previewReloadTimer: ReturnType<typeof setTimeout> | null = null;

const formatOptions: Array<{ value: api.TableImportSourceFormat; icon: any; labelKey: string; descriptionKey: string }> = [
  { value: "csv", icon: FileText, labelKey: "tableImport.formatCsv", descriptionKey: "tableImport.formatCsvDescription" },
  { value: "tsv", icon: FileText, labelKey: "tableImport.formatTsv", descriptionKey: "tableImport.formatTsvDescription" },
  { value: "delimited", icon: FileText, labelKey: "tableImport.formatDelimited", descriptionKey: "tableImport.formatDelimitedDescription" },
  { value: "json", icon: FileJson, labelKey: "tableImport.formatJson", descriptionKey: "tableImport.formatJsonDescription" },
  { value: "excel", icon: FileSpreadsheet, labelKey: "tableImport.formatExcel", descriptionKey: "tableImport.formatExcelDescription" },
];

const wizardSteps: Array<{ value: TableImportWizardStep; labelKey: string }> = [
  { value: "source", labelKey: "tableImport.stepSource" },
  { value: "options", labelKey: "tableImport.stepOptions" },
  { value: "mapping", labelKey: "tableImport.stepMapping" },
  { value: "review", labelKey: "tableImport.stepReview" },
  { value: "execution", labelKey: "tableImport.stepExecution" },
];

const selectedConnection = computed(() => (props.prefillConnectionId ? store.getConfig(props.prefillConnectionId) : undefined));
const targetColumnNames = computed(() => targetColumns.value.map((column) => column.name));
const mappedColumns = computed<api.TableImportColumnMapping[]>(() => {
  const currentPreview = preview.value;
  if (!currentPreview) return [];
  return currentPreview.columns
    .map((sourceColumn) => ({
      sourceColumn,
      targetColumn: columnMapping.value[sourceColumn] ?? "",
    }))
    .filter((mapping) => mapping.targetColumn);
});
const mappedCount = computed(() => mappedColumns.value.length);
const mappingValidation = computed(() => validateImportMappings(mappedColumns.value));
const requiredUnmappedColumns = computed(() =>
  requiredImportTargetColumns(
    targetColumns.value,
    mappedColumns.value.map((mapping) => mapping.targetColumn),
  ),
);
const canImport = computed(() => !!preview.value && !!props.prefillConnectionId && !!props.prefillTable && mappingValidation.value.valid && !running.value);
const canGoBack = computed(() => wizardStep.value !== "source" && wizardStep.value !== "execution" && !running.value);
const canGoNext = computed(() => {
  if (wizardStep.value === "source") return !!selectedSource.value && !!sourceFormat.value;
  if (wizardStep.value === "options") return !!preview.value;
  if (wizardStep.value === "mapping") return mappingValidation.value.valid;
  return false;
});
const progressPercent = computed(() => {
  const p = progress.value;
  if (!p || p.totalRows <= 0) return 0;
  return Math.min(100, Math.round((p.rowsImported / p.totalRows) * 100));
});
const targetLabel = computed(() => {
  const pieces = [selectedConnection.value?.name, props.prefillDatabase, props.prefillSchema, props.prefillTable].filter(Boolean);
  return pieces.join(" / ");
});
const selectedSourceName = computed(() => {
  const source = selectedSource.value;
  if (!source) return "";
  return typeof source === "string" ? source.split(/[\\/]/).pop() || source : source.name;
});
const parseOptions = computed<api.TableImportParseOptions>(() => ({
  delimiter: sourceFormat.value === "tsv" ? "\\t" : sourceFormat.value === "csv" ? "," : delimiter.value,
  hasHeader: hasHeader.value,
  trimValues: trimValues.value,
  emptyStringAsNull: emptyStringAsNull.value,
  sheetName: sourceFormat.value === "excel" ? selectedSheet.value || null : null,
  jsonShape: sourceFormat.value === "json" ? jsonShape.value : null,
}));
const terminalStatus = computed(() => progress.value?.status && ["done", "error", "cancelled"].includes(progress.value.status));

function resetState() {
  targetColumns.value = [];
  selectedSource.value = null;
  sourceFormat.value = "csv";
  delimiter.value = ",";
  hasHeader.value = true;
  trimValues.value = false;
  emptyStringAsNull.value = true;
  selectedSheet.value = "";
  jsonShape.value = "auto";
  previewLimit.value = 50;
  preview.value = null;
  columnMapping.value = {};
  importMode.value = "append";
  batchSize.value = 500;
  running.value = false;
  cancelling.value = false;
  importId.value = "";
  progress.value = null;
  errorMessage.value = "";
  wizardStep.value = "source";
}

function detectFormat(name: string): api.TableImportSourceFormat {
  const lower = name.toLowerCase();
  if (lower.endsWith(".tsv")) return "tsv";
  if (lower.endsWith(".txt")) return "delimited";
  if (lower.endsWith(".json")) return "json";
  if (lower.endsWith(".xls") || lower.endsWith(".xlsx") || lower.endsWith(".xlsm")) return "excel";
  return "csv";
}

function applyAutoMapping() {
  const currentPreview = preview.value;
  if (!currentPreview) return;
  columnMapping.value = autoMapImportColumns(currentPreview.columns, targetColumnNames.value);
}

async function loadTargetColumns() {
  if (!props.prefillConnectionId || !props.prefillDatabase || !props.prefillTable) return;
  loadingTarget.value = true;
  errorMessage.value = "";
  try {
    await store.ensureConnected(props.prefillConnectionId);
    targetColumns.value = await api.getColumns(props.prefillConnectionId, props.prefillDatabase, props.prefillSchema || props.prefillDatabase, props.prefillTable);
    applyAutoMapping();
  } catch (e: any) {
    errorMessage.value = String(e?.message || e);
  } finally {
    loadingTarget.value = false;
  }
}

async function previewSelectedImportFile(fileOrPath: string | File) {
  return api.previewTableImportFile(fileOrPath, {
    sourceFormat: sourceFormat.value,
    parseOptions: parseOptions.value,
    previewLimit: Math.max(1, Number(previewLimit.value) || 50),
  });
}

async function loadPreview(fileOrPath = selectedSource.value) {
  if (!fileOrPath) return;
  loadingPreview.value = true;
  errorMessage.value = "";
  try {
    const nextPreview = await previewSelectedImportFile(fileOrPath);
    preview.value = nextPreview;
    if (sourceFormat.value === "excel" && !selectedSheet.value && nextPreview.sheets?.length) {
      selectedSheet.value = nextPreview.sheets[0];
    }
    applyAutoMapping();
  } catch (e: any) {
    preview.value = null;
    columnMapping.value = {};
    errorMessage.value = String(e?.message || e);
  } finally {
    loadingPreview.value = false;
  }
}

function assignSelectedSource(source: string | File) {
  selectedSource.value = source;
  preview.value = null;
  columnMapping.value = {};
  progress.value = null;
  errorMessage.value = "";
  const name = typeof source === "string" ? source : source.name;
  sourceFormat.value = detectFormat(name);
  delimiter.value = sourceFormat.value === "tsv" ? "\\t" : ",";
  selectedSheet.value = "";
  wizardStep.value = "options";
}

async function selectFile() {
  if (!isTauriRuntime()) {
    fileInput.value?.click();
    return;
  }
  const { open } = await import("@tauri-apps/plugin-dialog");
  const selected = await open({
    multiple: false,
    filters: [
      { name: "Data files", extensions: ["csv", "tsv", "txt", "json", "xlsx", "xlsm", "xls"] },
      { name: "Text", extensions: ["csv", "tsv", "txt"] },
      { name: "JSON", extensions: ["json"] },
      { name: "Excel", extensions: ["xlsx", "xlsm", "xls"] },
    ],
  });
  if (!selected || Array.isArray(selected)) return;
  assignSelectedSource(selected);
}

async function handleFileInputChange(event: Event) {
  const input = event.target as HTMLInputElement;
  const file = input.files?.[0];
  input.value = "";
  if (!file || running.value) return;
  assignSelectedSource(file);
}

function updateMapping(sourceColumn: string, value: any) {
  const target = String(value);
  columnMapping.value = {
    ...columnMapping.value,
    [sourceColumn]: target === SKIP_VALUE ? "" : target,
  };
}

function formatCell(value: unknown) {
  if (value === null) return "NULL";
  if (typeof value === "object") return JSON.stringify(value);
  return String(value);
}

function goBack() {
  wizardStep.value = previousTableImportWizardStep(wizardStep.value);
}

function canOpenStep(step: TableImportWizardStep) {
  if (running.value || step === "execution") return false;
  if (step === "source") return true;
  if (step === "options") return !!selectedSource.value;
  if (step === "mapping") return !!preview.value;
  if (step === "review") return !!preview.value && mappingValidation.value.valid;
  return false;
}

async function goNext() {
  if (wizardStep.value === "options" && !preview.value) {
    await loadPreview();
    if (!preview.value) return;
  }
  wizardStep.value = nextTableImportWizardStep(wizardStep.value);
}

async function startImport() {
  const currentPreview = preview.value;
  if (!canImport.value || !currentPreview || !props.prefillConnectionId || !props.prefillTable) return;
  running.value = true;
  cancelling.value = false;
  errorMessage.value = "";
  wizardStep.value = "execution";
  importId.value = uuid();
  progress.value = {
    importId: importId.value,
    status: "running",
    rowsImported: 0,
    totalRows: currentPreview.totalRows,
  };

  try {
    const summary = await api.importTableFile(
      {
        importId: importId.value,
        connectionId: props.prefillConnectionId,
        database: props.prefillDatabase || "",
        schema: props.prefillSchema || "",
        table: props.prefillTable,
        filePath: currentPreview.filePath,
        sourceRef: currentPreview.sourceRef || null,
        sourceFormat: sourceFormat.value,
        parseOptions: parseOptions.value,
        mappings: mappedColumns.value,
        mode: importMode.value,
        batchSize: Math.max(1, Number(batchSize.value) || 500),
      },
      (nextProgress) => {
        progress.value = nextProgress;
      },
    );
    progress.value = { importId: summary.importId, status: "done", rowsImported: summary.rowsImported, totalRows: summary.totalRows };
    toast(t("tableImport.success", { count: summary.rowsImported }), 2500);
    store.invalidateMetadataCache(props.prefillConnectionId, props.prefillDatabase || "", props.prefillSchema || undefined, props.prefillTable);
  } catch (e: any) {
    const message = String(e?.message || e);
    errorMessage.value = message;
    progress.value = {
      importId: importId.value,
      status: progress.value?.status === "cancelled" ? "cancelled" : "error",
      rowsImported: progress.value?.rowsImported ?? 0,
      totalRows: progress.value?.totalRows ?? currentPreview.totalRows,
      error: message,
    };
  } finally {
    running.value = false;
    cancelling.value = false;
  }
}

async function cancelImport() {
  if (!importId.value) return;
  cancelling.value = true;
  await api.cancelTableImport(importId.value);
}

function schedulePreviewReload() {
  if (!preview.value || !selectedSource.value || loadingPreview.value || running.value) return;
  if (previewReloadTimer) clearTimeout(previewReloadTimer);
  previewReloadTimer = setTimeout(() => {
    void loadPreview();
  }, 250);
}

watch(
  open,
  (value) => {
    if (value) {
      resetState();
      void loadTargetColumns();
    }
  },
  { immediate: true },
);

watch([sourceFormat, delimiter, hasHeader, trimValues, emptyStringAsNull, selectedSheet, jsonShape, previewLimit], schedulePreviewReload);
</script>

<template>
  <Dialog v-model:open="open">
    <DialogScrollContent class="sm:max-w-[980px] pt-12" :trap-focus="false" @interact-outside.prevent>
      <DialogHeader>
        <DialogTitle class="flex items-center gap-2 text-base">
          <FileUp class="h-4 w-4" />
          {{ t("tableImport.title") }}
        </DialogTitle>
      </DialogHeader>

      <div class="space-y-4 py-2">
        <div class="grid grid-cols-[1fr_auto] gap-2">
          <input ref="fileInput" type="file" accept=".csv,.tsv,.txt,.json,.xlsx,.xlsm,.xls" class="hidden" @change="handleFileInputChange" />
          <div class="min-w-0 rounded-md border bg-muted/20 px-3 py-2">
            <div class="truncate text-xs text-muted-foreground">{{ t("tableImport.target") }}</div>
            <div class="truncate text-sm font-medium">
              {{ targetLabel || t("editor.noDatabase") }}
            </div>
          </div>
          <Button variant="outline" size="sm" :disabled="running || loadingPreview" @click="selectFile">
            <Loader2 v-if="loadingPreview" class="mr-1.5 h-3.5 w-3.5 animate-spin" />
            <Upload v-else class="mr-1.5 h-3.5 w-3.5" />
            {{ selectedSource ? t("tableImport.changeFile") : t("tableImport.selectFile") }}
          </Button>
        </div>

        <div class="grid grid-cols-5 gap-1 rounded-md border bg-muted/20 p-1">
          <button v-for="step in wizardSteps" :key="step.value" type="button" class="h-8 rounded px-2 text-xs font-medium" :class="wizardStep === step.value ? 'bg-background text-foreground shadow-sm' : 'text-muted-foreground'" :disabled="!canOpenStep(step.value)" @click="wizardStep = step.value">
            {{ t(step.labelKey) }}
          </button>
        </div>

        <div v-if="wizardStep === 'source'" class="space-y-4">
          <div class="rounded-md border border-dashed p-6 text-center">
            <FileUp class="mx-auto mb-3 h-8 w-8 text-muted-foreground" />
            <div class="text-sm font-medium">{{ selectedSourceName || t("tableImport.noFileSelected") }}</div>
            <Button class="mt-4" size="sm" @click="selectFile">
              <Upload class="mr-1.5 h-3.5 w-3.5" />
              {{ t("tableImport.selectFile") }}
            </Button>
          </div>
          <div class="grid grid-cols-5 gap-2">
            <button v-for="format in formatOptions" :key="format.value" type="button" class="min-h-20 rounded-md border px-3 py-2 text-left" :class="sourceFormat === format.value ? 'border-primary bg-primary/5' : 'hover:bg-muted/30'" @click="sourceFormat = format.value">
              <component :is="format.icon" class="mb-2 h-4 w-4 text-muted-foreground" />
              <div class="text-xs font-medium">{{ t(format.labelKey) }}</div>
              <div class="mt-1 text-[11px] leading-snug text-muted-foreground">{{ t(format.descriptionKey) }}</div>
            </button>
          </div>
        </div>

        <div v-else-if="wizardStep === 'options'" class="space-y-4">
          <div class="grid grid-cols-3 gap-3">
            <div class="space-y-1.5">
              <Label class="text-xs">{{ t("tableImport.sourceFormat") }}</Label>
              <Select :model-value="sourceFormat" @update:model-value="(value: any) => (sourceFormat = value)">
                <SelectTrigger class="h-8 text-xs">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem v-for="format in formatOptions" :key="format.value" :value="format.value">
                    {{ t(format.labelKey) }}
                  </SelectItem>
                </SelectContent>
              </Select>
            </div>
            <div class="space-y-1.5">
              <Label class="text-xs">{{ t("tableImport.previewRows") }}</Label>
              <Input v-model.number="previewLimit" type="number" min="1" max="500" class="h-8 text-xs" />
            </div>
            <div class="space-y-1.5">
              <Label class="text-xs">{{ t("tableImport.sourceFile") }}</Label>
              <div class="flex h-8 items-center rounded-md border px-2 text-xs">
                <span class="truncate">{{ selectedSourceName || t("tableImport.noFileSelected") }}</span>
              </div>
            </div>
          </div>

          <div v-if="sourceFormat === 'csv' || sourceFormat === 'tsv' || sourceFormat === 'delimited'" class="grid grid-cols-4 gap-3 rounded-md border p-3">
            <div class="space-y-1.5">
              <Label class="text-xs">{{ t("tableImport.delimiter") }}</Label>
              <Input v-model="delimiter" :disabled="sourceFormat !== 'delimited'" class="h-8 text-xs font-mono" />
            </div>
            <label class="flex items-center gap-2 text-xs">
              <input v-model="hasHeader" type="checkbox" class="h-3.5 w-3.5 accent-primary" />
              {{ t("tableImport.hasHeader") }}
            </label>
            <label class="flex items-center gap-2 text-xs">
              <input v-model="trimValues" type="checkbox" class="h-3.5 w-3.5 accent-primary" />
              {{ t("tableImport.trimValues") }}
            </label>
            <label class="flex items-center gap-2 text-xs">
              <input v-model="emptyStringAsNull" type="checkbox" class="h-3.5 w-3.5 accent-primary" />
              {{ t("tableImport.emptyStringAsNull") }}
            </label>
          </div>

          <div v-else-if="sourceFormat === 'json'" class="grid grid-cols-2 gap-3 rounded-md border p-3">
            <div class="space-y-1.5">
              <Label class="text-xs">{{ t("tableImport.jsonShape") }}</Label>
              <Select :model-value="jsonShape" @update:model-value="(value: any) => (jsonShape = value)">
                <SelectTrigger class="h-8 text-xs">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="auto">{{ t("tableImport.jsonShapeAuto") }}</SelectItem>
                  <SelectItem value="objects">{{ t("tableImport.jsonShapeObjects") }}</SelectItem>
                  <SelectItem value="arrays">{{ t("tableImport.jsonShapeArrays") }}</SelectItem>
                </SelectContent>
              </Select>
            </div>
          </div>

          <div v-else-if="sourceFormat === 'excel'" class="grid grid-cols-3 gap-3 rounded-md border p-3">
            <div class="space-y-1.5">
              <Label class="text-xs">{{ t("tableImport.sheet") }}</Label>
              <Select :model-value="selectedSheet" :disabled="!preview?.sheets?.length" @update:model-value="(value: any) => (selectedSheet = value)">
                <SelectTrigger class="h-8 text-xs">
                  <SelectValue :placeholder="t('tableImport.firstSheet')" />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem v-for="sheet in preview?.sheets || []" :key="sheet" :value="sheet">{{ sheet }}</SelectItem>
                </SelectContent>
              </Select>
            </div>
            <label class="flex items-center gap-2 text-xs">
              <input v-model="hasHeader" type="checkbox" class="h-3.5 w-3.5 accent-primary" />
              {{ t("tableImport.hasHeader") }}
            </label>
          </div>

          <div class="flex items-center gap-2">
            <Button size="sm" :disabled="!selectedSource || loadingPreview" @click="loadPreview()">
              <Loader2 v-if="loadingPreview" class="mr-1.5 h-3.5 w-3.5 animate-spin" />
              <RefreshCw v-else class="mr-1.5 h-3.5 w-3.5" />
              {{ preview ? t("tableImport.reloadPreview") : t("tableImport.loadPreview") }}
            </Button>
            <span v-if="preview" class="text-xs text-muted-foreground">{{ t("tableImport.previewReady", { rows: preview.totalRows, columns: preview.columns.length }) }}</span>
          </div>
        </div>

        <div v-else-if="wizardStep === 'mapping'" class="space-y-3">
          <div v-if="preview" class="grid grid-cols-3 gap-2 text-xs">
            <div class="rounded-md border px-3 py-2">
              <div class="text-muted-foreground">{{ t("tableImport.file") }}</div>
              <div class="truncate font-medium">{{ preview.fileName }}</div>
            </div>
            <div class="rounded-md border px-3 py-2">
              <div class="text-muted-foreground">{{ t("tableImport.rows") }}</div>
              <div class="font-medium">{{ preview.totalRows.toLocaleString() }}</div>
            </div>
            <div class="rounded-md border px-3 py-2">
              <div class="text-muted-foreground">{{ t("tableImport.mapped") }}</div>
              <div class="font-medium">{{ mappedCount }} / {{ preview.columns.length }}</div>
            </div>
          </div>

          <div v-if="preview" class="grid grid-cols-[minmax(240px,300px)_1fr] gap-3">
            <div class="rounded-md border">
              <div class="border-b px-3 py-2 text-xs font-medium">{{ t("tableImport.mapping") }}</div>
              <div class="max-h-[320px] overflow-auto p-2">
                <div v-for="sourceColumn in preview.columns" :key="sourceColumn" class="grid grid-cols-[1fr_1fr] items-center gap-2 py-1">
                  <div class="truncate font-mono text-xs" :title="sourceColumn">
                    {{ sourceColumn }}
                  </div>
                  <Select :model-value="columnMapping[sourceColumn] || SKIP_VALUE" @update:model-value="(value: any) => updateMapping(sourceColumn, value)">
                    <SelectTrigger class="h-7 text-xs">
                      <SelectValue />
                    </SelectTrigger>
                    <SelectContent>
                      <SelectItem :value="SKIP_VALUE">{{ t("tableImport.skipColumn") }}</SelectItem>
                      <SelectItem v-for="column in targetColumns" :key="column.name" :value="column.name">
                        {{ column.name }}
                      </SelectItem>
                    </SelectContent>
                  </Select>
                </div>
              </div>
            </div>

            <div class="min-w-0 rounded-md border">
              <div class="border-b px-3 py-2 text-xs font-medium">{{ t("tableImport.preview") }}</div>
              <div class="max-h-[320px] overflow-auto">
                <table class="min-w-full border-separate border-spacing-0 text-xs">
                  <thead class="sticky top-0 bg-background">
                    <tr>
                      <th v-for="column in preview.columns" :key="column" class="border-b border-r px-2 py-1.5 text-left font-medium">
                        <span class="block max-w-[140px] truncate">{{ column }}</span>
                      </th>
                    </tr>
                  </thead>
                  <tbody>
                    <tr v-for="(row, rowIndex) in preview.rows" :key="rowIndex">
                      <td v-for="(cell, colIndex) in row" :key="colIndex" class="max-w-[180px] border-b border-r px-2 py-1.5 font-mono" :class="{ 'text-muted-foreground': cell === null }">
                        <span class="block truncate">{{ formatCell(cell) }}</span>
                      </td>
                    </tr>
                  </tbody>
                </table>
              </div>
            </div>
          </div>

          <div v-if="mappingValidation.errors.length" class="rounded-md border border-destructive/30 bg-destructive/10 px-3 py-2 text-xs text-destructive">
            {{ mappingValidation.errors.join("; ") }}
          </div>
          <div v-else-if="requiredUnmappedColumns.length" class="flex items-start gap-2 rounded-md border border-amber-300 bg-amber-50 px-3 py-2 text-xs text-amber-800 dark:bg-amber-950/20 dark:text-amber-300">
            <AlertTriangle class="mt-0.5 h-3.5 w-3.5 shrink-0" />
            <span>{{ t("tableImport.requiredUnmapped", { columns: requiredUnmappedColumns.join(", ") }) }}</span>
          </div>
        </div>

        <div v-else-if="wizardStep === 'review'" class="space-y-3">
          <div class="grid grid-cols-2 gap-3 text-xs">
            <div class="rounded-md border px-3 py-2">
              <div class="text-muted-foreground">{{ t("tableImport.target") }}</div>
              <div class="truncate font-medium">{{ targetLabel }}</div>
            </div>
            <div class="rounded-md border px-3 py-2">
              <div class="text-muted-foreground">{{ t("tableImport.sourceFile") }}</div>
              <div class="truncate font-medium">{{ preview?.fileName }}</div>
            </div>
            <div class="rounded-md border px-3 py-2">
              <div class="text-muted-foreground">{{ t("tableImport.rows") }}</div>
              <div class="font-medium">{{ preview?.totalRows.toLocaleString() }}</div>
            </div>
            <div class="rounded-md border px-3 py-2">
              <div class="text-muted-foreground">{{ t("tableImport.mapped") }}</div>
              <div class="font-medium">{{ mappedCount }} / {{ preview?.columns.length || 0 }}</div>
            </div>
          </div>
          <div class="grid grid-cols-3 gap-3">
            <div class="space-y-1.5">
              <Label class="text-xs">{{ t("tableImport.mode") }}</Label>
              <Select :model-value="importMode" @update:model-value="(value: any) => (importMode = value)">
                <SelectTrigger class="h-8 text-xs">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="append">{{ t("tableImport.append") }}</SelectItem>
                  <SelectItem value="truncate">{{ t("tableImport.truncate") }}</SelectItem>
                </SelectContent>
              </Select>
            </div>
            <div class="space-y-1.5">
              <Label class="text-xs">{{ t("transfer.batchSize") }}</Label>
              <Input v-model.number="batchSize" type="number" min="1" class="h-8 text-xs" />
            </div>
          </div>
          <div v-if="importMode === 'truncate'" class="flex items-start gap-2 rounded-md border border-amber-300 bg-amber-50 px-3 py-2 text-xs text-amber-800 dark:bg-amber-950/20 dark:text-amber-300">
            <AlertTriangle class="mt-0.5 h-3.5 w-3.5 shrink-0" />
            <span>{{ t("tableImport.truncateWarning") }}</span>
          </div>
        </div>

        <div v-else class="space-y-4">
          <div class="rounded-md border px-4 py-5">
            <div class="flex items-center gap-3">
              <Loader2 v-if="running && !cancelling" class="h-5 w-5 animate-spin text-primary" />
              <Square v-else-if="cancelling || progress?.status === 'cancelled'" class="h-5 w-5 fill-current text-destructive" />
              <CheckCircle2 v-else-if="progress?.status === 'done'" class="h-5 w-5 text-emerald-600" />
              <AlertTriangle v-else-if="progress?.status === 'error'" class="h-5 w-5 text-destructive" />
              <FileUp v-else class="h-5 w-5 text-muted-foreground" />
              <div class="min-w-0 flex-1">
                <div class="text-sm font-medium">{{ t(`tableImport.status_${progress?.status || "idle"}`) }}</div>
                <div class="mt-1 text-xs text-muted-foreground">{{ progress?.rowsImported ?? 0 }} / {{ progress?.totalRows ?? preview?.totalRows ?? 0 }} · {{ progressPercent }}%</div>
              </div>
            </div>
            <div class="mt-4 h-2 overflow-hidden rounded bg-muted">
              <div class="h-full bg-primary transition-all" :style="{ width: `${progressPercent}%` }" />
            </div>
          </div>
          <div v-if="errorMessage || progress?.error" class="rounded-md border border-destructive/30 bg-destructive/10 px-3 py-2 text-xs text-destructive">
            {{ errorMessage || progress?.error }}
          </div>
        </div>

        <div v-if="loadingTarget" class="flex items-center gap-2 text-xs text-muted-foreground">
          <Loader2 class="h-3.5 w-3.5 animate-spin" />
          {{ t("common.loading") }}
        </div>
        <div v-if="errorMessage && wizardStep !== 'execution'" class="rounded-md border border-destructive/30 bg-destructive/10 px-3 py-2 text-xs text-destructive">
          {{ errorMessage }}
        </div>
      </div>

      <DialogFooter>
        <Button variant="outline" :disabled="running" @click="open = false">
          <X class="mr-1.5 h-3.5 w-3.5" />
          {{ terminalStatus ? t("common.close") : t("dangerDialog.cancel") }}
        </Button>
        <Button v-if="canGoBack" variant="outline" @click="goBack">
          <ArrowLeft class="mr-1.5 h-3.5 w-3.5" />
          {{ t("tableImport.back") }}
        </Button>
        <Button v-if="wizardStep === 'source' || wizardStep === 'options' || wizardStep === 'mapping'" :disabled="!canGoNext || loadingPreview" @click="goNext">
          <ArrowRight class="mr-1.5 h-3.5 w-3.5" />
          {{ t("tableImport.next") }}
        </Button>
        <Button v-else-if="wizardStep === 'review'" :disabled="!canImport" @click="startImport">
          <Upload class="mr-1.5 h-3.5 w-3.5" />
          {{ t("tableImport.start") }}
        </Button>
        <Button v-else-if="running" variant="destructive" :disabled="cancelling" @click="cancelImport">
          <Loader2 v-if="cancelling" class="mr-1.5 h-3.5 w-3.5 animate-spin" />
          <Square v-else class="mr-1.5 h-3.5 w-3.5 fill-current" />
          {{ t("sqlFile.cancel") }}
        </Button>
        <Button v-else-if="progress?.status === 'done'" @click="open = false">
          <Check class="mr-1.5 h-3.5 w-3.5" />
          {{ t("common.done") }}
        </Button>
      </DialogFooter>
    </DialogScrollContent>
  </Dialog>
</template>
