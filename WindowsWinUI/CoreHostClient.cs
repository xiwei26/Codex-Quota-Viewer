using System.Diagnostics;
using System.Text.Json;
using System.Text.Json.Serialization;

namespace CodexQuotaViewer.WinUI;

public sealed class CoreHostClient : IAsyncDisposable
{
    private readonly SemaphoreSlim _gate = new(1, 1);
    private readonly JsonSerializerOptions _jsonOptions = new(JsonSerializerDefaults.Web)
    {
        PropertyNameCaseInsensitive = true,
        DefaultIgnoreCondition = JsonIgnoreCondition.WhenWritingNull
    };
    private Process? _process;
    private long _nextId;
    private bool _disposed;

    public async Task<DashboardState> GetDashboardAsync(bool refresh, CancellationToken cancellationToken = default) =>
        await CallAsync<DashboardState>("getDashboard", new { refresh }, cancellationToken);

    public async Task<SettingsEnvelope> GetSettingsAsync(CancellationToken cancellationToken = default) =>
        await CallAsync<SettingsEnvelope>("getSettings", new { }, cancellationToken);

    public async Task SaveSettingsAsync(AppSettings settings, CancellationToken cancellationToken = default) =>
        _ = await CallAsync<JsonElement>("saveSettings", new { settings }, cancellationToken);

    public async Task<DashboardState> ImportCurrentChatGptAsync(string? displayName, CancellationToken cancellationToken = default) =>
        await CallAsync<DashboardState>("importCurrentChatGpt", new { displayName }, cancellationToken);

    public async Task<DashboardState> AddApiAccountAsync(
        string displayName,
        string apiKey,
        string baseUrl,
        string? model,
        string? providerName,
        CancellationToken cancellationToken = default) =>
        await CallAsync<DashboardState>("addApiAccount", new
        {
            input = new { displayName, apiKey, baseUrl, model, providerName }
        }, cancellationToken);

    public async Task<DashboardState> ActivateAccountAsync(string accountId, CancellationToken cancellationToken = default) =>
        await CallAsync<DashboardState>("activateAccount", new { accountId }, cancellationToken);

    public async Task<DashboardState> RenameAccountAsync(string accountId, string displayName, CancellationToken cancellationToken = default) =>
        await CallAsync<DashboardState>("renameAccount", new { accountId, displayName }, cancellationToken);

    public async Task<DashboardState> ForgetAccountAsync(string accountId, CancellationToken cancellationToken = default) =>
        await CallAsync<DashboardState>("forgetAccount", new { accountId }, cancellationToken);

    public async Task RollbackAsync(CancellationToken cancellationToken = default) =>
        _ = await CallAsync<DashboardState>("rollback", new { }, cancellationToken);

    public async Task OpenCodexFolderAsync(CancellationToken cancellationToken = default) =>
        _ = await CallAsync<JsonElement>("openCodexFolder", new { }, cancellationToken);

    public async Task OpenVaultFolderAsync(CancellationToken cancellationToken = default) =>
        _ = await CallAsync<JsonElement>("openVaultFolder", new { }, cancellationToken);

    public async Task OpenSessionManagerAsync(CancellationToken cancellationToken = default) =>
        _ = await CallAsync<JsonElement>("openSessionManager", new { }, cancellationToken);

    public async Task<RepairSummary> RepairAsync(CancellationToken cancellationToken = default) =>
        await CallAsync<RepairSummary>("repair", new { }, cancellationToken);

    private async Task<T> CallAsync<T>(string method, object parameters, CancellationToken cancellationToken)
    {
        ObjectDisposedException.ThrowIf(_disposed, this);
        await _gate.WaitAsync(cancellationToken);
        try
        {
            EnsureStarted();
            var id = Interlocked.Increment(ref _nextId);
            var request = JsonSerializer.Serialize(new { id, method, @params = parameters }, _jsonOptions);
            await _process!.StandardInput.WriteLineAsync(request.AsMemory(), cancellationToken);
            await _process.StandardInput.FlushAsync(cancellationToken);

            while (true)
            {
                var line = await _process.StandardOutput.ReadLineAsync(cancellationToken);
                if (line is null)
                {
                    var diagnostics = await ReadDiagnosticsAsync(_process);
                    DisposeProcess();
                    throw new CoreHostException(new CoreHostError
                    {
                        Code = "coreHostExited",
                        Message = "The Codex CoreHost stopped unexpectedly",
                        Diagnostics = diagnostics
                    });
                }

                var envelope = JsonSerializer.Deserialize<RpcEnvelope>(line, _jsonOptions)
                    ?? throw new InvalidDataException("CoreHost returned an empty response.");
                if (envelope.Id != id)
                {
                    continue;
                }
                if (!envelope.Ok)
                {
                    throw new CoreHostException(envelope.Error ?? new CoreHostError());
                }
                return envelope.Result.Deserialize<T>(_jsonOptions)
                    ?? throw new InvalidDataException($"CoreHost method {method} returned no result.");
            }
        }
        finally
        {
            _gate.Release();
        }
    }

    private void EnsureStarted()
    {
        if (_process is { HasExited: false })
        {
            return;
        }

        DisposeProcess();
        var executable = ResolveCoreHostPath();
        var startInfo = new ProcessStartInfo
        {
            FileName = executable,
            Arguments = $"--resource-root \"{AppContext.BaseDirectory.TrimEnd('\\')}\"",
            WorkingDirectory = AppContext.BaseDirectory,
            UseShellExecute = false,
            CreateNoWindow = true,
            RedirectStandardInput = true,
            RedirectStandardOutput = true,
            RedirectStandardError = true
        };
        _process = Process.Start(startInfo)
            ?? throw new InvalidOperationException("Could not start Codex CoreHost.");
    }

    private static string ResolveCoreHostPath()
    {
        var overridePath = Environment.GetEnvironmentVariable("CODEX_QUOTA_VIEWER_CORE_HOST");
        if (!string.IsNullOrWhiteSpace(overridePath) && File.Exists(overridePath))
        {
            return overridePath;
        }
        var adjacent = Path.Combine(AppContext.BaseDirectory, "codex-quota-viewer-core-host.exe");
        if (File.Exists(adjacent))
        {
            return adjacent;
        }
        throw new FileNotFoundException(
            "Codex CoreHost was not found. Build with scripts/build-windows-winui.ps1.",
            adjacent);
    }

    private static async Task<string?> ReadDiagnosticsAsync(Process process)
    {
        try
        {
            var text = await process.StandardError.ReadToEndAsync();
            return string.IsNullOrWhiteSpace(text) ? null : text.Trim();
        }
        catch
        {
            return null;
        }
    }

    private void DisposeProcess()
    {
        if (_process is null)
        {
            return;
        }
        try
        {
            if (!_process.HasExited)
            {
                _process.Kill(entireProcessTree: true);
            }
        }
        catch
        {
        }
        _process.Dispose();
        _process = null;
    }

    public async ValueTask DisposeAsync()
    {
        if (_disposed)
        {
            return;
        }
        _disposed = true;
        if (_process is { HasExited: false })
        {
            try
            {
                var id = Interlocked.Increment(ref _nextId);
                var request = JsonSerializer.Serialize(new { id, method = "shutdown", @params = new { } }, _jsonOptions);
                await _process.StandardInput.WriteLineAsync(request);
                await _process.StandardInput.FlushAsync();
                await _process.WaitForExitAsync(new CancellationTokenSource(TimeSpan.FromSeconds(2)).Token);
            }
            catch
            {
            }
        }
        DisposeProcess();
        _gate.Dispose();
    }

    private sealed class RpcEnvelope
    {
        public long Id { get; set; }
        public bool Ok { get; set; }
        public JsonElement Result { get; set; }
        public CoreHostError? Error { get; set; }
    }
}

public sealed class CoreHostException(CoreHostError error) : Exception(error.ToString())
{
    public CoreHostError Error { get; } = error;
}
