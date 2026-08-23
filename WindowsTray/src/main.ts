import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
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
  let isLoading = false;

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

  function renderError(errorMessage: string): void {
    root.innerHTML = `
      <main class="settings-shell settings-shell--error" style="display:flex; flex-direction:column; align-items:center; justify-content:center; height:100%; padding:24px; gap:16px; text-align:center;">
        <div style="color:var(--text-danger, #ef4444); font-size:14px; word-break:break-word;">
          ${escapeHtml(errorMessage)}
        </div>
        <button id="retrySettingsBtn" class="primary-button" style="padding:6px 16px; cursor:pointer;">
          Retry / 重试
        </button>
      </main>
    `;
    document.querySelector("#retrySettingsBtn")?.addEventListener("click", () => {
      void load();
    });
  }

  async function load(): Promise<void> {
    if (isLoading) return;
    isLoading = true;
    try {
      const [settings, accounts] = await Promise.all([
        invoke<SettingsPresentation>("get_settings"),
        invoke<AccountsPresentation>("get_accounts"),
      ]);
      settingsPresentation = settings;
      accountsPresentation = accounts;
      render();
    } catch (error) {
      renderError(String(error));
    } finally {
      isLoading = false;
    }
  }

  if (isTauri) {
    void listen("settings-shown", () => {
      void load();
    });
  }

  window.addEventListener("focus", () => {
    if (!settingsPresentation) {
      void load();
    }
  });

  void load();
}
