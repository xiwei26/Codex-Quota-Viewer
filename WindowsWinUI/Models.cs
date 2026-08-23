using System.Text.Json.Serialization;

namespace CodexQuotaViewer.WinUI;

public sealed class DashboardState
{
    public int SchemaVersion { get; set; }
    public QuotaSnapshot? Quota { get; set; }
    public List<AccountView> Accounts { get; set; } = [];
    public string? ActiveAccountId { get; set; }
    public CoreHostError? LastError { get; set; }
    public AppSettings Settings { get; set; } = new();
    public string? SettingsIssue { get; set; }
    public string CodexHome { get; set; } = string.Empty;
    public DateTimeOffset UpdatedAt { get; set; }
    public int? RolloutUpdates { get; set; }
    public string? RepairWarning { get; set; }
}

public sealed class QuotaSnapshot
{
    public QuotaAccount Account { get; set; } = new();
    public List<QuotaWindow> Windows { get; set; } = [];

    [JsonPropertyName("fetched_at")]
    public DateTimeOffset FetchedAt { get; set; }
}

public sealed class QuotaAccount
{
    public string? Id { get; set; }
    public string? Email { get; set; }

    [JsonPropertyName("account_type")]
    public string AccountType { get; set; } = "unknown";
}

public sealed class QuotaWindow
{
    public string Label { get; set; } = "quota";
    public double RemainingPercent { get; set; }
    public long? WindowDurationMins { get; set; }
    public long? ResetsAt { get; set; }
}

public sealed class AccountView
{
    public string Id { get; set; } = string.Empty;
    public string DisplayName { get; set; } = string.Empty;
    public string Kind { get; set; } = "chatGpt";
    public bool Active { get; set; }
}

public sealed class AppSettings
{
    public string RefreshIntervalPreset { get; set; } = "fiveMinutes";
    public bool LaunchAtLoginEnabled { get; set; }
    public string StatusItemStyle { get; set; } = "meter";
    public string AppLanguage { get; set; } = "system";
    public string? LastResolvedLanguage { get; set; }
}

public sealed class SettingsEnvelope
{
    public AppSettings Settings { get; set; } = new();
    public string? Issue { get; set; }
}

public sealed class RepairSummary
{
    public int CreatedThreads { get; set; }
    public int UpdatedThreads { get; set; }
    public int UpdatedSessionIndexEntries { get; set; }
    public int RemovedBrokenThreads { get; set; }
    public int HiddenSnapshotOnlySessions { get; set; }
}

public sealed class CoreHostError
{
    public string Code { get; set; } = "unknown";
    public string Message { get; set; } = "CoreHost error";
    public string? Diagnostics { get; set; }

    public override string ToString() => string.IsNullOrWhiteSpace(Diagnostics)
        ? Message
        : $"{Message}: {Diagnostics}";
}
