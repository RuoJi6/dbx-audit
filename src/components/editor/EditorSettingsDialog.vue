<script setup lang="ts">
import { ref, watch, shallowRef, computed } from "vue";
import type { EditorView as EditorViewType } from "@codemirror/view";
import { useI18n } from "vue-i18n";
import { CircleHelp, ExternalLink, FolderOpen, Loader2, Settings, Trash2 } from "lucide-vue-next";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { Separator } from "@/components/ui/separator";
import { Switch } from "@/components/ui/switch";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import {
  useSettingsStore,
  EDITOR_THEMES,
  FONT_FAMILIES,
  DEFAULT_EDITOR_SETTINGS,
  type AiProvider,
  type AiApiStyle,
} from "@/stores/settingsStore";
import { loadEditorTheme, editorFontTheme } from "@/lib/editorThemes";
import { isTauriRuntime } from "@/lib/tauriRuntime";
import type { JdbcDriverInfo, JdbcPluginStatus } from "@/types/database";
import * as api from "@/lib/api";
import { aiTestConnection } from "@/lib/api";
import { useToast } from "@/composables/useToast";

const { t } = useI18n();
const settingsStore = useSettingsStore();
const { toast } = useToast();

const props = defineProps<{
  open: boolean;
  initialTab?: string;
  appVersion?: string;
}>();

const emit = defineEmits<{
  "update:open": [value: boolean];
}>();

// Local edit state
const editFontFamily = ref(settingsStore.editorSettings.fontFamily);
const editFontSize = ref(settingsStore.editorSettings.fontSize);
const editTheme = ref(settingsStore.editorSettings.theme);
const editExecuteMode = ref(settingsStore.editorSettings.executeMode);
const editWordWrap = ref(settingsStore.editorSettings.wordWrap);
const editAppLayout = ref(settingsStore.editorSettings.appLayout);

// Sync from store when dialog opens
watch(
  () => props.open,
  (open) => {
    if (open) {
      editFontFamily.value = settingsStore.editorSettings.fontFamily;
      editFontSize.value = settingsStore.editorSettings.fontSize;
      editTheme.value = settingsStore.editorSettings.theme;
      editExecuteMode.value = settingsStore.editorSettings.executeMode;
      editWordWrap.value = settingsStore.editorSettings.wordWrap;
      editAppLayout.value = settingsStore.editorSettings.appLayout;
    }
  },
);

function hasChanges(): boolean {
  return (
    editFontFamily.value !== settingsStore.editorSettings.fontFamily ||
    editFontSize.value !== settingsStore.editorSettings.fontSize ||
    editTheme.value !== settingsStore.editorSettings.theme ||
    editExecuteMode.value !== settingsStore.editorSettings.executeMode ||
    editWordWrap.value !== settingsStore.editorSettings.wordWrap ||
    editAppLayout.value !== settingsStore.editorSettings.appLayout
  );
}

function applySettings() {
  settingsStore.updateEditorSettings({
    fontFamily: editFontFamily.value,
    fontSize: editFontSize.value,
    theme: editTheme.value,
    executeMode: editExecuteMode.value,
    wordWrap: editWordWrap.value,
    appLayout: editAppLayout.value,
  });
  emit("update:open", false);
}

function resetDefaults() {
  editFontFamily.value = DEFAULT_EDITOR_SETTINGS.fontFamily;
  editFontSize.value = DEFAULT_EDITOR_SETTINGS.fontSize;
  editTheme.value = DEFAULT_EDITOR_SETTINGS.theme;
  editExecuteMode.value = DEFAULT_EDITOR_SETTINGS.executeMode;
  editWordWrap.value = DEFAULT_EDITOR_SETTINGS.wordWrap;
  editAppLayout.value = DEFAULT_EDITOR_SETTINGS.appLayout;
}

function onExecuteModeChange(v: any) {
  if (v === "all" || v === "current") editExecuteMode.value = v;
}

function onFontFamilyChange(v: any) {
  if (typeof v === "string") editFontFamily.value = v;
}

function onThemeChange(v: any) {
  if (typeof v === "string") editTheme.value = v as typeof DEFAULT_EDITOR_SETTINGS.theme;
}

function setAppLayout(value: "separated" | "classic") {
  editAppLayout.value = value;
}

const activeSettingsTab = ref("editor");
const isWeb = !isTauriRuntime();
const displayedAppVersion = computed(() => (props.appVersion ? `v${props.appVersion}` : ""));

function openExternalUrl(url: string) {
  if (isTauriRuntime()) {
    import("@tauri-apps/plugin-shell").then(({ open }) => open(url));
  } else {
    window.open(url, "_blank", "noopener,noreferrer");
  }
}

const jdbcDrivers = ref<JdbcDriverInfo[]>([]);
const isLoadingJdbcDrivers = ref(false);
const jdbcPluginStatus = ref<JdbcPluginStatus | null>(null);
const isInstallingJdbcPlugin = ref(false);
const isUninstallingJdbcPlugin = ref(false);
const jdbcDriverPathInput = ref("");

watch(
  () => props.open,
  async (open) => {
    if (open) {
      activeSettingsTab.value = props.initialTab || "editor";
      passwordMessage.value = "";
      oldPassword.value = "";
      newPassword.value = "";
      confirmNewPassword.value = "";
      await settingsStore.initAiConfig();
      syncAiEditState();
      void loadJdbcDrivers();
      void loadJdbcPluginStatus();
    }
  },
);
const oldPassword = ref("");
const newPassword = ref("");
const confirmNewPassword = ref("");
const passwordMessage = ref("");
const passwordError = ref(false);
const changingPassword = ref(false);

async function changePassword() {
  if (newPassword.value !== confirmNewPassword.value) {
    passwordMessage.value = t("auth.passwordMismatch");
    passwordError.value = true;
    return;
  }
  changingPassword.value = true;
  passwordMessage.value = "";
  try {
    const res = await fetch("/api/auth/change-password", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ old_password: oldPassword.value, new_password: newPassword.value }),
    });
    if (res.ok) {
      passwordMessage.value = t("auth.passwordChanged");
      passwordError.value = false;
      oldPassword.value = "";
      newPassword.value = "";
      confirmNewPassword.value = "";
    } else if (res.status === 401) {
      passwordMessage.value = t("auth.oldPasswordWrong");
      passwordError.value = true;
    } else {
      passwordMessage.value = t("auth.changePasswordFailed");
      passwordError.value = true;
    }
  } catch {
    passwordMessage.value = t("auth.connectFailed");
    passwordError.value = true;
  } finally {
    changingPassword.value = false;
  }
}

function formatBytes(bytes: number) {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
}

async function loadJdbcDrivers() {
  if (isWeb) return;
  isLoadingJdbcDrivers.value = true;
  try {
    jdbcDrivers.value = await api.listJdbcDrivers();
  } catch (e: any) {
    toast(String(e?.message || e), 5000);
  } finally {
    isLoadingJdbcDrivers.value = false;
  }
}

async function loadJdbcPluginStatus() {
  if (isWeb) return;
  try {
    jdbcPluginStatus.value = await api.jdbcPluginStatus();
  } catch (e: any) {
    toast(String(e?.message || e), 5000);
  }
}

async function installJdbcPlugin() {
  if (isWeb || isInstallingJdbcPlugin.value) return;
  isInstallingJdbcPlugin.value = true;
  try {
    jdbcPluginStatus.value = await api.installJdbcPlugin();
    toast(t("settings.jdbcPluginInstallSuccess"));
    await loadJdbcDrivers();
  } catch (e: any) {
    toast(String(e?.message || e), 5000);
  } finally {
    isInstallingJdbcPlugin.value = false;
  }
}

async function uninstallJdbcPlugin() {
  if (isWeb || isUninstallingJdbcPlugin.value) return;
  isUninstallingJdbcPlugin.value = true;
  try {
    jdbcPluginStatus.value = await api.uninstallJdbcPlugin();
    toast(t("settings.jdbcPluginUninstallSuccess"));
    await loadJdbcDrivers();
  } catch (e: any) {
    toast(String(e?.message || e), 5000);
  } finally {
    isUninstallingJdbcPlugin.value = false;
  }
}

async function importJdbcDriverPaths(paths: string[]) {
  if (!paths.length) return;
  try {
    jdbcDrivers.value = await api.importJdbcDrivers(paths);
    jdbcDriverPathInput.value = "";
    toast(t("settings.jdbcImportSuccess", { count: paths.length }));
  } catch (e: any) {
    toast(String(e?.message || e), 5000);
  }
}

async function importJdbcDrivers() {
  if (isWeb) return;
  const { open } = await import("@tauri-apps/plugin-dialog");
  const selected = await open({
    title: t("settings.jdbcImport"),
    multiple: true,
    filters: [{ name: "JDBC Driver", extensions: ["jar"] }],
  });
  if (!selected) return;

  const paths = (Array.isArray(selected) ? selected : [selected]).filter(
    (path): path is string => typeof path === "string",
  );
  await importJdbcDriverPaths(paths);
}

async function importJdbcDriverPathInput() {
  const paths = jdbcDriverPathInput.value
    .split(/\r?\n/)
    .map((path) => path.trim())
    .filter(Boolean);
  await importJdbcDriverPaths(paths);
}

async function deleteJdbcDriver(path: string) {
  try {
    jdbcDrivers.value = await api.deleteJdbcDriver(path);
    toast(t("settings.jdbcDeleteSuccess"));
  } catch (e: any) {
    toast(String(e?.message || e), 5000);
  }
}

// ---------- AI Settings ----------
const aiProviderDefaults: Record<AiProvider, { endpoint: string; model: string }> = {
  claude: { endpoint: "https://api.anthropic.com/v1/messages", model: "claude-sonnet-4-20250514" },
  openai: { endpoint: "https://api.openai.com/v1/chat/completions", model: "gpt-4o" },
  custom: { endpoint: "", model: "" },
};

const aiEditProvider = ref<AiProvider>(settingsStore.aiConfig.provider);
const aiEditApiKey = ref(settingsStore.aiConfig.apiKey);
const aiEditEndpoint = ref(settingsStore.aiConfig.endpoint);
const aiEditModel = ref(settingsStore.aiConfig.model);
const aiEditApiStyle = ref<AiApiStyle>(settingsStore.aiConfig.apiStyle || "completions");
const aiEditProxyEnabled = ref(!!settingsStore.aiConfig.proxyEnabled);
const aiEditProxyUrl = ref(settingsStore.aiConfig.proxyUrl || "");
const aiEditEnableThinking = ref(settingsStore.aiConfig.enableThinking ?? true);

const aiCompletionsMode = computed(() => aiEditApiStyle.value === "completions");

const aiTesting = ref(false);
const aiTestResult = ref<"" | "success" | "error">("");
const aiTestError = ref("");

function syncAiEditState() {
  aiEditProvider.value = settingsStore.aiConfig.provider;
  aiEditApiKey.value = settingsStore.aiConfig.apiKey;
  aiEditEndpoint.value = settingsStore.aiConfig.endpoint;
  aiEditModel.value = settingsStore.aiConfig.model;
  aiEditApiStyle.value = settingsStore.aiConfig.apiStyle || "completions";
  aiEditProxyEnabled.value = !!settingsStore.aiConfig.proxyEnabled;
  aiEditProxyUrl.value = settingsStore.aiConfig.proxyUrl || "";
  aiEditEnableThinking.value = settingsStore.aiConfig.enableThinking ?? true;
  aiTestResult.value = "";
  aiTestError.value = "";
}

function aiSelectProvider(provider: AiProvider) {
  aiEditProvider.value = provider;
  aiEditEndpoint.value = aiProviderDefaults[provider].endpoint;
  aiEditModel.value = aiProviderDefaults[provider].model;
}

function aiHasChanges(): boolean {
  return (
    aiEditProvider.value !== settingsStore.aiConfig.provider ||
    aiEditApiKey.value !== settingsStore.aiConfig.apiKey ||
    aiEditEndpoint.value !== settingsStore.aiConfig.endpoint ||
    aiEditModel.value !== settingsStore.aiConfig.model ||
    aiEditApiStyle.value !== (settingsStore.aiConfig.apiStyle || "completions") ||
    aiEditProxyEnabled.value !== !!settingsStore.aiConfig.proxyEnabled ||
    aiEditProxyUrl.value !== (settingsStore.aiConfig.proxyUrl || "") ||
    aiEditEnableThinking.value !== (settingsStore.aiConfig.enableThinking ?? true)
  );
}

function aiApplySettings() {
  settingsStore.updateAiConfig({
    provider: aiEditProvider.value,
    apiKey: aiEditApiKey.value,
    endpoint: aiEditEndpoint.value,
    model: aiEditModel.value,
    apiStyle: aiEditApiStyle.value,
    proxyEnabled: aiEditProxyEnabled.value,
    proxyUrl: aiEditProxyUrl.value,
    enableThinking: aiEditEnableThinking.value,
  });
}

async function aiTestConn() {
  if (!aiEditApiKey.value.trim() || !aiEditEndpoint.value.trim() || !aiEditModel.value.trim()) return;
  aiTesting.value = true;
  aiTestResult.value = "";
  aiTestError.value = "";
  try {
    await aiTestConnection({
      provider: aiEditProvider.value,
      apiKey: aiEditApiKey.value,
      endpoint: aiEditEndpoint.value,
      model: aiEditModel.value,
      apiStyle: aiEditApiStyle.value,
      proxyEnabled: aiEditProxyEnabled.value,
      proxyUrl: aiEditProxyUrl.value,
    });
    aiTestResult.value = "success";
  } catch (e: any) {
    aiTestResult.value = "error";
    aiTestError.value = e?.message || String(e);
  } finally {
    aiTesting.value = false;
  }
}

// ---------- CodeMirror preview ----------
const previewRef = ref<HTMLDivElement>();
const previewView = shallowRef<EditorViewType | null>(null);

const previewSettings = computed(() => ({
  fontFamily: editFontFamily.value,
  fontSize: editFontSize.value,
  theme: editTheme.value,
}));

const previewSql = `SELECT u.id, u.name
FROM users u
ORDER BY u.id LIMIT 5;`;

let fontThemeComp: import("@codemirror/state").Compartment | null = null;
let themeComp: import("@codemirror/state").Compartment | null = null;
let editorViewModule: typeof import("@codemirror/view") | null = null;

watch(
  previewSettings,
  async (ss) => {
    if (!previewView.value || !fontThemeComp || !themeComp || !editorViewModule) return;

    const themeExt = await loadEditorTheme(ss.theme);
    previewView.value.dispatch({
      effects: [
        themeComp.reconfigure(themeExt),
        fontThemeComp.reconfigure(editorFontTheme(editorViewModule.EditorView, ss.fontSize, ss.fontFamily)),
      ],
    });
  },
  { deep: true },
);

let previewInitialized = false;

watch(activeSettingsTab, (tab) => {
  if (tab !== "editor" && previewView.value) {
    previewView.value.destroy();
    previewView.value = null;
    previewInitialized = false;
    fontThemeComp = null;
    themeComp = null;
    editorViewModule = null;
  }
});

watch(previewRef, async (el) => {
  if (!el || previewInitialized) return;
  previewInitialized = true;
  if (previewView.value) return;

  const [{ EditorView }, { EditorState, Compartment }, { sql, MySQL }, { basicSetup }] = await Promise.all([
    import("@codemirror/view"),
    import("@codemirror/state"),
    import("@codemirror/lang-sql"),
    import("codemirror"),
  ]);

  editorViewModule = { EditorView } as typeof import("@codemirror/view");
  fontThemeComp = new Compartment();
  themeComp = new Compartment();

  const ss = previewSettings.value;
  const themeExt = await loadEditorTheme(ss.theme);

  const state = EditorState.create({
    doc: previewSql,
    extensions: [
      basicSetup,
      sql({ dialect: MySQL }),
      themeComp.of(themeExt),
      fontThemeComp.of(editorFontTheme(EditorView, ss.fontSize, ss.fontFamily)),
    ],
  });

  previewView.value = new EditorView({ state, parent: previewRef.value });
});

watch(
  () => props.open,
  (open) => {
    if (!open && previewView.value) {
      previewView.value.destroy();
      previewView.value = null;
      previewInitialized = false;
      fontThemeComp = null;
      themeComp = null;
      editorViewModule = null;
    }
  },
);
</script>

<template>
  <Dialog :open="open" @update:open="(v: boolean) => emit('update:open', v)">
    <DialogContent class="sm:max-w-[720px] max-h-[calc(100vh-80px)] overflow-y-auto overflow-x-hidden">
      <DialogHeader>
        <DialogTitle class="flex items-center gap-2">
          <Settings class="h-4 w-4" />
          {{ t("settings.title") }}
        </DialogTitle>
      </DialogHeader>

      <Tabs v-model="activeSettingsTab">
        <TabsList class="w-full">
          <TabsTrigger value="editor" class="flex-1">{{ t("settings.editorTab") }}</TabsTrigger>
          <TabsTrigger value="appearance" class="flex-1">{{ t("settings.appearanceTab") }}</TabsTrigger>
          <TabsTrigger value="ai" class="flex-1">{{ t("settings.aiTab") }}</TabsTrigger>
          <TabsTrigger v-if="!isWeb" value="jdbc" class="flex-1">{{ t("settings.jdbcTab") }}</TabsTrigger>
          <TabsTrigger v-if="isWeb" value="security" class="flex-1">{{ t("settings.securityTab") }}</TabsTrigger>
          <TabsTrigger value="about" class="flex-1">{{ t("settings.aboutTab") }}</TabsTrigger>
        </TabsList>

        <TabsContent value="editor" class="space-y-5 py-2">
          <div class="grid gap-4 md:grid-cols-[minmax(0,1fr)_220px]">
            <!-- Font Family -->
            <div class="space-y-2">
              <Label>{{ t("settings.fontFamily") }}</Label>
              <Select :model-value="editFontFamily" @update:model-value="onFontFamilyChange">
                <SelectTrigger>
                  <SelectValue :placeholder="t('settings.selectFont')" />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem
                    v-for="font in FONT_FAMILIES"
                    :key="font.value"
                    :value="font.value"
                    :style="{ fontFamily: font.value }"
                  >
                    {{ font.label }}
                  </SelectItem>
                </SelectContent>
              </Select>
            </div>

            <!-- Theme -->
            <div class="space-y-2">
              <Label>{{ t("settings.theme") }}</Label>
              <Select :model-value="editTheme" @update:model-value="onThemeChange">
                <SelectTrigger>
                  <SelectValue :placeholder="t('settings.selectTheme')" />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem v-for="theme in EDITOR_THEMES" :key="theme.value" :value="theme.value">
                    <div class="flex items-center gap-2">
                      <span
                        class="h-3 w-3 rounded-full border"
                        :class="
                          theme.dark
                            ? 'bg-foreground border-foreground/20'
                            : 'bg-muted-foreground/30 border-muted-foreground/40'
                        "
                      />
                      {{ theme.label }}
                    </div>
                  </SelectItem>
                </SelectContent>
              </Select>
            </div>
          </div>

          <!-- Font Size -->
          <div class="space-y-2">
            <div class="flex items-center justify-between">
              <Label>{{ t("settings.fontSize") }}</Label>
              <span class="text-xs text-muted-foreground tabular-nums">{{ editFontSize }}px</span>
            </div>
            <input
              type="range"
              min="10"
              max="24"
              step="1"
              :value="editFontSize"
              @input="editFontSize = Number(($event.target as HTMLInputElement).value)"
              class="w-full accent-primary"
            />
            <div class="flex items-center gap-2 text-xs text-muted-foreground">
              <span>10px</span>
              <span class="flex-1 border-b border-dashed border-muted-foreground/30" />
              <span>24px</span>
            </div>
          </div>

          <Separator />

          <div class="grid gap-4 md:grid-cols-2">
            <div class="space-y-2">
              <Label>{{ t("settings.executeMode") }}</Label>
              <Select :model-value="editExecuteMode" @update:model-value="onExecuteModeChange">
                <SelectTrigger>
                  <SelectValue :placeholder="t('settings.executeMode')" />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="all">{{ t("settings.executeModeAll") }}</SelectItem>
                  <SelectItem value="current">{{ t("settings.executeModeCurrent") }}</SelectItem>
                </SelectContent>
              </Select>
            </div>

            <div class="flex items-start justify-between gap-4">
              <div class="space-y-1">
                <Label for="editor-word-wrap">{{ t("settings.wordWrap") }}</Label>
                <p class="text-xs text-muted-foreground">{{ t("settings.wordWrapDescription") }}</p>
              </div>
              <Switch id="editor-word-wrap" v-model:checked="editWordWrap" class="mt-0.5" />
            </div>
          </div>

          <Separator />

          <!-- Live Preview -->
          <div class="space-y-2">
            <Label>{{ t("settings.preview") }}</Label>
            <div
              class="rounded-md border overflow-auto max-w-full"
              :class="
                editTheme === 'vscode-light' || editTheme === 'duotone-light' || editTheme === 'xcode'
                  ? 'border-border'
                  : 'border-border/50'
              "
            >
              <div ref="previewRef" style="min-width: 100%" />
            </div>
          </div>

          <DialogFooter class="border-t-0 bg-transparent gap-3 sm:gap-3">
            <Button variant="outline" @click="resetDefaults">
              {{ t("settings.resetDefaults") }}
            </Button>
            <div class="flex-1" />
            <Button variant="outline" @click="emit('update:open', false)">
              {{ t("common.close") }}
            </Button>
            <Button :disabled="!hasChanges()" @click="applySettings">
              {{ t("settings.apply") }}
            </Button>
          </DialogFooter>
        </TabsContent>

        <TabsContent value="appearance" class="space-y-5 py-2">
          <div class="space-y-2">
            <Label>{{ t("settings.appLayout") }}</Label>
            <div class="grid grid-cols-2 gap-2">
              <Button
                type="button"
                variant="outline"
                class="h-auto justify-start p-3"
                :class="editAppLayout === 'separated' ? 'border-blue-300 border-2 ring-2 ring-blue-300/50' : ''"
                @click="setAppLayout('separated')"
              >
                <div class="text-left">
                  <div class="text-sm font-medium">{{ t("settings.appLayoutSeparated") }}</div>
                  <div class="text-xs text-muted-foreground">{{ t("settings.appLayoutSeparatedDescription") }}</div>
                </div>
              </Button>
              <Button
                type="button"
                variant="outline"
                class="h-auto justify-start p-3"
                :class="editAppLayout === 'classic' ? 'border-blue-300 border-2 ring-2 ring-blue-300/50' : ''"
                @click="setAppLayout('classic')"
              >
                <div class="text-left">
                  <div class="text-sm font-medium">{{ t("settings.appLayoutClassic") }}</div>
                  <div class="text-xs text-muted-foreground">{{ t("settings.appLayoutClassicDescription") }}</div>
                </div>
              </Button>
            </div>
          </div>

          <DialogFooter class="border-t-0 bg-transparent gap-3 sm:gap-3">
            <Button variant="outline" @click="resetDefaults">
              {{ t("settings.resetDefaults") }}
            </Button>
            <div class="flex-1" />
            <Button variant="outline" @click="emit('update:open', false)">
              {{ t("common.close") }}
            </Button>
            <Button :disabled="!hasChanges()" @click="applySettings">
              {{ t("settings.apply") }}
            </Button>
          </DialogFooter>
        </TabsContent>

        <!-- AI Settings Tab -->
        <TabsContent value="ai" class="space-y-5 py-2">
          <p class="text-xs text-muted-foreground">{{ t("ai.settingsHint") }}</p>

          <div class="space-y-3">
            <div class="grid grid-cols-3 items-center gap-3">
              <Label class="text-right text-xs">{{ t("ai.provider") }}</Label>
              <Select :model-value="aiEditProvider" @update:model-value="(v: any) => aiSelectProvider(v)">
                <SelectTrigger class="col-span-2 h-8 text-xs"><SelectValue /></SelectTrigger>
                <SelectContent>
                  <SelectItem value="claude">Claude</SelectItem>
                  <SelectItem value="openai">OpenAI</SelectItem>
                  <SelectItem value="custom">Custom</SelectItem>
                </SelectContent>
              </Select>
            </div>

            <div class="grid grid-cols-3 items-center gap-3">
              <Label class="text-right text-xs">API Key</Label>
              <Input v-model="aiEditApiKey" type="password" autocomplete="off" class="col-span-2 h-8 text-xs" />
            </div>

            <div class="grid grid-cols-3 items-center gap-3">
              <Label class="text-right text-xs">Endpoint</Label>
              <Input
                v-model="aiEditEndpoint"
                placeholder="https://api.openai.com/v1"
                autocomplete="off"
                class="col-span-2 h-8 text-xs"
              />
            </div>

            <div class="grid grid-cols-3 items-center gap-3">
              <Label class="text-right text-xs">Model</Label>
              <Input v-model="aiEditModel" autocomplete="off" class="col-span-2 h-8 text-xs" />
            </div>

            <div v-if="aiEditProvider !== 'claude'" class="grid grid-cols-3 items-center gap-3">
              <Label class="text-right text-xs">API</Label>
              <div class="col-span-2 flex gap-2">
                <Button
                  size="sm"
                  variant="outline"
                  class="h-8 flex-1 text-xs"
                  :class="{ 'border-blue-300 border-2 ring-2 ring-blue-300/50': aiEditApiStyle === 'completions' }"
                  @click="aiEditApiStyle = 'completions'"
                  >/chat/completions</Button
                >
                <Button
                  size="sm"
                  variant="outline"
                  class="h-8 flex-1 text-xs"
                  :class="{ 'border-blue-300 border-2 ring-2 ring-blue-300/50': aiEditApiStyle === 'responses' }"
                  @click="aiEditApiStyle = 'responses'"
                  >/responses</Button
                >
              </div>
            </div>

            <div class="grid grid-cols-3 items-center gap-3">
              <Label class="text-right text-xs">{{ t("ai.enableThinking") }}</Label>
              <div class="col-span-2 flex items-center gap-2">
                <label class="flex items-center gap-2 text-xs text-muted-foreground">
                  <input
                    v-model="aiEditEnableThinking"
                    type="checkbox"
                    class="h-4 w-4 shrink-0 accent-primary"
                    :disabled="!aiCompletionsMode"
                  />
                  {{ aiEditEnableThinking ? t("ai.enableThinkingOn") : t("ai.enableThinkingOff") }}
                </label>
                <Popover>
                  <PopoverTrigger as-child>
                    <CircleHelp class="h-3.5 w-3.5 cursor-help text-muted-foreground hover:text-foreground" />
                  </PopoverTrigger>
                  <PopoverContent class="max-w-[320px] text-xs leading-relaxed" side="top" align="start">
                    {{ t("ai.enableThinkingHint") }}
                  </PopoverContent>
                </Popover>
              </div>
            </div>

            <div class="grid grid-cols-3 items-center gap-3">
              <Label class="text-right text-xs">{{ t("ai.proxy") }}</Label>
              <label class="col-span-2 flex items-center gap-2 text-xs text-muted-foreground">
                <input v-model="aiEditProxyEnabled" type="checkbox" class="h-4 w-4 shrink-0 accent-primary" />
                {{ t("ai.proxyEnable") }}
              </label>
            </div>

            <div class="grid grid-cols-3 items-center gap-3">
              <Label class="text-right text-xs">{{ t("ai.proxyUrl") }}</Label>
              <Input
                v-model="aiEditProxyUrl"
                autocomplete="off"
                class="col-span-2 h-8 text-xs"
                placeholder="socks5://127.0.0.1:7890"
                :disabled="!aiEditProxyEnabled"
              />
            </div>
          </div>

          <DialogFooter class="flex items-center gap-2">
            <div class="flex-1 flex items-center gap-2">
              <Button
                size="sm"
                variant="outline"
                :disabled="aiTesting || !aiEditApiKey?.trim() || !aiEditEndpoint?.trim() || !aiEditModel?.trim()"
                @click="aiTestConn"
              >
                <Loader2 v-if="aiTesting" class="h-3 w-3 animate-spin mr-1" />
                {{ t("connection.test") }}
              </Button>
              <span v-if="aiTestResult === 'success'" class="text-xs text-green-500">{{
                t("connection.testSuccess")
              }}</span>
              <span
                v-else-if="aiTestResult === 'error'"
                class="text-xs text-destructive truncate max-w-[200px]"
                :title="aiTestError"
                >{{ aiTestError }}</span
              >
            </div>
            <Button variant="outline" @click="emit('update:open', false)">{{ t("common.close") }}</Button>
            <Button :disabled="!aiHasChanges()" @click="aiApplySettings">{{ t("settings.apply") }}</Button>
          </DialogFooter>
        </TabsContent>

        <TabsContent v-if="!isWeb" value="jdbc" class="space-y-5 py-2">
          <div class="rounded-md border bg-muted/20 p-4">
            <div class="flex min-h-12 items-center justify-between gap-3">
              <div class="min-w-0 space-y-1">
                <Label>{{ t("settings.jdbcPlugin") }}</Label>
                <p v-if="!jdbcPluginStatus?.installed" class="text-xs text-muted-foreground">
                  {{ t("settings.jdbcPluginNotInstalled") }}
                </p>
              </div>
              <div class="flex shrink-0 items-center gap-3">
                <span
                  v-if="jdbcPluginStatus?.installed"
                  class="text-xs"
                  :class="jdbcPluginStatus.compatible ? 'text-green-600' : 'text-destructive'"
                >
                  {{
                    jdbcPluginStatus.compatible
                      ? t("settings.jdbcPluginInstalled", {
                          version: jdbcPluginStatus.version || "-",
                        })
                      : t("settings.jdbcPluginIncompatible")
                  }}
                </span>
                <Button
                  v-if="jdbcPluginStatus?.installed"
                  type="button"
                  variant="outline"
                  :disabled="isUninstallingJdbcPlugin"
                  @click="uninstallJdbcPlugin"
                >
                  {{ isUninstallingJdbcPlugin ? t("common.loading") : t("settings.jdbcPluginUninstall") }}
                </Button>
                <Button
                  v-else
                  type="button"
                  variant="default"
                  :disabled="isInstallingJdbcPlugin"
                  @click="installJdbcPlugin"
                >
                  {{ isInstallingJdbcPlugin ? t("common.loading") : t("settings.jdbcPluginInstall") }}
                </Button>
              </div>
            </div>
          </div>

          <div class="space-y-3">
            <div class="space-y-1">
              <Label>{{ t("settings.jdbcDrivers") }}</Label>
            </div>
            <div class="flex items-center gap-2">
              <Input
                v-model="jdbcDriverPathInput"
                class="flex-1"
                :placeholder="t('settings.jdbcDriverPathPlaceholder')"
                @keydown.enter.prevent="importJdbcDriverPathInput"
              />
              <Button variant="outline" :disabled="!jdbcDriverPathInput.trim()" @click="importJdbcDriverPathInput">
                {{ t("settings.jdbcImportPath") }}
              </Button>
              <Button class="shrink-0" @click="importJdbcDrivers">
                <FolderOpen class="h-4 w-4" />
                {{ t("settings.jdbcImport") }}
              </Button>
            </div>
          </div>

          <div class="rounded-md border">
            <div v-if="isLoadingJdbcDrivers" class="p-4 text-sm text-muted-foreground">
              {{ t("common.loading") }}
            </div>
            <div v-else-if="jdbcDrivers.length === 0" class="p-4 text-sm text-muted-foreground">
              {{ t("settings.jdbcNoDrivers") }}
            </div>
            <div v-else class="divide-y">
              <div v-for="driver in jdbcDrivers" :key="driver.path" class="flex items-center gap-3 p-3">
                <div class="min-w-0 flex-1">
                  <div class="truncate text-sm font-medium">{{ driver.name }}</div>
                  <div class="truncate text-xs text-muted-foreground">{{ driver.path }}</div>
                </div>
                <div class="shrink-0 text-xs text-muted-foreground">{{ formatBytes(driver.size) }}</div>
                <Button variant="ghost" size="icon" class="h-8 w-8 shrink-0" @click="deleteJdbcDriver(driver.path)">
                  <Trash2 class="h-4 w-4" />
                </Button>
              </div>
            </div>
          </div>

          <DialogFooter class="border-t-0 bg-transparent">
            <Button variant="outline" @click="emit('update:open', false)">
              {{ t("common.close") }}
            </Button>
          </DialogFooter>
        </TabsContent>

        <TabsContent v-if="isWeb" value="security" class="space-y-5 py-2">
          <div class="space-y-3">
            <Label class="text-base">{{ t("auth.changePassword") }}</Label>
            <p class="text-sm text-muted-foreground">{{ t("auth.changePasswordDescription") }}</p>
            <Input
              v-model="oldPassword"
              type="password"
              :placeholder="t('auth.oldPassword')"
              class="h-9"
              autocomplete="off"
            />
            <Input
              v-model="newPassword"
              type="password"
              :placeholder="t('auth.newPassword')"
              class="h-9"
              autocomplete="off"
            />
            <Input
              v-model="confirmNewPassword"
              type="password"
              :placeholder="t('auth.confirmPassword')"
              class="h-9"
              autocomplete="off"
            />
            <p v-if="passwordMessage" class="text-xs" :class="passwordError ? 'text-destructive' : 'text-green-500'">
              {{ passwordMessage }}
            </p>
          </div>
          <DialogFooter class="border-t-0 bg-transparent">
            <Button variant="outline" @click="emit('update:open', false)">
              {{ t("common.close") }}
            </Button>
            <Button
              :disabled="changingPassword || !oldPassword || !newPassword || !confirmNewPassword"
              @click="changePassword"
            >
              {{ t("auth.changePassword") }}
            </Button>
          </DialogFooter>
        </TabsContent>

        <TabsContent value="about" class="space-y-5 py-2">
          <div class="rounded-lg border bg-muted/20 p-4">
            <div class="flex items-start justify-between gap-4">
              <div class="min-w-0 space-y-1">
                <div class="text-lg font-semibold">DBX</div>
                <p class="text-sm text-muted-foreground">{{ t("settings.aboutDescription") }}</p>
              </div>
              <div
                v-if="displayedAppVersion"
                class="rounded-md border bg-background px-2 py-1 text-xs text-muted-foreground"
              >
                {{ displayedAppVersion }}
              </div>
            </div>
          </div>

          <div class="grid gap-3 sm:grid-cols-2">
            <button
              type="button"
              class="rounded-lg border p-4 text-left transition-colors hover:bg-muted/40 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
              @click="openExternalUrl('https://qm.qq.com/cgi-bin/qm/qr?k=&group_code=1087880322')"
            >
              <div class="text-xs font-medium uppercase tracking-wider text-muted-foreground">
                {{ t("settings.community") }}
              </div>
              <div class="mt-3 flex items-center gap-2 text-sm font-medium">
                <img
                  src="data:image/svg+xml;base64,PHN2ZyB4bWxucz0iaHR0cDovL3d3dy53My5vcmcvMjAwMC9zdmciIGhlaWdodD0iODYiIHdpZHRoPSI4NiIgdmlld0JveD0iMCAwIDEyMCAxNDUiPjxwYXRoIGZpbGw9IiNmYWFiMDciIGQ9Ik02MC41MDMgMTQyLjIzN2MtMTIuNTMzIDAtMjQuMDM4LTQuMTk1LTMxLjQ0NS0xMC40Ni0zLjc2MiAxLjEyNC04LjU3NCAyLjkzMi0xMS42MSA1LjE3NS0yLjYgMS45MTgtMi4yNzUgMy44NzQtMS44MDcgNC42NjMgMi4wNTYgMy40NyAzNS4yNzMgMi4yMTYgNDQuODYyIDEuMTM2em0wIDBjMTIuNTM1IDAgMjQuMDM5LTQuMTk1IDMxLjQ0Ny0xMC40NiAzLjc2IDEuMTI0IDguNTczIDIuOTMyIDExLjYxIDUuMTc1IDIuNTk4IDEuOTE4IDIuMjc0IDMuODc0IDEuODA1IDQuNjYzLTIuMDU2IDMuNDctMzUuMjcyIDIuMjE2LTQ0Ljg2MiAxLjEzNnptMCAwIi8+PHBhdGggZD0iTTYwLjU3NiA2Ny4xMTljMjAuNjk4LS4xNCAzNy4yODYtNC4xNDcgNDIuOTA3LTUuNjgzIDEuMzQtLjM2NyAyLjA1Ni0xLjAyNCAyLjA1Ni0xLjAyNC4wMDUtLjE4OS4wODUtMy4zNy4wODUtNS4wMUMxMDUuNjI0IDI3Ljc2OCA5Mi41OC4wMDEgNjAuNSAwIDI4LjQyLjAwMSAxNS4zNzUgMjcuNzY5IDE1LjM3NSA1NS40MDFjMCAxLjY0Mi4wOCA0LjgyMi4wODYgNS4wMSAwIDAgLjU4My42MTUgMS42NS45MTMgNS4xOSAxLjQ0NCAyMi4wOSA1LjY1IDQzLjMxMiA1Ljc5NXptNTYuMjQ1IDIzLjAyYy0xLjI4My00LjEyOS0zLjAzNC04Ljk0NC00LjgwOC0xMy41NjggMCAwLTEuMDItLjEyNi0xLjUzNy4wMjMtMTUuOTEzIDQuNjIzLTM1LjIwMiA3LjU3LTQ5LjkgNy4zOTJoLS4xNTNjLTE0LjYxNi4xNzUtMzMuNzc0LTIuNzM3LTQ5LjYzNC03LjMxNS0uNjA2LS4xNzUtMS44MDItLjEtMS44MDItLjEtMS43NzQgNC42MjQtMy41MjUgOS40NC00LjgwOCAxMy41NjgtNi4xMTkgMTkuNjktNC4xMzYgMjcuODM4LTIuNjI3IDI4LjAyIDMuMjM5LjM5MiAxMi42MDYtMTQuODIxIDEyLjYwNi0xNC44MjEgMCAxNS40NTkgMTMuOTU3IDM5LjE5NSA0NS45MTggMzkuNDEzaC44NDhjMzEuOTYtLjIxOCA0NS45MTctMjMuOTU0IDQ1LjkxNy0zOS40MTMgMCAwIDkuMzY4IDE1LjIxMyAxMi42MDcgMTQuODIyIDEuNTA4LS4xODMgMy40OTEtOC4zMzItMi42MjctMjguMDIxIi8+PHBhdGggZmlsbD0iI2ZmZiIgZD0iTTQ5LjA4NSA0MC44MjRjLTQuMzUyLjE5Ny04LjA3LTQuNzYtOC4zMDQtMTEuMDYzLS4yMzYtNi4zMDUgMy4wOTgtMTEuNTc2IDcuNDUtMTEuNzczIDQuMzQ3LS4xOTUgOC4wNjQgNC43NiA4LjMgMTEuMDY1LjIzOCA2LjMwNi0zLjA5NyAxMS41NzctNy40NDYgMTEuNzcxbTMxLjEzMy0xMS4wNjNjLS4yMzMgNi4zMDItMy45NTEgMTEuMjYtOC4zMDMgMTEuMDYzLTQuMzUtLjE5NS03LjY4NC01LjQ2NS03LjQ0Ni0xMS43Ny4yMzYtNi4zMDUgMy45NTItMTEuMjYgOC4zLTExLjA2NiA0LjM1Mi4xOTcgNy42ODYgNS40NjggNy40NDkgMTEuNzczIi8+PHBhdGggZmlsbD0iI2ZhYWIwNyIgZD0iTTg3Ljk1MiA0OS43MjVDODYuNzkgNDcuMTUgNzUuMDc3IDQ0LjI4IDYwLjU3OCA0NC4yOGgtLjE1NmMtMTQuNSAwLTI2LjIxMiAyLjg3LTI3LjM3NSA1LjQ0NmEuODYzLjg2MyAwIDAwLS4wODUuMzY3Ljg4Ljg4IDAgMDAuMTYuNDk2Yy45OCAxLjQyNyAxMy45ODUgOC40ODcgMjcuMyA4LjQ4N2guMTU2YzEzLjMxNCAwIDI2LjMxOS03LjA1OCAyNy4yOTktOC40ODdhLjg3My44NzMgMCAwMC4xNi0uNDk4Ljg1Ni44NTYgMCAwMC0uMDg1LS4zNjUiLz48cGF0aCBkPSJNNTQuNDM0IDI5Ljg1NGMuMTk5IDIuNDktMS4xNjcgNC43MDItMy4wNDYgNC45NDMtMS44ODMuMjQyLTMuNTY4LTEuNTgtMy43NjgtNC4wNy0uMTk3LTIuNDkyIDEuMTY3LTQuNzA0IDMuMDQzLTQuOTQ0IDEuODg2LS4yNDQgMy41NzQgMS41OCAzLjc3MSA0LjA3bTExLjk1Ni44MzNjLjM4NS0uNjg5IDMuMDA0LTQuMzEyIDguNDI3LTIuOTkzIDEuNDI1LjM0NyAyLjA4NC44NTcgMi4yMjMgMS4wNTcuMjA1LjI5Ni4yNjIuNzE4LjA1MyAxLjI4Ni0uNDEyIDEuMTI2LTEuMjYzIDEuMDk1LTEuNzM0Ljg3NS0uMzA1LS4xNDItNC4wODItMi42Ni03LjU2MiAxLjA5Ny0uMjQuMjU3LS42NjguMzQ2LTEuMDczLjA0LS40MDctLjMwOC0uNTc0LS45My0uMzM0LTEuMzYyIi8+PHBhdGggZmlsbD0iI2ZmZiIgZD0iTTYwLjU3NiA4My4wOGgtLjE1M2MtOS45OTYuMTItMjIuMTE2LTEuMjA0LTMzLjg1NC0zLjUxOC0xLjAwNCA1LjgxOC0xLjYxIDEzLjEzMi0xLjA5IDIxLjg1MyAxLjMxNiAyMi4wNDMgMTQuNDA3IDM1LjkgMzQuNjE0IDM2LjFoLjgyYzIwLjIwOC0uMiAzMy4yOTgtMTQuMDU3IDM0LjYxNi0zNi4xLjUyLTguNzIzLS4wODctMTYuMDM1LTEuMDkyLTIxLjg1NC0xMS43MzkgMi4zMTUtMjMuODYyIDMuNjQtMzMuODYgMy41MTgiLz48cGF0aCBmaWxsPSIjZWIxOTIzIiBkPSJNMzIuMTAyIDgxLjIzNXYyMS42OTNzOS45MzcgMi4wMDQgMTkuODkzLjYxNlY4My41MzVjLTYuMzA3LS4zNTctMTMuMTA5LTEuMTUyLTE5Ljg5My0yLjMiLz48cGF0aCBmaWxsPSIjZWIxOTIzIiBkPSJNMTA1LjUzOSA2MC40MTJzLTE5LjMzIDYuMTAyLTQ0Ljk2MyA2LjI3NWgtLjE1M2MtMjUuNTkxLS4xNzItNDQuODk2LTYuMjU1LTQ0Ljk2Mi02LjI3NUw4Ljk4NyA3Ni41N2MxNi4xOTMgNC44ODIgMzYuMjYxIDguMDI4IDUxLjQzNiA3Ljg0NWguMTUzYzE1LjE3NS4xODMgMzUuMjQyLTIuOTYzIDUxLjQzNy03Ljg0NXptMCAwIi8+PC9zdmc+"
                  alt="QQ"
                  class="h-7 w-7 rounded-md bg-white p-1"
                />
                {{ t("settings.qqGroup") }}
                <ExternalLink class="ml-auto h-3.5 w-3.5 text-muted-foreground" />
              </div>
              <div class="mt-1 font-mono text-base">1087880322</div>
            </button>
            <button
              type="button"
              class="rounded-lg border p-4 text-left transition-colors hover:bg-muted/40 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
              @click="openExternalUrl('https://discord.gg/W7NyVDRt6a')"
            >
              <div class="text-xs font-medium uppercase tracking-wider text-muted-foreground">
                {{ t("settings.community") }}
              </div>
              <div class="mt-3 flex items-center gap-2 text-sm font-medium">
                <img
                  src="https://cdn.simpleicons.org/discord/5865F2"
                  alt="Discord"
                  class="h-7 w-7 rounded-md bg-white p-1"
                />
                Discord
                <ExternalLink class="ml-auto h-3.5 w-3.5 text-muted-foreground" />
              </div>
              <div class="mt-1 text-sm text-primary">discord.gg/W7NyVDRt6a</div>
            </button>
            <button
              type="button"
              class="rounded-lg border p-4 text-left transition-colors hover:bg-muted/40 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
              @click="openExternalUrl('https://github.com/t8y2/dbx')"
            >
              <div class="text-xs font-medium uppercase tracking-wider text-muted-foreground">
                {{ t("settings.project") }}
              </div>
              <div class="mt-3 flex items-center gap-2 text-sm font-medium">
                <img
                  src="https://cdn.simpleicons.org/github/181717"
                  alt="GitHub"
                  class="h-7 w-7 rounded-md bg-white p-1"
                />
                {{ t("settings.openSource") }}
                <ExternalLink class="ml-auto h-3.5 w-3.5 text-muted-foreground" />
              </div>
              <div class="mt-1 text-sm text-primary">github.com/t8y2/dbx</div>
            </button>
            <button
              type="button"
              class="rounded-lg border p-4 text-left transition-colors hover:bg-muted/40 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
              @click="openExternalUrl('https://dbxio.com')"
            >
              <div class="text-xs font-medium uppercase tracking-wider text-muted-foreground">
                {{ t("settings.project") }}
              </div>
              <div class="mt-3 flex items-center gap-2 text-sm font-medium">
                <img src="/logo.png" alt="DBX" class="h-7 w-7 rounded-md" />
                {{ t("settings.officialDocs") }}
                <ExternalLink class="ml-auto h-3.5 w-3.5 text-muted-foreground" />
              </div>
              <div class="mt-1 text-sm text-primary">dbxio.com</div>
            </button>
          </div>

          <DialogFooter class="border-t-0 bg-transparent">
            <Button variant="outline" @click="emit('update:open', false)">
              {{ t("common.close") }}
            </Button>
          </DialogFooter>
        </TabsContent>
      </Tabs>
    </DialogContent>
  </Dialog>
</template>
