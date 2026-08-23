import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { renderAccounts } from "./accounts-view";
import { escapeHtml } from "./dom";
import { renderGeneralSettings } from "./settings-view";
import type {
  AccountsPresentation,
  LocalProviderSyncPresentation,
  SettingsPresentation,
} from "./types";
import { mountWidget } from "./widget-view";

const app = document.querySelector<HTMLDivElement>("#app");

if (!app) {
  throw new Error("Missing #app element");
}

const query = new URLSearchParams(window.location.search);
const previewWidget = query.get("preview") === "widget";
const requestedView = query.get("view");
const isTauri = "__TAURI_INTERNALS__" in window;
const windowLabel = isTauri ? getCurrentWindow().label : "main";
const isWidget = previewWidget || requestedView === "widget" || windowLabel === "widget";

if (isWidget) {
  void mountWidget(app, { preview: previewWidget });
} else {
  mountSettings(app);
}

function mountSettings(root: HTMLDivElement): void {
  document.body.dataset.view = "settings";
  document.documentElement.dataset.view = "settings";
  let settingsPresentation: SettingsPresentation | null = null;
  let accountsPresentation: AccountsPresentation | null = null;
  let localProviderSync: LocalProviderSyncPresentation | null = null;
  let activeTab: "general" | "accounts" = "general";

  function render(): void {
    if (!settingsPresentation || !accountsPresentation) {
      return;
    }
    document.title = settingsPresentation.labels.title;
    document.documentElement.lang = settingsPresentation.resolvedLanguage === "chinese" ? "zh-CN" : "en";
    const body =
      activeTab === "general"
        ? renderGeneralSettings(settingsPresentation, (next) => {
            settingsPresentation = next;
            render();
          })
        : renderAccounts(
            accountsPresentation,
            (next) => {
              accountsPresentation = next;
              render();
            },
            localProviderSync,
            (next) => {
              localProviderSync = next;
              render();
            },
          );

    root.innerHTML = `
      <main class="settings-shell">
        <header class="settings-header">
          <h1>${escapeHtml(settingsPresentation.labels.title)}</h1>
          <nav class="settings-tabs">
            <button id="tabGeneral" class="${activeTab === "general" ? "active" : ""}">General</button>
            <button id="tabAccounts" class="${activeTab === "accounts" ? "active" : ""}">${escapeHtml(accountsPresentation.labels.accounts)}</button>
          </nav>
        </header>
        ${body}
      </main>
    `;
    document.querySelector("#tabGeneral")?.addEventListener("click", () => {
      activeTab = "general";
      render();
    });
    document.querySelector("#tabAccounts")?.addEventListener("click", () => {
      activeTab = "accounts";
      render();
    });
  }

  async function load(): Promise<void> {
    try {
      settingsPresentation = await invoke<SettingsPresentation>("get_settings");
      accountsPresentation = await invoke<AccountsPresentation>("get_accounts");
      render();
    } catch (error) {
      root.textContent = String(error);
    }
  }

  void load();
}
