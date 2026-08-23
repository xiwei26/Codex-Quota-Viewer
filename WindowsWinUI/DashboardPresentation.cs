namespace CodexQuotaViewer.WinUI;

public readonly record struct WidgetDashboardPresentation(
    string AccountName,
    string AccountStatus,
    bool IsApiAccount,
    string UnavailableQuotaMessage);

public static class DashboardPresentation
{
    public static WidgetDashboardPresentation Resolve(DashboardState state)
    {
        var quota = state.Quota;
        var active = state.Accounts.FirstOrDefault(account =>
            account.Active || string.Equals(account.Id, state.ActiveAccountId, StringComparison.Ordinal));
        var isApi = quota is not null
            ? IsApiKind(quota.Account.AccountType)
            : active is not null && IsApiKind(active.Kind);
        var accountName = quota?.Account.Email
            ?? active?.DisplayName
            ?? (isApi ? "API account" : "Current Codex account");
        var accountStatus = quota is not null
            ? $"Active {FriendlyKind(quota.Account.AccountType)} account"
            : active is not null
                ? $"Active {FriendlyKind(active.Kind)} account · Quota unavailable"
                : "Quota unavailable";
        var unavailableMessage = isApi
            ? "API accounts do not expose Codex rate-limit windows."
            : quota is null && active is not null
                ? "Codex quota is currently unavailable for this ChatGPT account."
                : quota is null
                    ? "Codex quota is currently unavailable."
                    : "No rate-limit windows are available.";

        return new WidgetDashboardPresentation(
            accountName,
            accountStatus,
            isApi,
            unavailableMessage);
    }

    public static string RefreshErrorNotice(DashboardState state) => state.Quota is null
        ? $"Quota refresh failed. {state.LastError}"
        : $"Showing the last successful quota snapshot. {state.LastError}";

    private static bool IsApiKind(string kind) =>
        string.Equals(kind, "api", StringComparison.OrdinalIgnoreCase) ||
        string.Equals(kind, "apiKey", StringComparison.OrdinalIgnoreCase);

    private static string FriendlyKind(string kind) => IsApiKind(kind)
        ? "API"
        : string.Equals(kind, "chatGpt", StringComparison.OrdinalIgnoreCase) ||
          string.Equals(kind, "chatgpt", StringComparison.OrdinalIgnoreCase)
            ? "ChatGPT"
            : "Codex";
}
