using Microsoft.UI.Composition.SystemBackdrops;
using Microsoft.UI.Windowing;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media;
using Microsoft.Win32;
using Windows.Graphics;
using Windows.UI;
using WinRT.Interop;

namespace CodexQuotaViewer.WinUI;

public sealed class SettingsWindow : Window
{
    private const string RunKeyPath = @"Software\Microsoft\Windows\CurrentVersion\Run";
    private const string RunValueName = "Codex Quota Viewer";
    private readonly CoreHostClient _core;
    private readonly AppWindow _appWindow;
    private readonly Grid _root;
    private readonly ComboBox _refreshInterval;
    private readonly ComboBox _language;
    private readonly ComboBox _trayStyle;
    private readonly ToggleSwitch _launchAtLogin;
    private readonly TextBlock _generalStatus;
    private readonly StackPanel _accountRows;
    private readonly TextBlock _accountsStatus;
    private bool _loaded;

    public event EventHandler? DashboardChanged;

    public SettingsWindow(CoreHostClient core)
    {
        _core = core;
        Title = "Codex Quota Viewer Settings";
        _root = new Grid { Background = new SolidColorBrush(Color.FromArgb(255, 24, 23, 29)) };
        Content = _root;

        var handle = WindowNative.GetWindowHandle(this);
        _appWindow = AppWindow.GetFromWindowId(Microsoft.UI.Win32Interop.GetWindowIdFromWindow(handle));
        _appWindow.Resize(new SizeInt32(760, 570));
        _appWindow.Closing += (_, args) =>
        {
            args.Cancel = true;
            _appWindow.Hide();
        };
        try
        {
            if (MicaController.IsSupported())
            {
                SystemBackdrop = new MicaBackdrop();
            }
        }
        catch
        {
        }

        _refreshInterval = Combo(
            ("Manual", "manual"),
            ("1 minute", "oneMinute"),
            ("5 minutes", "fiveMinutes"),
            ("15 minutes", "fifteenMinutes"));
        _language = Combo(
            ("Follow system", "system"),
            ("English", "english"),
            ("中文", "chinese"));
        _trayStyle = Combo(("Meter", "meter"), ("Text", "text"));
        _launchAtLogin = new ToggleSwitch();
        _generalStatus = Note(string.Empty);
        _accountRows = new StackPanel { Spacing = 8 };
        _accountsStatus = Note(string.Empty);

        BuildContent();
    }

    public async Task ShowAsync()
    {
        Activate();
        if (!_loaded)
        {
            _loaded = true;
            await LoadAsync();
        }
        else
        {
            await LoadAccountsAsync();
        }
    }

    private void BuildContent()
    {
        _root.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });
        _root.RowDefinitions.Add(new RowDefinition { Height = new GridLength(1, GridUnitType.Star) });
        var header = new StackPanel { Margin = new Thickness(28, 24, 28, 12), Spacing = 4 };
        header.Children.Add(new TextBlock
        {
            Text = "Settings",
            FontSize = 28,
            FontWeight = Microsoft.UI.Text.FontWeights.SemiBold
        });
        header.Children.Add(Note("Native WinUI 3 shell · shared settings and account vault"));
        _root.Children.Add(header);

        var tabs = new TabView
        {
            Margin = new Thickness(24, 0, 24, 24),
            IsAddTabButtonVisible = false,
            CanDragTabs = false,
            CanReorderTabs = false
        };
        tabs.TabItems.Add(new TabViewItem
        {
            Header = "General",
            IconSource = new SymbolIconSource { Symbol = Symbol.Setting },
            Content = BuildGeneralPage()
        });
        tabs.TabItems.Add(new TabViewItem
        {
            Header = "Accounts",
            IconSource = new SymbolIconSource { Symbol = Symbol.Contact },
            Content = BuildAccountsPage()
        });
        Grid.SetRow(tabs, 1);
        _root.Children.Add(tabs);
    }

    private UIElement BuildGeneralPage()
    {
        var panel = new StackPanel { Spacing = 18, Padding = new Thickness(20) };
        panel.Children.Add(SettingRow("Refresh interval", "How often quota is refreshed in the background.", _refreshInterval));
        panel.Children.Add(SettingRow("Language", "Language shared with the existing Windows tray settings.", _language));
        panel.Children.Add(SettingRow("Tray summary", "Presentation preference retained for the Tauri shell.", _trayStyle));
        panel.Children.Add(SettingRow("Launch at login", "Start this native widget when you sign in to Windows.", _launchAtLogin));
        var save = new Button
        {
            Content = "Save changes",
            HorizontalAlignment = HorizontalAlignment.Left,
            Padding = new Thickness(18, 9, 18, 9)
        };
        save.Click += async (_, _) => await SaveGeneralAsync(save);
        panel.Children.Add(save);
        panel.Children.Add(_generalStatus);
        return new ScrollViewer { Content = panel, VerticalScrollBarVisibility = ScrollBarVisibility.Auto };
    }

    private UIElement BuildAccountsPage()
    {
        var panel = new StackPanel { Spacing = 12, Padding = new Thickness(20) };
        var actions = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 8 };
        var import = new Button { Content = "Save current ChatGPT", Padding = new Thickness(13, 8, 13, 8) };
        import.Click += async (_, _) => await ImportCurrentAsync();
        actions.Children.Add(import);
        var addApi = new Button { Content = "Add API account", Padding = new Thickness(13, 8, 13, 8) };
        addApi.Click += async (_, _) => await AddApiAsync();
        actions.Children.Add(addApi);
        var vault = new Button { Content = "Open vault folder", Padding = new Thickness(13, 8, 13, 8) };
        vault.Click += async (_, _) => await RunAccountActionAsync(() => _core.OpenVaultFolderAsync());
        actions.Children.Add(vault);
        panel.Children.Add(actions);
        panel.Children.Add(_accountsStatus);
        panel.Children.Add(_accountRows);
        return new ScrollViewer { Content = panel, VerticalScrollBarVisibility = ScrollBarVisibility.Auto };
    }

    private async Task LoadAsync()
    {
        try
        {
            var envelope = await _core.GetSettingsAsync();
            Select(_refreshInterval, envelope.Settings.RefreshIntervalPreset);
            Select(_language, envelope.Settings.AppLanguage);
            Select(_trayStyle, envelope.Settings.StatusItemStyle);
            _launchAtLogin.IsOn = envelope.Settings.LaunchAtLoginEnabled;
            _generalStatus.Text = envelope.Issue ?? string.Empty;
            await LoadAccountsAsync();
        }
        catch (Exception error)
        {
            _generalStatus.Text = error.Message;
        }
    }

    private async Task LoadAccountsAsync()
    {
        try
        {
            var dashboard = await _core.GetDashboardAsync(false);
            RenderAccounts(dashboard);
        }
        catch (Exception error)
        {
            _accountsStatus.Text = error.Message;
        }
    }

    private void RenderAccounts(DashboardState dashboard)
    {
        _accountRows.Children.Clear();
        _accountsStatus.Text = dashboard.SettingsIssue ??
            (dashboard.Accounts.Count == 0 ? "No saved accounts yet." : $"{dashboard.Accounts.Count} saved account(s)");
        foreach (var account in dashboard.Accounts)
        {
            var row = new Grid
            {
                Padding = new Thickness(14, 12, 14, 12),
                Background = new SolidColorBrush(Color.FromArgb(60, 255, 255, 255)),
                CornerRadius = new CornerRadius(10)
            };
            row.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(1, GridUnitType.Star) });
            row.ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto });
            var labels = new StackPanel { Spacing = 2 };
            labels.Children.Add(new TextBlock { Text = account.DisplayName, FontSize = 16, FontWeight = Microsoft.UI.Text.FontWeights.SemiBold });
            labels.Children.Add(Note(account.Active ? "Current account" : account.Kind == "api" ? "API account" : "ChatGPT account"));
            row.Children.Add(labels);

            var actions = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 6 };
            if (!account.Active)
            {
                var activate = new Button { Content = "Activate", Tag = account.Id };
                activate.Click += async (_, _) => await ActivateAsync(account.Id, activate);
                actions.Children.Add(activate);
            }
            var rename = new Button { Content = "Rename", Tag = account.Id };
            rename.Click += async (_, _) => await RenameAsync(account);
            actions.Children.Add(rename);
            var forget = new Button { Content = "Forget", Tag = account.Id };
            forget.Click += async (_, _) => await ForgetAsync(account);
            actions.Children.Add(forget);
            Grid.SetColumn(actions, 1);
            row.Children.Add(actions);
            _accountRows.Children.Add(row);
        }
    }

    private async Task SaveGeneralAsync(Button button)
    {
        button.IsEnabled = false;
        _generalStatus.Text = "Saving…";
        try
        {
            var settings = new AppSettings
            {
                RefreshIntervalPreset = Value(_refreshInterval),
                AppLanguage = Value(_language),
                StatusItemStyle = Value(_trayStyle),
                LaunchAtLoginEnabled = _launchAtLogin.IsOn
            };
            await _core.SaveSettingsAsync(settings);
            ApplyLaunchAtLogin(settings.LaunchAtLoginEnabled);
            _generalStatus.Text = "Saved";
            DashboardChanged?.Invoke(this, EventArgs.Empty);
        }
        catch (Exception error)
        {
            _generalStatus.Text = error.Message;
        }
        finally
        {
            button.IsEnabled = true;
        }
    }

    private async Task ImportCurrentAsync()
    {
        var name = new TextBox { Header = "Display name (optional)", PlaceholderText = "Personal" };
        var dialog = Dialog("Save current ChatGPT account", name, "Save");
        if (await dialog.ShowAsync() != ContentDialogResult.Primary)
        {
            return;
        }
        await RunAccountActionAsync(async () =>
        {
            var dashboard = await _core.ImportCurrentChatGptAsync(name.Text);
            RenderAccounts(dashboard);
        });
    }

    private async Task AddApiAsync()
    {
        var name = new TextBox { Header = "Display name", PlaceholderText = "API Workspace" };
        var key = new PasswordBox { Header = "API key", PlaceholderText = "sk-…" };
        var baseUrl = new TextBox { Header = "Base URL", Text = "https://api.openai.com/v1" };
        var model = new TextBox { Header = "Model (optional)", PlaceholderText = "gpt-5.4" };
        var provider = new TextBox { Header = "Provider id (optional)", PlaceholderText = "openai" };
        var fields = new StackPanel { Spacing = 10 };
        fields.Children.Add(name);
        fields.Children.Add(key);
        fields.Children.Add(baseUrl);
        fields.Children.Add(model);
        fields.Children.Add(provider);
        var dialog = Dialog("Add OpenAI-compatible API account", fields, "Add");
        if (await dialog.ShowAsync() != ContentDialogResult.Primary)
        {
            return;
        }
        await RunAccountActionAsync(async () =>
        {
            var dashboard = await _core.AddApiAccountAsync(
                name.Text,
                key.Password,
                baseUrl.Text,
                model.Text,
                provider.Text);
            RenderAccounts(dashboard);
        });
    }

    private async Task ActivateAsync(string accountId, Button button)
    {
        button.IsEnabled = false;
        _accountsStatus.Text = "Creating restore point and switching account…";
        try
        {
            var dashboard = await _core.ActivateAccountAsync(accountId);
            RenderAccounts(dashboard);
            _accountsStatus.Text = "Account switched safely.";
            DashboardChanged?.Invoke(this, EventArgs.Empty);
        }
        catch (Exception error)
        {
            _accountsStatus.Text = error.Message;
        }
        finally
        {
            button.IsEnabled = true;
        }
    }

    private async Task RenameAsync(AccountView account)
    {
        var name = new TextBox { Header = "Display name", Text = account.DisplayName };
        var dialog = Dialog("Rename account", name, "Rename");
        if (await dialog.ShowAsync() != ContentDialogResult.Primary)
        {
            return;
        }
        await RunAccountActionAsync(async () =>
        {
            var dashboard = await _core.RenameAccountAsync(account.Id, name.Text);
            RenderAccounts(dashboard);
        });
    }

    private async Task ForgetAsync(AccountView account)
    {
        var dialog = Dialog(
            "Forget account?",
            new TextBlock { Text = $"Remove {account.DisplayName} from the local vault?", TextWrapping = TextWrapping.Wrap },
            "Forget");
        if (await dialog.ShowAsync() != ContentDialogResult.Primary)
        {
            return;
        }
        await RunAccountActionAsync(async () =>
        {
            var dashboard = await _core.ForgetAccountAsync(account.Id);
            RenderAccounts(dashboard);
        });
    }

    private async Task RunAccountActionAsync(Func<Task> action)
    {
        _accountsStatus.Text = "Working…";
        try
        {
            await action();
            _accountsStatus.Text = "Done";
            DashboardChanged?.Invoke(this, EventArgs.Empty);
        }
        catch (Exception error)
        {
            _accountsStatus.Text = error.Message;
        }
    }

    private ContentDialog Dialog(string title, UIElement content, string primaryText) => new()
    {
        XamlRoot = _root.XamlRoot,
        Title = title,
        Content = content,
        PrimaryButtonText = primaryText,
        CloseButtonText = "Cancel",
        DefaultButton = ContentDialogButton.Primary
    };

    private static Grid SettingRow(string title, string detail, Control control)
    {
        var row = new Grid();
        row.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(1, GridUnitType.Star) });
        row.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(260) });
        var labels = new StackPanel { Spacing = 3, Margin = new Thickness(0, 3, 20, 0) };
        labels.Children.Add(new TextBlock { Text = title, FontSize = 16, FontWeight = Microsoft.UI.Text.FontWeights.SemiBold });
        var note = Note(detail);
        note.TextWrapping = TextWrapping.Wrap;
        labels.Children.Add(note);
        row.Children.Add(labels);
        control.HorizontalAlignment = HorizontalAlignment.Stretch;
        control.VerticalAlignment = VerticalAlignment.Center;
        Grid.SetColumn(control, 1);
        row.Children.Add(control);
        return row;
    }

    private static ComboBox Combo(params (string Label, string Value)[] items)
    {
        var combo = new ComboBox();
        foreach (var item in items)
        {
            combo.Items.Add(new ComboBoxItem { Content = item.Label, Tag = item.Value });
        }
        combo.SelectedIndex = 0;
        return combo;
    }

    private static void Select(ComboBox combo, string value)
    {
        combo.SelectedItem = combo.Items
            .OfType<ComboBoxItem>()
            .FirstOrDefault(item => string.Equals(item.Tag?.ToString(), value, StringComparison.OrdinalIgnoreCase))
            ?? combo.Items[0];
    }

    private static string Value(ComboBox combo) =>
        (combo.SelectedItem as ComboBoxItem)?.Tag?.ToString() ?? string.Empty;

    private static TextBlock Note(string value) => new()
    {
        Text = value,
        FontSize = 12,
        Foreground = new SolidColorBrush(Color.FromArgb(255, 170, 166, 176))
    };

    private static void ApplyLaunchAtLogin(bool enabled)
    {
        using var key = Registry.CurrentUser.CreateSubKey(RunKeyPath, writable: true)
            ?? throw new InvalidOperationException("Could not open the Windows Run registry key.");
        if (enabled)
        {
            var executable = Environment.ProcessPath
                ?? throw new InvalidOperationException("Current executable path is unavailable.");
            key.SetValue(RunValueName, $"\"{executable}\" --background", RegistryValueKind.String);
        }
        else
        {
            key.DeleteValue(RunValueName, throwOnMissingValue: false);
        }
    }
}
