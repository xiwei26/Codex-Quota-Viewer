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
  providerAccountId: string;
  providerDisplayName: string;
  activatedAt: string;
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
