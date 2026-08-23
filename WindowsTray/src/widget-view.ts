import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { escapeHtml } from "./dom";
import type {
  AccountsPresentation,
  DashboardAccount,
  DashboardLabels,
  DashboardQuotaWindow,
  DashboardState,
} from "./types";

type WidgetOptions = {
  preview: boolean;
};

type WaitingSwitch = {
  accountId: string;
  targetRefreshRevision: number | null;
};

type FocusAnchor = {
  command: string | null;
  accountId: string | null;
};

let dashboard: DashboardState | null = null;
let waitingSwitch: WaitingSwitch | null = null;
let busyAction: "repair" | null = null;
let toast: { kind: "success" | "error"; message: string } | null = null;
let previewMode = false;
let switchTimeout: number | null = null;

export async function mountWidget(root: HTMLDivElement, options: WidgetOptions): Promise<void> {
  previewMode = options.preview;
  document.body.dataset.view = "widget";
  document.documentElement.dataset.view = "widget";
  document.documentElement.lang = "en";

  document.addEventListener("keydown", (event) => {
    if (event.key === "Escape" && !previewMode) {
      event.preventDefault();
      void animateAndHide();
    }
  });

  if (previewMode) {
    dashboard = previewDashboard();
    document.documentElement.lang = dashboard.language === "chinese" ? "zh-CN" : "en";
    renderWidget(root);
    restartEntranceAnimation();
    return;
  }

  renderInitialLoading(root);
  await listen<DashboardState>("dashboard-state-changed", (event) => {
    ingestDashboard(event.payload, root);
  });
  await listen("widget-shown", () => {
    restartEntranceAnimation();
  });

  try {
    ingestDashboard(await invoke<DashboardState>("get_dashboard_state"), root);
  } catch (error) {
    dashboard = fallbackErrorDashboard(String(error));
    renderWidget(root);
  }

  window.setInterval(() => {
    if (dashboard) renderWidget(root);
  }, 30_000);
}

function ingestDashboard(next: DashboardState, root: HTMLDivElement): void {
  if (next.schemaVersion !== 1) {
    dashboard = fallbackErrorDashboard(`Unsupported dashboard state version: ${String(next.schemaVersion)}`);
    renderWidget(root);
    return;
  }
  if (dashboard && next.stateRevision < dashboard.stateRevision) {
    return;
  }

  dashboard = next;
  document.documentElement.lang = next.language === "chinese" ? "zh-CN" : "en";

  if (waitingSwitch) {
    const targetIsActive = next.accounts.some(
      (account) => account.id === waitingSwitch?.accountId && account.isActive,
    );
    const reachedTerminalSnapshot = waitingSwitch.targetRefreshRevision !== null
      && next.refreshCompletedRevision >= waitingSwitch.targetRefreshRevision
      && !next.isRefreshing;
    if (targetIsActive && reachedTerminalSnapshot) {
      waitingSwitch = null;
      clearSwitchTimeout();
    }
  }

  renderWidget(root);
}

function renderInitialLoading(root: HTMLDivElement): void {
  root.innerHTML = `
    <main class="widget-shell widget-shell--initial" aria-busy="true">
      <div class="widget-initial-spinner" aria-label="Loading"></div>
    </main>
  `;
}

function renderWidget(root: HTMLDivElement): void {
  if (!dashboard) return;
  const state = dashboard;
  const previousScrollTop = root.querySelector<HTMLElement>(".widget-content")?.scrollTop ?? 0;
  const focusAnchor = captureFocusAnchor(root);
  const current = state.currentAccount;
  const refreshBusy = state.isRefreshing;

  root.innerHTML = `
    <main class="widget-shell" tabindex="-1" aria-label="${escapeHtml(state.labels.title)}">
      <header class="widget-header">
        <div class="widget-brand">
          <span class="widget-brand-mark" aria-hidden="true">${brandIcon()}</span>
          <h1>${escapeHtml(state.labels.title)}</h1>
        </div>
        <div class="widget-header-actions">
          <button class="widget-icon-button${refreshBusy ? " is-spinning" : ""}" type="button"
            data-command="refresh" aria-label="${escapeHtml(refreshBusy ? state.labels.refreshing : state.labels.refresh)}"
            title="${escapeHtml(state.labels.refresh)}"${refreshBusy ? " disabled" : ""}>
            ${icon("refresh")}
          </button>
          <button class="widget-icon-button" type="button" data-command="settings"
            aria-label="${escapeHtml(state.labels.settings)}" title="${escapeHtml(state.labels.settings)}">
            ${icon("settings")}
          </button>
        </div>
      </header>

      <div class="widget-content">
        <section class="current-account" aria-label="${escapeHtml(state.labels.activeAccount)}">
          <span class="account-avatar account-avatar--${current?.kind === "api" ? "api" : "chatgpt"}" aria-hidden="true">
            ${icon("user")}
          </span>
          <div class="current-account-copy">
            <strong>${escapeHtml(current?.displayName ?? state.labels.quotaUnavailable)}</strong>
            <span>${escapeHtml(current?.detail ?? state.labels.activeAccount)}${current ? '<i class="live-dot" aria-hidden="true"></i>' : ""}</span>
          </div>
        </section>

        ${renderQuota(state)}

        <section class="accounts-section" aria-labelledby="accountsHeading">
          <h2 id="accountsHeading">${escapeHtml(state.labels.accounts)}</h2>
          ${renderAccounts(state.accounts, state.labels)}
        </section>

        <section class="widget-tools" aria-label="${escapeHtml(state.labels.settings)}">
          ${toolRow("session", state.labels.sessionManager, "session-manager")}
          ${toolRow("repair", busyAction === "repair" ? state.labels.repairing : state.labels.repairNow, "repair", busyAction === "repair")}
          ${toolRow("folder", state.labels.openCodexFolder, "codex-folder")}
        </section>

        ${state.notice ? `<p class="widget-notice" role="status">${icon("info")}<span>${escapeHtml(state.notice)}</span></p>` : ""}
        ${toast ? `<p class="widget-toast widget-toast--${toast.kind}" role="status">${toast.kind === "success" ? icon("check") : icon("alert")}<span>${escapeHtml(toast.message)}</span></p>` : ""}
        <footer class="widget-footer">${escapeHtml(updatedText(state))}</footer>
      </div>
    </main>
  `;

  const content = root.querySelector<HTMLElement>(".widget-content");
  if (content) content.scrollTop = previousScrollTop;
  bindWidgetControls(root);
  restoreFocusAnchor(root, focusAnchor);
}

function renderQuota(state: DashboardState): string {
  const staleWarning = state.error && state.quotaWindows.length
    ? `<div class="quota-warning" role="status">${icon("alert")}<span>${escapeHtml(state.error)}</span></div>`
    : "";

  if (state.status === "loading" && !state.quotaWindows.length) {
    return `
      <section class="quota-section" aria-busy="true" aria-label="${escapeHtml(state.labels.quotaLoading)}">
        <div class="quota-skeleton"><span></span><strong></strong><i></i><small></small></div>
        <div class="quota-skeleton"><span></span><strong></strong><i></i><small></small></div>
        <p class="visually-hidden">${escapeHtml(state.labels.quotaLoading)}</p>
      </section>
    `;
  }

  if (state.status === "error" && !state.quotaWindows.length) {
    return `
      <section class="quota-state quota-state--error" role="alert">
        <span class="quota-state-icon">${icon("alert")}</span>
        <div><strong>${escapeHtml(state.labels.quotaUnavailable)}</strong><p>${escapeHtml(state.error ?? state.labels.quotaUnavailable)}</p></div>
        <button type="button" data-command="refresh">${escapeHtml(state.labels.tryAgain)}</button>
      </section>
    `;
  }

  if (state.status === "empty" || !state.quotaWindows.length) {
    const emptyMessage = state.error ?? state.labels.noQuotaWindows;
    return `
      <section class="quota-state${state.error ? " quota-state--error" : ""}" role="${state.error ? "alert" : "status"}">
        <span class="quota-state-icon">${icon(state.error ? "alert" : "clock")}</span>
        <div><strong>${escapeHtml(state.labels.quotaUnavailable)}</strong><p>${escapeHtml(emptyMessage)}</p></div>
        <button type="button" data-command="refresh">${escapeHtml(state.labels.tryAgain)}</button>
      </section>
    `;
  }

  return `
    <section class="quota-section" aria-label="${escapeHtml(state.labels.quota)}"${state.isRefreshing ? ' aria-busy="true"' : ""}>
      ${staleWarning}
      ${state.quotaWindows.map((window, index) => quotaCard(window, state, index)).join("")}
    </section>
  `;
}

function quotaCard(window: DashboardQuotaWindow, state: DashboardState, index: number): string {
  const percent = Math.round(Math.max(0, Math.min(100, window.remainingPercent)));
  const severity = percent <= 20 ? "critical" : percent <= 40 ? "low" : "healthy";
  const quotaIcon = window.label.toLowerCase().includes("w") || index > 0 ? "calendar" : "clock";
  return `
    <article class="quota-card quota-card--${severity}">
      <div class="quota-card-title">
        <span aria-hidden="true">${icon(quotaIcon)}</span>
        <h2>${escapeHtml(window.displayLabel)}</h2>
      </div>
      <p class="quota-value"><strong>${percent}%</strong><span>${escapeHtml(state.labels.remaining)}</span></p>
      <div class="quota-progress" role="progressbar" aria-label="${escapeHtml(window.displayLabel)}"
        aria-valuemin="0" aria-valuemax="100" aria-valuenow="${percent}">
        <span style="width:${percent}%"></span>
      </div>
      <p class="quota-reset">${escapeHtml(resetText(window.resetAt, state))}</p>
    </article>
  `;
}

function renderAccounts(accounts: DashboardAccount[], labels: DashboardLabels): string {
  if (!accounts.length) {
    return `
      <div class="accounts-empty">
        <p>${escapeHtml(labels.noSavedAccounts)}</p>
        <button type="button" data-command="settings">${escapeHtml(labels.settings)}</button>
      </div>
    `;
  }

  return `
    <div class="account-list" role="list">
      ${accounts.map((account) => {
        const switching = waitingSwitch?.accountId === account.id;
        return `
          <div class="widget-account-row${account.isActive ? " is-active" : ""}" role="listitem">
            <span class="account-avatar account-avatar--small account-avatar--${account.kind === "api" ? "api" : "chatgpt"}" aria-hidden="true">${icon("user")}</span>
            <div class="widget-account-copy">
              <strong>${escapeHtml(account.displayName)}</strong>
              <span>${account.kind === "chatGpt" ? "ChatGPT" : "API"} · ${escapeHtml(account.status)}</span>
            </div>
            ${switching
              ? `<button class="account-switch" type="button" data-account-id="${escapeHtml(account.id)}" aria-disabled="true">
                  <span class="button-spinner" aria-hidden="true"></span>${escapeHtml(labels.switching)}
                </button>`
              : account.isActive
                ? `<span class="account-current" aria-label="${escapeHtml(labels.current)}">${icon("check")}</span>`
                : `<button class="account-switch" type="button" data-account-id="${escapeHtml(account.id)}"${waitingSwitch ? " disabled" : ""}>
                    ${escapeHtml(labels.switchAccount)}
                  </button>`}
          </div>
        `;
      }).join("")}
    </div>
  `;
}

function toolRow(iconName: string, label: string, command: string, busy = false): string {
  return `
    <button class="tool-row" type="button" data-command="${command}"${busy ? " disabled" : ""}>
      <span class="tool-row-icon" aria-hidden="true">${busy ? '<span class="button-spinner"></span>' : icon(iconName)}</span>
      <span>${escapeHtml(label)}</span>
      <span class="tool-row-chevron" aria-hidden="true">${icon("chevron")}</span>
    </button>
  `;
}

function bindWidgetControls(root: HTMLDivElement): void {
  root.querySelectorAll<HTMLButtonElement>("[data-command]").forEach((button) => {
    button.addEventListener("click", () => void runCommand(button.dataset.command ?? "", root));
  });
  root.querySelectorAll<HTMLButtonElement>("[data-account-id]").forEach((button) => {
    button.addEventListener("click", () => void switchAccount(button.dataset.accountId ?? "", root));
  });
}

function captureFocusAnchor(root: HTMLDivElement): FocusAnchor | null {
  const active = document.activeElement;
  if (!(active instanceof HTMLElement) || !root.contains(active)) return null;
  return {
    command: active.dataset.command ?? null,
    accountId: active.dataset.accountId ?? null,
  };
}

function restoreFocusAnchor(root: HTMLDivElement, anchor: FocusAnchor | null): void {
  if (!anchor) return;
  const candidates = Array.from(root.querySelectorAll<HTMLElement>("[data-command], [data-account-id]"));
  const target = candidates.find((element) => (
    (anchor.command !== null && element.dataset.command === anchor.command)
    || (anchor.accountId !== null && element.dataset.accountId === anchor.accountId)
  ));
  if (target instanceof HTMLButtonElement && !target.disabled) {
    target.focus({ preventScroll: true });
    return;
  }
  root.querySelector<HTMLElement>(".widget-shell")?.focus({ preventScroll: true });
}

async function runCommand(command: string, root: HTMLDivElement): Promise<void> {
  if (!dashboard) return;
  toast = null;

  if (previewMode) {
    runPreviewCommand(command, root);
    return;
  }

  try {
    if (command === "refresh") {
      dashboard = { ...dashboard, isRefreshing: true };
      renderWidget(root);
      await invoke("widget_refresh");
      return;
    }
    if (command === "settings") {
      await invoke("widget_open_settings");
      return;
    }
    if (command === "session-manager") {
      await invoke("widget_open_session_manager");
      return;
    }
    if (command === "codex-folder") {
      await invoke("widget_open_codex_folder");
      return;
    }
    if (command === "repair") {
      busyAction = "repair";
      renderWidget(root);
      await invoke<AccountsPresentation>("repair_now");
      busyAction = null;
      toast = { kind: "success", message: dashboard.labels.repairComplete };
      renderWidget(root);
    }
  } catch (error) {
    busyAction = null;
    if (dashboard) dashboard = { ...dashboard, isRefreshing: false };
    toast = { kind: "error", message: `${dashboard?.labels.actionFailed ?? "Action failed"}: ${String(error)}` };
    renderWidget(root);
  }
}

async function switchAccount(accountId: string, root: HTMLDivElement): Promise<void> {
  if (!dashboard || waitingSwitch || !accountId) return;
  const account = dashboard.accounts.find((row) => row.id === accountId);
  if (!account || account.isActive) return;
  if (!window.confirm(dashboard.labels.switchConfirm)) return;

  toast = null;
  waitingSwitch = {
    accountId,
    targetRefreshRevision: null,
  };
  renderWidget(root);

  if (previewMode) {
    window.setTimeout(() => {
      if (!dashboard) return;
      dashboard = {
        ...dashboard,
        status: account.kind === "api" ? "empty" : "ready",
        currentAccount: {
          displayName: account.displayName,
          detail: account.kind === "chatGpt" ? "personal@example.com" : dashboard.labels.providerActive,
          kind: account.kind,
        },
        quotaWindows: account.kind === "api"
          ? []
          : previewQuotaWindows(dashboard.language === "chinese", Date.now()),
        accounts: dashboard.accounts.map((row) => {
          const isActive = row.id === accountId;
          return {
            ...row,
            isActive,
            status: isActive ? dashboard!.labels.current : dashboard!.labels.available,
          };
        }),
        fetchedAt: new Date().toISOString(),
        error: null,
        refreshRequestedRevision: dashboard.refreshRequestedRevision + 1,
        refreshCompletedRevision: dashboard.refreshRequestedRevision + 1,
      };
      waitingSwitch = null;
      renderWidget(root);
    }, 700);
    return;
  }

  try {
    await invoke<AccountsPresentation>("activate_account", { accountId });
    const afterActivation = await invoke<DashboardState>("get_dashboard_state");
    if (waitingSwitch) {
      waitingSwitch.targetRefreshRevision = afterActivation.refreshRequestedRevision;
    }
    ingestDashboard(afterActivation, root);
    armSwitchFallback(root);
  } catch (error) {
    waitingSwitch = null;
    clearSwitchTimeout();
    toast = { kind: "error", message: `${dashboard.labels.actionFailed}: ${String(error)}` };
    renderWidget(root);
  }
}

function armSwitchFallback(root: HTMLDivElement): void {
  clearSwitchTimeout();
  switchTimeout = window.setTimeout(async () => {
    if (!waitingSwitch) return;
    try {
      ingestDashboard(await invoke<DashboardState>("get_dashboard_state"), root);
    } catch {
      // The event stream remains authoritative; this is only a missed-event fallback.
    }
    if (!waitingSwitch || !dashboard) return;

    const target = waitingSwitch.targetRefreshRevision;
    if (target === null || dashboard.isRefreshing || dashboard.refreshCompletedRevision < target) {
      armSwitchFallback(root);
      return;
    }

    waitingSwitch = null;
    toast = { kind: "error", message: dashboard.labels.quotaUnavailable };
    renderWidget(root);
  }, 30_000);
}

function clearSwitchTimeout(): void {
  if (switchTimeout !== null) {
    window.clearTimeout(switchTimeout);
    switchTimeout = null;
  }
}

function runPreviewCommand(command: string, root: HTMLDivElement): void {
  if (!dashboard) return;
  if (command === "refresh") {
    dashboard = { ...dashboard, isRefreshing: true };
    renderWidget(root);
    window.setTimeout(() => {
      if (!dashboard) return;
      dashboard = { ...dashboard, isRefreshing: false, fetchedAt: new Date().toISOString(), error: null };
      renderWidget(root);
    }, 650);
  } else if (command === "repair") {
    busyAction = "repair";
    renderWidget(root);
    window.setTimeout(() => {
      if (!dashboard) return;
      busyAction = null;
      toast = { kind: "success", message: dashboard.labels.repairComplete };
      renderWidget(root);
    }, 700);
  }
}

async function animateAndHide(): Promise<void> {
  document.body.classList.add("widget-is-leaving");
  await new Promise((resolve) => window.setTimeout(resolve, 120));
  await invoke("widget_hide");
  document.body.classList.remove("widget-is-leaving");
}

function restartEntranceAnimation(): void {
  document.body.classList.remove("widget-is-entering", "widget-is-leaving");
  void document.body.offsetWidth;
  document.body.classList.add("widget-is-entering");
  window.setTimeout(() => document.body.classList.remove("widget-is-entering"), 320);
}

function resetText(resetAt: string | null, state: DashboardState): string {
  if (!resetAt) return state.labels.resetUnavailable;
  const target = new Date(resetAt);
  if (Number.isNaN(target.getTime())) return state.labels.resetUnavailable;
  const milliseconds = target.getTime() - Date.now();
  const locale = state.language === "chinese" ? "zh-CN" : "en-US";

  if (milliseconds > 0 && milliseconds < 86_400_000) {
    const totalMinutes = Math.max(1, Math.ceil(milliseconds / 60_000));
    const hours = Math.floor(totalMinutes / 60);
    const minutes = totalMinutes % 60;
    const duration = state.language === "chinese"
      ? `${hours ? `${hours} 小时` : ""}${hours && minutes ? " " : ""}${minutes ? `${minutes} 分钟` : ""}`
      : `${hours ? `${hours}h` : ""}${hours && minutes ? " " : ""}${minutes ? `${minutes}m` : ""}`;
    return `${state.labels.resetsIn} ${duration}`;
  }

  return `${state.labels.resetsAt} ${new Intl.DateTimeFormat(locale, {
    weekday: "short",
    hour: "2-digit",
    minute: "2-digit",
  }).format(target)}`;
}

function updatedText(state: DashboardState): string {
  if (!state.fetchedAt) return state.labels.neverUpdated;
  const fetched = new Date(state.fetchedAt);
  if (Number.isNaN(fetched.getTime())) return state.labels.neverUpdated;
  const minutes = Math.max(0, Math.floor((Date.now() - fetched.getTime()) / 60_000));
  if (minutes < 1) return state.labels.updatedJustNow;
  const locale = state.language === "chinese" ? "zh-CN" : "en-US";
  return `${state.labels.updated} ${new Intl.RelativeTimeFormat(locale, { numeric: "always" }).format(-minutes, "minute")}`;
}

function previewDashboard(): DashboardState {
  const params = new URLSearchParams(window.location.search);
  const chinese = params.get("lang") === "zh";
  const stateName = params.get("state");
  const labels = previewLabels(chinese);
  const now = Date.now();
  const base: DashboardState = {
    schemaVersion: 1,
    stateRevision: 1,
    status: "ready",
    language: chinese ? "chinese" : "english",
    labels,
    currentAccount: { displayName: chinese ? "ChatGPT 个人账号" : "ChatGPT Personal", detail: "personal@example.com", kind: "chatGpt" },
    quotaWindows: previewQuotaWindows(chinese, now),
    accounts: [
      { id: "chatgpt-personal", displayName: chinese ? "ChatGPT 个人账号" : "ChatGPT Personal", kind: "chatGpt", isActive: true, status: labels.current },
      { id: "api-workspace", displayName: chinese ? "API 工作区" : "API Workspace", kind: "api", isActive: false, status: labels.available },
    ],
    isRefreshing: false,
    refreshRequestedRevision: 1,
    refreshCompletedRevision: 1,
    fetchedAt: new Date(now - 20_000).toISOString(),
    error: null,
    notice: null,
  };

  if (stateName === "loading") return { ...base, status: "loading", quotaWindows: [], fetchedAt: null, currentAccount: null };
  if (stateName === "empty") return { ...base, status: "empty", quotaWindows: [] };
  if (stateName === "error") return { ...base, status: "error", quotaWindows: [], fetchedAt: null, error: chinese ? "Codex 登录已失效，请重新登录。" : "Your Codex sign-in has expired. Sign in again." };
  if (stateName === "stale") return { ...base, error: chinese ? "刷新失败，正在显示上次成功读取的数据。" : "Refresh failed. Showing the last successful reading." };
  return base;
}

function previewQuotaWindows(chinese: boolean, now: number): DashboardQuotaWindow[] {
  return [
    { label: "5h", displayLabel: chinese ? "5 小时额度" : "5-hour limit", remainingPercent: 78, resetAt: new Date(now + 2 * 3_600_000 + 14 * 60_000).toISOString() },
    { label: "1w", displayLabel: chinese ? "每周额度" : "Weekly limit", remainingPercent: 46, resetAt: new Date(now + 3 * 86_400_000).toISOString() },
  ];
}

function previewLabels(chinese: boolean): DashboardLabels {
  const pair = (english: string, zh: string) => chinese ? zh : english;
  return {
    title: "Codex Quota Viewer",
    activeAccount: pair("Active account", "当前账号"),
    refreshing: pair("Refreshing quota", "正在刷新额度"),
    refresh: pair("Refresh", "刷新"),
    settings: pair("Settings", "设置"),
    quota: pair("Quota", "额度"),
    remaining: pair("left", "剩余"),
    resetsIn: pair("Resets in", "将在以下时间后重置"),
    resetsAt: pair("Resets", "重置于"),
    resetUnavailable: pair("Reset time unavailable", "暂无重置时间"),
    accounts: pair("Accounts", "账号"),
    current: pair("Current", "当前"),
    available: pair("Available", "可用"),
    switchAccount: pair("Switch", "切换"),
    noSavedAccounts: pair("No saved accounts. Add one in Settings.", "暂无已保存账号，请在设置中添加。"),
    sessionManager: pair("Session Manager", "会话管理器"),
    repairNow: pair("Repair now", "立即修复"),
    openCodexFolder: pair("Open Codex folder", "打开 Codex 文件夹"),
    updated: pair("Updated", "更新于"),
    justNow: pair("just now", "刚刚"),
    updatedJustNow: pair("Updated just now", "刚刚更新"),
    neverUpdated: pair("Not updated yet", "尚未更新"),
    quotaLoading: pair("Reading your latest quota…", "正在读取最新额度…"),
    quotaUnavailable: pair("Quota is unavailable", "暂时无法读取额度"),
    noQuotaWindows: pair("No quota windows were returned for this account.", "此账号暂未返回额度窗口。"),
    tryAgain: pair("Try again", "重试"),
    switchConfirm: pair("Safely switch to this account? A restore point will be created first.", "安全切换到此账号？程序会先创建还原点。"),
    switching: pair("Switching account…", "正在切换账号…"),
    repairing: pair("Repairing local sessions…", "正在修复本地会话…"),
    repairComplete: pair("Repair complete", "修复完成"),
    actionFailed: pair("Action failed", "操作失败"),
    providerActive: pair("Third-party Provider active", "第三方 Provider 已启用"),
  };
}

function fallbackErrorDashboard(message: string): DashboardState {
  const labels = previewLabels(false);
  return {
    schemaVersion: 1,
    stateRevision: 1,
    status: "error",
    language: "english",
    labels,
    currentAccount: null,
    quotaWindows: [],
    accounts: [],
    isRefreshing: false,
    refreshRequestedRevision: 0,
    refreshCompletedRevision: 0,
    fetchedAt: null,
    error: message,
    notice: null,
  };
}

function icon(name: string): string {
  const paths: Record<string, string> = {
    refresh: '<path d="M20 11a8 8 0 1 0-2.34 5.66"/><path d="M20 4v7h-7"/>',
    settings: '<circle cx="12" cy="12" r="3"/><path d="M19.4 15a1.7 1.7 0 0 0 .34 1.88l.06.06-2.83 2.83-.06-.06A1.7 1.7 0 0 0 15 19.4a1.7 1.7 0 0 0-1 .6 1.7 1.7 0 0 0-.4 1.1V21h-4v-.09A1.7 1.7 0 0 0 8.6 19.4a1.7 1.7 0 0 0-1.88.34l-.06.06-2.83-2.83.06-.06A1.7 1.7 0 0 0 4.6 15a1.7 1.7 0 0 0-.6-1 1.7 1.7 0 0 0-1.1-.4H3v-4h.09A1.7 1.7 0 0 0 4.6 8.6a1.7 1.7 0 0 0-.34-1.88l-.06-.06 2.83-2.83.06.06A1.7 1.7 0 0 0 9 4.6a1.7 1.7 0 0 0 1-.6 1.7 1.7 0 0 0 .4-1.1V3h4v.09A1.7 1.7 0 0 0 15.4 4.6a1.7 1.7 0 0 0 1.88-.34l.06-.06 2.83 2.83-.06.06A1.7 1.7 0 0 0 19.4 9c.1.37.32.72.6 1 .3.28.68.42 1.1.4h.09v4h-.09c-.42-.02-.8.12-1.1.4-.28.28-.5.63-.6 1Z"/>',
    user: '<circle cx="12" cy="8" r="3.25" fill="currentColor" stroke="none"/><path d="M5.5 19c.9-3.15 3.15-4.75 6.5-4.75s5.6 1.6 6.5 4.75" fill="currentColor" stroke="none"/>',
    clock: '<circle cx="12" cy="12" r="8.5"/><path d="M12 7.5V12l3 2"/>',
    calendar: '<rect x="4" y="5.5" width="16" height="14" rx="2"/><path d="M8 3.5v4M16 3.5v4M4 10h16"/>',
    check: '<path d="m5 12.5 4.2 4.2L19 7"/>',
    session: '<rect x="3.5" y="5" width="14" height="11" rx="2"/><path d="M7 5V3h14v11h-3.5"/><circle cx="16.5" cy="17.5" r="3.5"/><path d="M16.5 15.5v2.3l1.6 1"/>',
    repair: '<path d="M14.5 6.3a4.5 4.5 0 0 0-5.8 5.8L3.5 17.3a2 2 0 0 0 2.8 2.8l5.2-5.2a4.5 4.5 0 0 0 5.8-5.8l-2.5 2.5-2.4-.6-.6-2.4 2.7-2.3Z"/>',
    folder: '<path d="M3.5 7.5V18a2 2 0 0 0 2 2h13a2 2 0 0 0 2-2V8.5a2 2 0 0 0-2-2H12l-2-2H5.5a2 2 0 0 0-2 2v1Z"/>',
    chevron: '<path d="m9 5 7 7-7 7"/>',
    alert: '<path d="M10.3 4.1 2.7 17.3A2 2 0 0 0 4.4 20h15.2a2 2 0 0 0 1.7-2.7L13.7 4.1a2 2 0 0 0-3.4 0Z"/><path d="M12 9v4M12 16.5h.01"/>',
    info: '<circle cx="12" cy="12" r="9"/><path d="M12 11v5M12 8h.01"/>',
  };
  return `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">${paths[name] ?? paths.info}</svg>`;
}

function brandIcon(): string {
  return `<svg viewBox="146 227 269 266" fill="none"><path d="M249.176 323.434v-25.158c0-2.118.795-3.707 2.649-4.767l50.581-29.128c6.884-3.972 15.094-5.826 23.567-5.826 31.777 0 51.904 24.63 51.904 50.844 0 1.854 0 3.972-.266 6.091l-52.433-30.719c-3.177-1.852-6.356-1.852-9.533 0l-66.469 38.663Zm118.107 97.981v-60.114c0-3.709-1.589-6.356-4.767-8.209l-66.468-38.662 21.715-12.448c1.854-1.057 3.443-1.057 5.295 0l50.581 29.13c14.566 8.474 24.364 26.48 24.364 43.957 0 20.126-11.916 38.664-30.72 46.346ZM233.553 368.452l-21.715-12.71c-1.852-1.058-2.648-2.647-2.648-4.767v-58.257c0-28.335 21.715-49.786 51.111-49.786 11.122 0 21.447 3.709 30.189 10.328l-52.169 30.189c-3.175 1.854-4.766 4.502-4.766 8.21v76.796l-.002-.003Zm46.739 27.01-31.116-17.477v-37.072l31.116-17.477 31.115 17.477v37.072l-31.115 17.477Zm19.994 80.506c-11.123 0-21.449-3.709-30.189-10.328l52.167-30.191c3.177-1.852 4.766-4.5 4.766-8.21v-76.794l21.981 12.71c1.854 1.058 2.649 2.647 2.649 4.767v58.257c0 28.335-21.981 49.786-51.374 49.786v.003Zm-62.761-59.053-50.581-29.13c-14.566-8.475-24.362-26.48-24.362-43.958 0-20.391 12.181-38.663 30.981-46.342v60.376c0 3.71 1.591 6.356 4.767 8.21l66.205 38.396-21.715 12.448c-1.853 1.057-3.443 1.057-5.295 0Zm-2.911 43.428c-29.925 0-51.904-22.51-51.904-50.315 0-2.118.266-4.236.528-6.356l52.167 30.191c3.177 1.852 6.358 1.852 9.533 0l66.469-38.397v25.156c0 2.12-.795 3.709-2.649 4.767l-50.579 29.13c-6.886 3.972-15.096 5.824-23.568 5.824h.003Zm65.672 31.511c32.043 0 58.787-22.772 64.881-52.962 29.658-7.681 48.725-35.486 48.725-63.819 0-18.538-7.944-36.544-22.244-49.521 1.324-5.561 2.118-11.122 2.118-16.682 0-37.867-30.718-66.204-66.204-66.204-7.149 0-14.034 1.057-20.918 3.443-11.919-11.652-28.337-19.067-46.343-19.067-32.043 0-58.788 22.773-64.881 52.962-29.659 7.681-48.726 35.486-48.726 63.82 0 18.538 7.944 36.544 22.244 49.52-1.325 5.562-2.119 11.123-2.119 16.683 0 37.867 30.719 66.204 66.205 66.204 7.148 0 14.034-1.058 20.919-3.443 11.916 11.653 28.335 19.066 46.343 19.066Z" fill="currentColor"/></svg>`;
}
