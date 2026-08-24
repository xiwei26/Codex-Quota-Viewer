using Microsoft.UI.Dispatching;
using Microsoft.UI.Xaml;

namespace CodexQuotaViewer.WinUI;

public partial class App : Application
{
    private CoreHostClient? _core;
    private WidgetWindow? _widget;
    private SettingsWindow? _settings;
    private TrayIconService? _tray;
    private SingleInstanceService? _singleInstance;
    private DispatcherQueueTimer? _refreshTimer;
    private bool _quitting;

    public App()
    {
        InitializeComponent();
        UnhandledException += (_, args) =>
        {
            RuntimeLog.Write($"UnhandledException: {args.Exception.GetType().Name}: {args.Exception.Message}");
            _widget?.Notify(args.Exception.Message);
            args.Handled = true;
        };
    }

    protected override void OnLaunched(LaunchActivatedEventArgs args)
    {
        RuntimeLog.Write("App launch requested.");
        var singleInstance = new SingleInstanceService(DispatcherQueue.GetForCurrentThread());
        _singleInstance = singleInstance;
        if (!singleInstance.IsPrimary)
        {
            RuntimeLog.Write("Secondary instance signalled the primary instance and will exit.");
            singleInstance.Dispose();
            Exit();
            return;
        }

        RuntimeLog.Write("Primary instance is initializing CoreHost, widget, and tray icon.");
        _core = new CoreHostClient();
        _widget = new WidgetWindow(_core);
        var widget = _widget;
        singleInstance.StartListening(() =>
        {
            RuntimeLog.Write("Primary instance received a wake signal.");
            _ = widget.ShowAsync();
        });
        _tray = new TrayIconService(WinRT.Interop.WindowNative.GetWindowHandle(_widget));
        _tray.LeftClicked += (_, _) =>
        {
            RuntimeLog.Write("Tray left-click queued after the shell callback returns.");
            _ = _widget.ToggleFromTrayAsync();
        };
        _tray.CommandInvoked += OnTrayCommand;
        _tray.ContextMenuVisibilityChanged += (_, visible) => _widget.SetContextMenuVisible(visible);

        _widget.SettingsRequested += (_, _) => _ = ShowSettingsAsync();
        _widget.OpenSessionManagerRequested += async (_, _) => await OpenSessionManagerAsync();
        _widget.RepairRequested += async (_, _) => await RepairAsync();
        _widget.OpenCodexFolderRequested += async (_, _) => await OpenCodexFolderAsync();

        _ = InitializeAsync();
        RuntimeLog.Write("Primary instance initialization completed; widget remains hidden.");
    }

    private async Task InitializeAsync()
    {
        if (_widget is null)
        {
            return;
        }
        await _widget.InitializeAsync();
        await ConfigureRefreshTimerAsync();
    }

    private async void OnTrayCommand(object? sender, TrayCommand command)
    {
        switch (command)
        {
            case TrayCommand.ToggleWidget:
                if (_widget is not null) await _widget.ToggleFromTrayAsync();
                break;
            case TrayCommand.Refresh:
                if (_widget is not null) await _widget.RefreshAsync(true);
                break;
            case TrayCommand.Settings:
                await ShowSettingsAsync();
                break;
            case TrayCommand.SessionManager:
                await OpenSessionManagerAsync();
                break;
            case TrayCommand.Repair:
                await RepairAsync();
                break;
            case TrayCommand.OpenCodexFolder:
                await OpenCodexFolderAsync();
                break;
            case TrayCommand.Rollback:
                await RollbackAsync();
                break;
            case TrayCommand.Quit:
                await QuitAsync();
                break;
        }
    }

    private async Task ShowSettingsAsync()
    {
        if (_core is null)
        {
            return;
        }
        _settings ??= new SettingsWindow(_core);
        _settings.DashboardChanged -= OnDashboardChanged;
        _settings.DashboardChanged += OnDashboardChanged;
        await _settings.ShowAsync();
    }

    private async void OnDashboardChanged(object? sender, EventArgs args)
    {
        if (_widget is not null)
        {
            await _widget.RefreshAsync(false);
        }
        await ConfigureRefreshTimerAsync();
    }

    private async Task OpenSessionManagerAsync()
    {
        if (_core is null) return;
        try
        {
            _widget?.Notify("Opening Session Manager…");
            await _core.OpenSessionManagerAsync();
            _widget?.Notify("Session Manager opened in your browser");
        }
        catch (Exception error)
        {
            _widget?.Notify(error.Message);
        }
    }

    private async Task RepairAsync()
    {
        if (_core is null) return;
        try
        {
            _widget?.Notify("Repairing local thread metadata…");
            var summary = await _core.RepairAsync();
            _widget?.Notify($"Repair complete: +{summary.CreatedThreads}, ~{summary.UpdatedThreads}, index {summary.UpdatedSessionIndexEntries}");
        }
        catch (Exception error)
        {
            _widget?.Notify(error.Message);
        }
    }

    private async Task OpenCodexFolderAsync()
    {
        if (_core is null) return;
        try
        {
            await _core.OpenCodexFolderAsync();
        }
        catch (Exception error)
        {
            _widget?.Notify(error.Message);
        }
    }

    private async Task RollbackAsync()
    {
        if (_core is null) return;
        try
        {
            _widget?.Notify("Restoring the latest Safe Switch restore point…");
            await _core.RollbackAsync();
            if (_widget is not null)
            {
                await _widget.RefreshAsync(false);
            }
            _widget?.Notify("Rollback complete");
        }
        catch (Exception error)
        {
            _widget?.Notify(error.Message);
        }
    }

    private async Task ConfigureRefreshTimerAsync()
    {
        if (_core is null)
        {
            return;
        }
        try
        {
            var settings = (await _core.GetSettingsAsync()).Settings;
            _refreshTimer?.Stop();
            var interval = settings.RefreshIntervalPreset switch
            {
                "oneMinute" => TimeSpan.FromMinutes(1),
                "fiveMinutes" => TimeSpan.FromMinutes(5),
                "fifteenMinutes" => TimeSpan.FromMinutes(15),
                _ => TimeSpan.Zero
            };
            if (interval == TimeSpan.Zero)
            {
                return;
            }
            _refreshTimer ??= DispatcherQueue.GetForCurrentThread().CreateTimer();
            _refreshTimer.Tick -= OnRefreshTimer;
            _refreshTimer.Tick += OnRefreshTimer;
            _refreshTimer.Interval = interval;
            _refreshTimer.IsRepeating = true;
            _refreshTimer.Start();
        }
        catch (Exception error)
        {
            _widget?.Notify(error.Message);
        }
    }

    private async void OnRefreshTimer(DispatcherQueueTimer sender, object args)
    {
        if (_widget is not null)
        {
            await _widget.RefreshAsync(true);
        }
    }

    private async Task QuitAsync()
    {
        if (_quitting)
        {
            return;
        }
        _quitting = true;
        _refreshTimer?.Stop();
        _tray?.Dispose();
        _singleInstance?.Dispose();
        if (_core is not null)
        {
            await _core.DisposeAsync();
        }
        Exit();
    }
}
