import { invoke } from "@tauri-apps/api/core";
import { escapeHtml } from "./dom";
import type { AccountsPresentation, ApiProbeResult, LocalProviderSyncPresentation, ProviderCount } from "./types";

export function renderAccounts(
  presentation: AccountsPresentation,
  onUpdated: (next: AccountsPresentation) => void,
  providerSync: LocalProviderSyncPresentation | null,
  onProviderSyncUpdated: (next: LocalProviderSyncPresentation) => void,
): string {
  queueMicrotask(() => bindAccountControls(onUpdated, onProviderSyncUpdated));
  const providerMode = presentation.providerMode;
  const providerModeBanner = providerMode
    ? `<div class="provider-mode-banner">
        <span>${escapeHtml(presentation.labels.providerModeActive)}: ${escapeHtml(providerMode.providerDisplayName)}</span>
        <button data-action="exit-provider">${escapeHtml(presentation.labels.switchBackFromProvider)}</button>
      </div>`
    : "";
  const rows = presentation.rows.length
    ? presentation.rows.map((row) => `
      <div class="account-row" data-account-id="${escapeHtml(row.id)}">
        <div>
          <strong>${escapeHtml(row.displayName)}</strong>
          <span>${escapeHtml(row.kind === "chatGpt" ? "ChatGPT" : "API")}</span>
          <small>${escapeHtml(row.status)}</small>
        </div>
        <div class="account-actions">
          ${!providerMode ? `<button data-action="activate" data-account-id="${escapeHtml(row.id)}">${escapeHtml(presentation.labels.activate)}</button>` : ""}
          ${row.kind === "api" && !providerMode ? `<button data-action="provider" data-account-id="${escapeHtml(row.id)}">${escapeHtml(presentation.labels.switchToProvider)}</button>` : ""}
          <button data-action="rename" data-account-id="${escapeHtml(row.id)}">${escapeHtml(presentation.labels.rename)}</button>
          <button data-action="forget" data-account-id="${escapeHtml(row.id)}">${escapeHtml(presentation.labels.forget)}</button>
        </div>
      </div>
    `).join("")
    : `<p class="settings-status">${escapeHtml(presentation.labels.noSavedAccounts)}</p>`;

  return `
    <section class="accounts-panel">
      ${providerModeBanner}
      <div class="accounts-toolbar">
        <button id="importChatGpt">${escapeHtml(presentation.labels.signInWithChatgpt)}</button>
        <button id="showApiForm">${escapeHtml(presentation.labels.addApiAccount)}</button>
        <button id="rollbackLastChange">${escapeHtml(presentation.labels.rollbackLastChange)}</button>
        <button id="repairNow">${escapeHtml(presentation.labels.repairNow)}</button>
        <button id="openVaultFolder">${escapeHtml(presentation.labels.openVaultFolder)}</button>
      </div>
      <form id="apiAccountForm" class="api-form" hidden>
        <label>Display name<input id="apiDisplayName" /></label>
        <label>API key<input id="apiKey" /></label>
        <label>Base URL<input id="apiBaseUrl" placeholder="https://api.openai.com/v1" /></label>
        <div class="api-form-probe">
          <button type="button" id="probeApi">Detect</button>
        </div>
        <label>Model<input id="apiModel" /></label>
        <label>Provider name<input id="apiProviderName" /></label>
        <button type="submit">${escapeHtml(presentation.labels.addApiAccount)}</button>
      </form>
      <div class="account-list">${rows}</div>
      ${renderProviderSync(providerSync)}
      <p id="accountsStatus" class="settings-status">${escapeHtml(presentation.message ?? "")}</p>
    </section>
  `;
}

function renderProviderSync(sync: LocalProviderSyncPresentation | null): string {
  const rollout = sync ? renderProviderCounts(sync.rolloutProviders) : "";
  const threads = sync ? renderProviderCounts(sync.threadProviders) : "";
  return `
    <section class="provider-sync-panel">
      <div class="provider-sync-header">
        <div>
          <strong>${escapeHtml(sync?.title ?? "Local Provider Sync")}</strong>
          <small>${escapeHtml(sync?.expectedProvider ? `Expected: ${sync.expectedProvider}` : "Expected: -")}</small>
        </div>
        <button id="inspectProviderSync">Inspect</button>
      </div>
      <div class="provider-sync-grid">
        <div>
          <span>Rollout</span>
          ${rollout || "<small>-</small>"}
        </div>
        <div>
          <span>Threads</span>
          ${threads || "<small>-</small>"}
        </div>
      </div>
      <p class="settings-status">${escapeHtml(sync?.status ?? "Run Inspect to compare local provider metadata.")}</p>
      ${sync?.threadIssue ? `<p class="settings-status">${escapeHtml(sync.threadIssue)}</p>` : ""}
    </section>
  `;
}

function renderProviderCounts(counts: ProviderCount[]): string {
  if (!counts.length) {
    return "<small>-</small>";
  }
  return counts
    .map((count) => `<small>${escapeHtml(count.providerId || "(blank)")}: ${count.count}</small>`)
    .join("");
}

function bindAccountControls(
  onUpdated: (next: AccountsPresentation) => void,
  onProviderSyncUpdated: (next: LocalProviderSyncPresentation) => void,
): void {
  document.querySelector("#importChatGpt")?.addEventListener("click", async () => {
    onUpdated(await invoke<AccountsPresentation>("import_current_chatgpt_account", { displayName: null }));
  });
  document.querySelector("#showApiForm")?.addEventListener("click", () => {
    const form = document.querySelector<HTMLFormElement>("#apiAccountForm");
    if (form) {
      form.hidden = !form.hidden;
    }
  });
  document.querySelector("#openVaultFolder")?.addEventListener("click", async () => {
    await invoke("open_vault_folder");
  });
  document.querySelector("#rollbackLastChange")?.addEventListener("click", async () => {
    if (confirm("Rollback the latest safe switch restore point?")) {
      onUpdated(await invoke<AccountsPresentation>("rollback_last_change"));
    }
  });
  document.querySelector("#repairNow")?.addEventListener("click", async () => {
    onUpdated(await invoke<AccountsPresentation>("repair_now"));
  });
  document.querySelector("#inspectProviderSync")?.addEventListener("click", async () => {
    onProviderSyncUpdated(await invoke<LocalProviderSyncPresentation>("inspect_local_provider_sync"));
  });
  document.querySelector("#probeApi")?.addEventListener("click", async () => {
    const apiKey = document.querySelector<HTMLInputElement>("#apiKey")?.value ?? "";
    const baseUrl = document.querySelector<HTMLInputElement>("#apiBaseUrl")?.value ?? "";
    const status = document.querySelector<HTMLParagraphElement>("#accountsStatus");
    if (status) status.textContent = "Probing...";
    
    try {
      const result = await invoke<ApiProbeResult>("probe_api_account", { apiKey, baseUrl });
      const displayNameField = document.querySelector<HTMLInputElement>("#apiDisplayName");
      if (displayNameField && !displayNameField.value) {
        displayNameField.value = result.suggestedDisplayName;
      }
      const baseUrlField = document.querySelector<HTMLInputElement>("#apiBaseUrl");
      if (baseUrlField) {
        baseUrlField.value = result.normalizedBaseUrl;
      }
      const modelField = document.querySelector<HTMLInputElement>("#apiModel");
      if (modelField && !modelField.value && result.modelIds.length > 0) {
        modelField.value = result.modelIds[0];
      }
      const providerNameField = document.querySelector<HTMLInputElement>("#apiProviderName");
      if (providerNameField && !providerNameField.value) {
        providerNameField.value = result.suggestedDisplayName.toLowerCase();
      }
      if (status) status.textContent = "Detected: " + result.modelIds.length + " models found.";
    } catch (error) {
      if (status) status.textContent = "Detection failed: " + error;
    }
  });
  document.querySelector("#apiAccountForm")?.addEventListener("submit", async (event) => {
    event.preventDefault();
    onUpdated(await invoke<AccountsPresentation>("add_api_account", {
      input: {
        displayName: document.querySelector<HTMLInputElement>("#apiDisplayName")?.value ?? "",
        apiKey: document.querySelector<HTMLInputElement>("#apiKey")?.value ?? "",
        baseUrl: document.querySelector<HTMLInputElement>("#apiBaseUrl")?.value ?? "",
        model: document.querySelector<HTMLInputElement>("#apiModel")?.value || null,
        providerName: document.querySelector<HTMLInputElement>("#apiProviderName")?.value || null,
      },
    }));
  });
  const actionButtons = document.querySelectorAll<HTMLButtonElement>("[data-action]");
  for (let i = 0; i < actionButtons.length; i++) {
    const button = actionButtons[i];
    button.addEventListener("click", async () => {
      const accountId = button.dataset.accountId ?? "";
      const action = button.dataset.action;
      if (action === "activate" && confirm("Safely switch to this account? A restore point will be created first.")) {
        onUpdated(await invoke<AccountsPresentation>("activate_account", { accountId }));
      }
      if (action === "rename") {
        const displayName = prompt("Rename account");
        if (displayName) {
          onUpdated(await invoke<AccountsPresentation>("rename_account", { accountId, displayName }));
        }
      }
      if (action === "provider" && confirm("Use this API account as the third-party Provider for the current ChatGPT login? This backs up and updates auth.json/config.toml.")) {
        onUpdated(await invoke<AccountsPresentation>("enter_provider_mode", { accountId }));
      }
      if (action === "exit-provider" && confirm("Switch back from third-party Provider mode and restore the previous auth.json/config.toml?")) {
        onUpdated(await invoke<AccountsPresentation>("exit_provider_mode"));
      }
      if (action === "forget" && confirm("Forget this account?")) {
        onUpdated(await invoke<AccountsPresentation>("forget_account", { accountId }));
      }
    });
  }
}
