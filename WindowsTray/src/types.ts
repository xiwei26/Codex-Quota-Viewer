export type RefreshIntervalPreset =
  | "manual"
  | "oneMinute"
  | "fiveMinutes"
  | "fifteenMinutes";
export type StatusItemStyle = "meter" | "text";
export type AppLanguage = "system" | "english" | "chinese";
export type ResolvedAppLanguage = "english" | "chinese";

export type AppSettings = {
  refreshIntervalPreset: RefreshIntervalPreset;
  launchAtLoginEnabled: boolean;
  statusItemStyle: StatusItemStyle;
  appLanguage: AppLanguage;
  lastResolvedLanguage: ResolvedAppLanguage | null;
};

export type SelectOption = {
  value: string;
  label: string;
};

export type SettingsPresentation = {
  settings: AppSettings;
  resolvedLanguage: ResolvedAppLanguage;
  labels: {
    title: string;
    refreshInterval: string;
    language: string;
    trayStyle: string;
    launchAtLogin: string;
    saved: string;
  };
  refreshIntervalOptions: SelectOption[];
  languageOptions: SelectOption[];
  trayStyleOptions: SelectOption[];
  message: string | null;
};

export type AccountKind = "chatGpt" | "api";
export type AccountRowState = "active" | "available" | "needsAttention";

export type AccountRow = {
  id: string;
  displayName: string;
  kind: AccountKind;
  state: AccountRowState;
  status: string;
};

export type ProviderModeState = {
  restorePointId?: string | null;
  providerAccountId: string;
  providerDisplayName: string;
  activatedAt: string;
};

export type ProviderCount = {
  providerId: string;
  count: number;
};

export type LocalProviderSyncPresentation = {
  title: string;
  status: string;
  expectedProvider: string | null;
  rolloutProviders: ProviderCount[];
  threadProviders: ProviderCount[];
  threadIssue: string | null;
};

export type AccountsPresentation = {
  labels: {
    accounts: string;
    signInWithChatgpt: string;
    addApiAccount: string;
    openVaultFolder: string;
    rollbackLastChange: string;
    repairNow: string;
    activate: string;
    rename: string;
    forget: string;
    switchToProvider: string;
    switchBackFromProvider: string;
    providerModeActive: string;
    current: string;
    noSavedAccounts: string;
  };
  rows: AccountRow[];
  providerMode: ProviderModeState | null;
  message: string | null;
};

export type ApiProbeResult = {
  normalizedBaseUrl: string;
  modelIds: string[];
  suggestedDisplayName: string;
};

export type DashboardStatus = "loading" | "ready" | "empty" | "error";

export type DashboardQuotaWindow = {
  label: string;
  displayLabel: string;
  remainingPercent: number;
  resetAt: string | null;
};

export type DashboardAccount = {
  id: string;
  displayName: string;
  kind: AccountKind;
  isActive: boolean;
  status: string;
};

export type DashboardCurrentAccount = {
  displayName: string;
  detail: string;
  kind: AccountKind;
};

export type DashboardLabels = {
  title: string;
  activeAccount: string;
  refreshing: string;
  refresh: string;
  settings: string;
  quota: string;
  remaining: string;
  resetsIn: string;
  resetsAt: string;
  resetUnavailable: string;
  accounts: string;
  current: string;
  available: string;
  switchAccount: string;
  noSavedAccounts: string;
  sessionManager: string;
  repairNow: string;
  openCodexFolder: string;
  updated: string;
  justNow: string;
  updatedJustNow: string;
  neverUpdated: string;
  quotaLoading: string;
  quotaUnavailable: string;
  noQuotaWindows: string;
  tryAgain: string;
  switchConfirm: string;
  switching: string;
  repairing: string;
  repairComplete: string;
  actionFailed: string;
  providerActive: string;
};

export type DashboardState = {
  schemaVersion: 1;
  stateRevision: number;
  status: DashboardStatus;
  language: ResolvedAppLanguage;
  labels: DashboardLabels;
  currentAccount: DashboardCurrentAccount | null;
  quotaWindows: DashboardQuotaWindow[];
  accounts: DashboardAccount[];
  isRefreshing: boolean;
  refreshRequestedRevision: number;
  refreshCompletedRevision: number;
  fetchedAt: string | null;
  error: string | null;
  notice: string | null;
};
