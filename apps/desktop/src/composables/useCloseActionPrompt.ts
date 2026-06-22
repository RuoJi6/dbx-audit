import { ref } from "vue";
import { useSettingsStore } from "@/stores/settingsStore";
import { isTauriRuntime } from "@/lib/tauriRuntime";

export function useCloseActionPrompt() {
  const settingsStore = useSettingsStore();
  const showCloseActionPrompt = ref(false);
  const unlistenHandles: Array<() => void> = [];

  async function applyCloseChoice(quitOnClose: boolean) {
    showCloseActionPrompt.value = false;
    await settingsStore.updateDesktopSettings({
      quit_on_close: quitOnClose,
      close_action_prompted: true,
    });
    if (!isTauriRuntime()) return;
    if (quitOnClose) {
      const { exit } = await import("@tauri-apps/plugin-process");
      await exit(0);
      return;
    }
    const { getCurrentWindow } = await import("@tauri-apps/api/window");
    await getCurrentWindow().hide();
  }

  function chooseQuit() {
    void applyCloseChoice(true);
  }

  function chooseMinimize() {
    void applyCloseChoice(false);
  }

  function setupCloseActionPromptListener() {
    if (!isTauriRuntime()) return;
    void import("@tauri-apps/api/event").then(({ listen }) => {
      listen("dbx-close-action-prompt", () => {
        showCloseActionPrompt.value = true;
      }).then((unlisten) => unlistenHandles.push(unlisten));
    });
  }

  function cleanupCloseActionPromptListener() {
    unlistenHandles.forEach((unlisten) => unlisten());
    unlistenHandles.length = 0;
  }

  return {
    showCloseActionPrompt,
    chooseQuit,
    chooseMinimize,
    setupCloseActionPromptListener,
    cleanupCloseActionPromptListener,
  };
}
