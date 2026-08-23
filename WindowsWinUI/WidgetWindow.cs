using System.Diagnostics;
using System.Globalization;
using Microsoft.UI.Composition.SystemBackdrops;
using Microsoft.UI.Windowing;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Input;
using Microsoft.UI.Xaml.Media;
using Microsoft.UI.Xaml.Media.Imaging;
using Windows.Foundation;
using Windows.Graphics;
using Windows.System;
using Windows.UI;
using WinRT.Interop;
using FontWeight = Windows.UI.Text.FontWeight;
using FontWeights = Microsoft.UI.Text.FontWeights;

namespace CodexQuotaViewer.WinUI;

public sealed class WidgetWindow : Window
{
    private static readonly CultureInfo EnglishUiCulture = CultureInfo.GetCultureInfo("en-US");
    private static readonly Color Mint = Color.FromArgb(255, 134, 228, 177);
    private static readonly Color PrimaryText = Color.FromArgb(255, 247, 244, 249);
    private static readonly Color SecondaryText = Color.FromArgb(255, 180, 173, 187);
    private static readonly Color CardFill = Color.FromArgb(92, 48, 42, 56);
    private static readonly Color CardStroke = Color.FromArgb(90, 150, 140, 160);
    private readonly CoreHostClient _core;
    private readonly nint _windowHandle;
    private readonly AppWindow _appWindow;
    private readonly OverlappedPresenter _presenter;
    private readonly Grid _root;
    private readonly StackPanel _quotaPanel;
    private readonly StackPanel _accountsPanel;
    private readonly TextBlock _accountName;
    private readonly TextBlock _accountStatus;
    private readonly TextBlock _updated;
    private readonly TextBlock _notice;
    private readonly Button _refreshButton;
    private CancellationTokenSource? _animation;
    private CancellationTokenSource? _deactivationCheck;
    private CancellationTokenSource? _refresh;
    private long _refreshGeneration;
    private int _currentX;
    private WidgetPlacement _placement;
    private DateTimeOffset _ignoreDeactivationUntil;
    private bool _shown;
    private bool _contextMenuVisible;
    private bool _isWindowActive;

    public event EventHandler? SettingsRequested;
    public event EventHandler? OpenSessionManagerRequested;
    public event EventHandler? RepairRequested;
    public event EventHandler? OpenCodexFolderRequested;

    public bool IsWidgetVisible => _shown;

    public void Notify(string message)
    {
        ShowNotice(message);
        _updated.Text = message;
    }

    public WidgetWindow(CoreHostClient core)
    {
        _core = core;
        Title = "Codex Quota Viewer";
        ExtendsContentIntoTitleBar = true;
        _root = new Grid
        {
            Background = new SolidColorBrush(Color.FromArgb(1, 0, 0, 0)),
            IsTabStop = true
        };
        Content = _root;

        _accountName = Text("Current Codex account", 15, PrimaryText, FontWeights.SemiBold);
        _accountStatus = Text("Loading account…", 12, SecondaryText);
        _updated = Text("Starting CoreHost…", 11, SecondaryText);
        _notice = Text(string.Empty, 11, Color.FromArgb(255, 244, 174, 174));
        _notice.TextWrapping = TextWrapping.Wrap;
        _notice.Visibility = Visibility.Collapsed;
        _quotaPanel = new StackPanel { Spacing = 8 };
        _accountsPanel = new StackPanel { Spacing = 0 };
        _refreshButton = CircleButton("\uE72C", "Refresh quota");
        _refreshButton.Click += async (_, _) => await RefreshAsync(true);

        BuildContent();

        _windowHandle = WindowNative.GetWindowHandle(this);
        var windowId = Microsoft.UI.Win32Interop.GetWindowIdFromWindow(_windowHandle);
        _appWindow = AppWindow.GetFromWindowId(windowId);
        _appWindow.IsShownInSwitchers = false;
        _presenter = OverlappedPresenter.Create();
        _presenter.IsResizable = false;
        _presenter.IsMaximizable = false;
        _presenter.IsMinimizable = false;
        _presenter.IsAlwaysOnTop = false;
        _presenter.SetBorderAndTitleBar(false, false);
        _appWindow.SetPresenter(_presenter);
        _appWindow.Hide();

        var style = NativeMethods.GetWindowLongPtr(_windowHandle, NativeMethods.GwlExStyle).ToInt64();
        style = (style | NativeMethods.WsExToolWindow) & ~NativeMethods.WsExAppWindow;
        NativeMethods.SetWindowLongPtr(_windowHandle, NativeMethods.GwlExStyle, (nint)style);

        TryApplyBackdrop();
        Activated += OnActivated;
        _appWindow.Closing += (sender, args) =>
        {
            args.Cancel = true;
            _ = HideAsync();
        };
        _root.KeyDown += (_, args) =>
        {
            if (args.Key == VirtualKey.Escape)
            {
                args.Handled = true;
                _ = HideAsync();
            }
        };
    }

    public void SetContextMenuVisible(bool visible)
    {
        _contextMenuVisible = visible;
        if (visible)
        {
            _deactivationCheck?.Cancel();
        }
        else
        {
            _ignoreDeactivationUntil = DateTimeOffset.UtcNow.AddMilliseconds(250);
            ScheduleDeactivationCheck();
        }
    }

    public async Task InitializeAsync()
    {
        await RefreshAsync(true);
    }

    public async Task ToggleAsync()
    {
        if (_shown)
        {
            await HideAsync();
        }
        else
        {
            await ShowAsync();
        }
    }

    public async Task ShowAsync()
    {
        var workArea = NativeMethods.WorkAreaFromCursor(_windowHandle, out var dpi);
        _placement = WidgetPlacement.ForWorkArea(workArea, dpi);
        _currentX = _placement.HiddenX;
        _appWindow.MoveAndResize(new RectInt32(
            _currentX,
            _placement.Y,
            _placement.Width,
            _placement.Height));
        _presenter.IsAlwaysOnTop = true;
        _appWindow.Show();
        _shown = true;
        _isWindowActive = true;
        _ignoreDeactivationUntil = DateTimeOffset.UtcNow.AddMilliseconds(600);
        Activate();
        NativeMethods.ForceForeground(_windowHandle);
        var completed = await AnimateToAsync(_placement.VisibleX);
        if (completed && _shown)
        {
            _isWindowActive = true;
            _ignoreDeactivationUntil = DateTimeOffset.UtcNow.AddMilliseconds(600);
            NativeMethods.ForceForeground(_windowHandle);
            _root.Focus(FocusState.Programmatic);
        }
    }

    public async Task HideAsync()
    {
        if (!_shown)
        {
            return;
        }
        _deactivationCheck?.Cancel();
        _shown = false;
        var completed = await AnimateToAsync(_placement.HiddenX);
        if (completed && !_shown)
        {
            _appWindow.Hide();
            _presenter.IsAlwaysOnTop = false;
            _isWindowActive = false;
        }
    }

    public async Task RefreshAsync(bool force)
    {
        _refresh?.Cancel();
        var refresh = new CancellationTokenSource();
        _refresh = refresh;
        var generation = ++_refreshGeneration;
        _refreshButton.IsEnabled = false;
        _updated.Text = force ? "Refreshing…" : "Loading…";
        try
        {
            var state = await _core.GetDashboardAsync(force, refresh.Token);
            if (generation == _refreshGeneration)
            {
                Render(state);
            }
        }
        catch (OperationCanceledException)
        {
        }
        catch (Exception error)
        {
            if (generation == _refreshGeneration)
            {
                ShowNotice(error.Message);
                _updated.Text = "Refresh failed";
            }
        }
        finally
        {
            if (generation == _refreshGeneration)
            {
                _refreshButton.IsEnabled = true;
                if (ReferenceEquals(_refresh, refresh))
                {
                    _refresh = null;
                }
            }
            refresh.Dispose();
        }
    }

    private async Task<bool> AnimateToAsync(int targetX)
    {
        _animation?.Cancel();
        _animation?.Dispose();
        _animation = new CancellationTokenSource();
        var token = _animation.Token;
        var from = _currentX;
        if (from == targetX)
        {
            return true;
        }
        var stopwatch = Stopwatch.StartNew();
        const double durationMs = 220;
        try
        {
            while (stopwatch.Elapsed.TotalMilliseconds < durationMs)
            {
                token.ThrowIfCancellationRequested();
                var progress = WidgetPlacement.EaseOutCubic(stopwatch.Elapsed.TotalMilliseconds / durationMs);
                _currentX = (int)Math.Round(from + ((targetX - from) * progress));
                _appWindow.Move(new PointInt32(_currentX, _placement.Y));
                await Task.Delay(10, token);
            }
            _currentX = targetX;
            _appWindow.Move(new PointInt32(targetX, _placement.Y));
            return true;
        }
        catch (OperationCanceledException)
        {
            return false;
        }
    }

    private void OnActivated(object sender, WindowActivatedEventArgs args)
    {
        _isWindowActive = args.WindowActivationState != WindowActivationState.Deactivated;
        if (_isWindowActive)
        {
            _deactivationCheck?.Cancel();
            return;
        }
        ScheduleDeactivationCheck();
    }

    private void ScheduleDeactivationCheck()
    {
        if (!_shown || _contextMenuVisible || _isWindowActive)
        {
            return;
        }

        _deactivationCheck?.Cancel();
        var check = new CancellationTokenSource();
        _deactivationCheck = check;
        _ = DispatcherQueue.TryEnqueue(async () =>
        {
            try
            {
                var delay = WindowActivationPolicy.DelayBeforeRecheck(
                    DateTimeOffset.UtcNow,
                    _ignoreDeactivationUntil);
                await Task.Delay(delay, check.Token);
                var isForeground = NativeMethods.GetForegroundWindow() == _windowHandle;
                if (WindowActivationPolicy.ShouldHide(
                    _shown,
                    _contextMenuVisible,
                    _isWindowActive,
                    isForeground,
                    DateTimeOffset.UtcNow,
                    _ignoreDeactivationUntil))
                {
                    await HideAsync();
                }
            }
            catch (OperationCanceledException)
            {
            }
            finally
            {
                if (ReferenceEquals(_deactivationCheck, check))
                {
                    _deactivationCheck = null;
                }
                check.Dispose();
            }
        });
    }

    private void Render(DashboardState state)
    {
        var quota = state.Quota;
        var presentation = DashboardPresentation.Resolve(state);
        _accountName.Text = presentation.AccountName;
        _accountStatus.Text = presentation.AccountStatus;

        _quotaPanel.Children.Clear();
        if (quota?.Windows.Count > 0)
        {
            foreach (var window in quota.Windows)
            {
                _quotaPanel.Children.Add(BuildQuotaCard(window));
            }
        }
        else
        {
            _quotaPanel.Children.Add(BuildUnavailableQuotaCard(presentation.UnavailableQuotaMessage));
        }

        _accountsPanel.Children.Clear();
        if (state.Accounts.Count == 0)
        {
            var empty = Text("No saved accounts — add the current account in Settings.", 12, SecondaryText);
            empty.Margin = new Thickness(12, 12, 12, 12);
            empty.TextWrapping = TextWrapping.Wrap;
            _accountsPanel.Children.Add(empty);
        }
        else
        {
            for (var index = 0; index < state.Accounts.Count; index++)
            {
                if (index > 0)
                {
                    _accountsPanel.Children.Add(new Border
                    {
                        Height = 1,
                        Background = new SolidColorBrush(Color.FromArgb(38, 255, 255, 255)),
                        Margin = new Thickness(10, 0, 10, 0)
                    });
                }
                _accountsPanel.Children.Add(BuildAccountRow(state.Accounts[index]));
            }
        }

        var fetched = quota?.FetchedAt ?? state.UpdatedAt;
        _updated.Text = $"Updated {RelativeTime(fetched)}";
        if (state.LastError is not null)
        {
            ShowNotice(DashboardPresentation.RefreshErrorNotice(state));
        }
        else if (!string.IsNullOrWhiteSpace(state.SettingsIssue))
        {
            ShowNotice(state.SettingsIssue);
        }
        else if (!string.IsNullOrWhiteSpace(state.RepairWarning))
        {
            ShowNotice($"Account switched; Session Manager repair needs attention: {state.RepairWarning}");
        }
        else
        {
            _notice.Visibility = Visibility.Collapsed;
        }
    }

    private void BuildContent()
    {
        var background = new LinearGradientBrush
        {
            StartPoint = new Point(0, 0),
            EndPoint = new Point(1, 1),
            GradientStops =
            {
                new GradientStop { Color = Color.FromArgb(220, 35, 25, 37), Offset = 0 },
                new GradientStop { Color = Color.FromArgb(235, 12, 16, 29), Offset = 0.72 },
                new GradientStop { Color = Color.FromArgb(232, 19, 20, 33), Offset = 1 }
            }
        };
        var shell = new Border
        {
            Margin = new Thickness(4),
            CornerRadius = new CornerRadius(16),
            BorderThickness = new Thickness(1),
            BorderBrush = new SolidColorBrush(Color.FromArgb(100, 137, 126, 147)),
            Background = background
        };
        _root.Children.Add(shell);

        var scroll = new ScrollViewer
        {
            VerticalScrollBarVisibility = ScrollBarVisibility.Auto,
            HorizontalScrollBarVisibility = ScrollBarVisibility.Disabled,
            Padding = new Thickness(14, 12, 14, 12)
        };
        shell.Child = scroll;
        var body = new StackPanel { Spacing = 9 };
        scroll.Content = body;

        var header = new Grid { Margin = new Thickness(2, 0, 2, 4) };
        header.ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto });
        header.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(1, GridUnitType.Star) });
        header.ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto });
        var logo = BuildLogo();
        header.Children.Add(logo);
        var title = Text("Codex Quota Viewer", 15, PrimaryText, FontWeights.SemiBold);
        title.VerticalAlignment = VerticalAlignment.Center;
        title.Margin = new Thickness(8, 0, 0, 0);
        Grid.SetColumn(title, 1);
        header.Children.Add(title);
        var headerActions = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 6 };
        headerActions.Children.Add(_refreshButton);
        var settings = CircleButton("\uE713", "Settings");
        settings.Click += (_, _) => SettingsRequested?.Invoke(this, EventArgs.Empty);
        headerActions.Children.Add(settings);
        Grid.SetColumn(headerActions, 2);
        header.Children.Add(headerActions);
        body.Children.Add(header);

        body.Children.Add(BuildActiveAccountCard());
        body.Children.Add(_quotaPanel);
        var accountsTitle = Text("Accounts", 13, SecondaryText, FontWeights.SemiBold);
        accountsTitle.Margin = new Thickness(1, 2, 0, 0);
        body.Children.Add(accountsTitle);
        body.Children.Add(Card(_accountsPanel, padding: 0));
        body.Children.Add(BuildActionsCard());
        body.Children.Add(_notice);
        body.Children.Add(_updated);
    }

    private UIElement BuildLogo()
    {
        try
        {
            return new Image
            {
                Width = 26,
                Height = 26,
                Source = new SvgImageSource(new Uri("ms-appx:///Assets/openai-blossom-dark.svg"))
            };
        }
        catch
        {
            return new FontIcon { Glyph = "\uE943", FontSize = 22, Foreground = new SolidColorBrush(PrimaryText) };
        }
    }

    private Border BuildActiveAccountCard()
    {
        var grid = new Grid();
        grid.ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto });
        grid.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(1, GridUnitType.Star) });
        var avatar = Avatar(Color.FromArgb(150, 101, 67, 115), 38);
        grid.Children.Add(avatar);
        var labels = new StackPanel { Spacing = 2, VerticalAlignment = VerticalAlignment.Center, Margin = new Thickness(10, 0, 0, 0) };
        labels.Children.Add(_accountName);
        var statusGrid = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 6 };
        statusGrid.Children.Add(_accountStatus);
        statusGrid.Children.Add(new Border
        {
            Width = 7,
            Height = 7,
            CornerRadius = new CornerRadius(4),
            Background = new SolidColorBrush(Mint),
            VerticalAlignment = VerticalAlignment.Center
        });
        labels.Children.Add(statusGrid);
        Grid.SetColumn(labels, 1);
        grid.Children.Add(labels);
        return Card(grid, 12);
    }

    private Border BuildQuotaCard(QuotaWindow window)
    {
        var body = new StackPanel { Spacing = 6 };
        var heading = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 8 };
        heading.Children.Add(IconBubble(window.WindowDurationMins >= 1440 ? "\uE787" : "\uE823"));
        heading.Children.Add(Text(FriendlyWindowLabel(window), 13, PrimaryText, FontWeights.SemiBold));
        body.Children.Add(heading);
        var valueRow = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 6, Margin = new Thickness(34, 0, 0, 0) };
        valueRow.Children.Add(Text($"{Math.Round(window.RemainingPercent):0}%", 24, PrimaryText, FontWeights.SemiBold));
        var left = Text("left", 12, SecondaryText);
        left.VerticalAlignment = VerticalAlignment.Bottom;
        left.Margin = new Thickness(0, 0, 0, 4);
        valueRow.Children.Add(left);
        body.Children.Add(valueRow);
        var meter = new Grid
        {
            Height = 5,
            Margin = new Thickness(34, 0, 0, 0),
            Background = new SolidColorBrush(Color.FromArgb(70, 225, 220, 230))
        };
        var percentage = Math.Clamp(window.RemainingPercent, 0.001, 100);
        meter.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(percentage, GridUnitType.Star) });
        meter.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(100 - percentage, GridUnitType.Star) });
        meter.Children.Add(new Border
        {
            Background = new SolidColorBrush(Mint),
            CornerRadius = new CornerRadius(3)
        });
        body.Children.Add(meter);
        var reset = Text(ResetText(window.ResetsAt), 11, SecondaryText);
        reset.Margin = new Thickness(34, 0, 0, 0);
        body.Children.Add(reset);
        return Card(body, 10);
    }

    private Border BuildUnavailableQuotaCard(string message)
    {
        var panel = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 10 };
        panel.Children.Add(IconBubble("\uE783"));
        var label = Text(message, 12, SecondaryText);
        label.TextWrapping = TextWrapping.Wrap;
        panel.Children.Add(label);
        return Card(panel, 10);
    }

    private Grid BuildAccountRow(AccountView account)
    {
        var row = new Grid { MinHeight = 44, Padding = new Thickness(10, 6, 10, 6) };
        row.ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto });
        row.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(1, GridUnitType.Star) });
        row.ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto });
        row.Children.Add(Avatar(
            account.Kind == "api" ? Color.FromArgb(180, 44, 131, 86) : Color.FromArgb(150, 101, 67, 115),
            28));
        var label = Text(account.DisplayName, 13, PrimaryText, FontWeights.Medium);
        label.Margin = new Thickness(10, 0, 6, 0);
        label.VerticalAlignment = VerticalAlignment.Center;
        label.TextTrimming = TextTrimming.CharacterEllipsis;
        Grid.SetColumn(label, 1);
        row.Children.Add(label);
        FrameworkElement action;
        if (account.Active)
        {
            action = new FontIcon
            {
                Glyph = "\uE73E",
                FontSize = 15,
                Foreground = new SolidColorBrush(Mint),
                VerticalAlignment = VerticalAlignment.Center
            };
        }
        else
        {
            var button = new Button
            {
                Content = "Switch",
                FontSize = 12,
                Padding = new Thickness(10, 4, 10, 4),
                CornerRadius = new CornerRadius(6),
                Background = new SolidColorBrush(Color.FromArgb(40, 255, 255, 255)),
                BorderBrush = new SolidColorBrush(Color.FromArgb(64, 255, 255, 255)),
                BorderThickness = new Thickness(1)
            };
            button.Click += async (_, _) => await ActivateAccountAsync(account.Id, button);
            action = button;
        }
        Grid.SetColumn(action, 2);
        row.Children.Add(action);
        return row;
    }

    private Border BuildActionsCard()
    {
        var panel = new StackPanel();
        panel.Children.Add(ActionRow("\uE8B7", "Session Manager", () => OpenSessionManagerRequested?.Invoke(this, EventArgs.Empty)));
        panel.Children.Add(Separator());
        panel.Children.Add(ActionRow("\uE90F", "Repair now", () => RepairRequested?.Invoke(this, EventArgs.Empty)));
        panel.Children.Add(Separator());
        panel.Children.Add(ActionRow("\uE8B7", "Open Codex folder", () => OpenCodexFolderRequested?.Invoke(this, EventArgs.Empty)));
        return Card(panel, 0);
    }

    private Button ActionRow(string glyph, string label, Action action)
    {
        var grid = new Grid { Padding = new Thickness(12, 9, 10, 9) };
        grid.ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto });
        grid.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(1, GridUnitType.Star) });
        grid.ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto });
        grid.Children.Add(new FontIcon { Glyph = glyph, FontSize = 15, Foreground = new SolidColorBrush(SecondaryText) });
        var text = Text(label, 13, PrimaryText);
        text.Margin = new Thickness(10, 0, 0, 0);
        Grid.SetColumn(text, 1);
        grid.Children.Add(text);
        var chevron = new FontIcon { Glyph = "\uE76C", FontSize = 11, Foreground = new SolidColorBrush(SecondaryText) };
        Grid.SetColumn(chevron, 2);
        grid.Children.Add(chevron);

        var button = new Button
        {
            HorizontalAlignment = HorizontalAlignment.Stretch,
            HorizontalContentAlignment = HorizontalAlignment.Stretch,
            Content = grid,
            Padding = new Thickness(0),
            Background = new SolidColorBrush(Color.FromArgb(0, 0, 0, 0)),
            BorderThickness = new Thickness(0),
            CornerRadius = new CornerRadius(8)
        };
        button.Click += (_, _) => action();
        return button;
    }

    private async Task ActivateAccountAsync(string accountId, Button button)
    {
        button.IsEnabled = false;
        _updated.Text = "Switching account safely…";
        try
        {
            var state = await _core.ActivateAccountAsync(accountId);
            Render(state);
        }
        catch (Exception error)
        {
            ShowNotice(error.Message);
        }
        finally
        {
            button.IsEnabled = true;
        }
    }

    private void TryApplyBackdrop()
    {
        try
        {
            if (DesktopAcrylicController.IsSupported())
            {
                SystemBackdrop = new DesktopAcrylicBackdrop();
            }
        }
        catch
        {
            // The opaque gradient remains the Windows 10 / transparency-off fallback.
        }
    }

    private void ShowNotice(string? message)
    {
        _notice.Text = message ?? "Unknown error";
        _notice.Visibility = Visibility.Visible;
    }

    private static Border Card(UIElement child, double padding) => new()
    {
        Child = child,
        Padding = new Thickness(padding),
        CornerRadius = new CornerRadius(10),
        Background = new SolidColorBrush(CardFill),
        BorderBrush = new SolidColorBrush(CardStroke),
        BorderThickness = new Thickness(1)
    };

    private static Border Avatar(Color color, double size) => new()
    {
        Width = size,
        Height = size,
        CornerRadius = new CornerRadius(size / 2),
        Background = new SolidColorBrush(color),
        Child = new FontIcon
        {
            Glyph = "\uE77B",
            FontSize = size * 0.46,
            Foreground = new SolidColorBrush(PrimaryText)
        }
    };

    private static Border IconBubble(string glyph) => new()
    {
        Width = 26,
        Height = 26,
        CornerRadius = new CornerRadius(13),
        Background = new SolidColorBrush(Color.FromArgb(70, 168, 145, 176)),
        Child = new FontIcon { Glyph = glyph, FontSize = 13, Foreground = new SolidColorBrush(PrimaryText) }
    };

    private static Button CircleButton(string glyph, string tooltip)
    {
        var button = new Button
        {
            Width = 32,
            Height = 32,
            Padding = new Thickness(0),
            CornerRadius = new CornerRadius(16),
            Background = new SolidColorBrush(Color.FromArgb(30, 255, 255, 255)),
            BorderBrush = new SolidColorBrush(Color.FromArgb(55, 255, 255, 255)),
            BorderThickness = new Thickness(1),
            Content = new FontIcon { Glyph = glyph, FontSize = 14, Foreground = new SolidColorBrush(SecondaryText) }
        };
        ToolTipService.SetToolTip(button, tooltip);
        return button;
    }

    private static Border Separator() => new()
    {
        Height = 1,
        Background = new SolidColorBrush(Color.FromArgb(38, 255, 255, 255)),
        Margin = new Thickness(10, 0, 10, 0)
    };

    private static TextBlock Text(string value, double size, Color color, FontWeight? weight = null) => new()
    {
        Text = value,
        FontSize = size,
        FontWeight = weight ?? FontWeights.Normal,
        Foreground = new SolidColorBrush(color),
        VerticalAlignment = VerticalAlignment.Center
    };

    private static string FriendlyWindowLabel(QuotaWindow window)
    {
        var duration = window.WindowDurationMins;
        if (duration == 300 || window.Label.Equals("5h", StringComparison.OrdinalIgnoreCase))
        {
            return "5-hour limit";
        }
        if (duration == 10_080 || window.Label.Contains('w', StringComparison.OrdinalIgnoreCase))
        {
            return "Weekly limit";
        }
        return $"{window.Label} limit";
    }

    private static string ResetText(long? unixSeconds)
    {
        if (unixSeconds is null)
        {
            return "Reset time unavailable";
        }
        var reset = DateTimeOffset.FromUnixTimeSeconds(unixSeconds.Value).ToLocalTime();
        var remaining = reset - DateTimeOffset.Now;
        if (remaining > TimeSpan.Zero && remaining < TimeSpan.FromDays(1))
        {
            return $"Resets in {(int)remaining.TotalHours}h {remaining.Minutes}m";
        }
        return $"Resets {reset.ToString("ddd, HH:mm", EnglishUiCulture)}";
    }

    private static string RelativeTime(DateTimeOffset time)
    {
        var age = DateTimeOffset.UtcNow - time.ToUniversalTime();
        if (age < TimeSpan.FromMinutes(1)) return "just now";
        if (age < TimeSpan.FromHours(1)) return $"{Math.Max(1, (int)age.TotalMinutes)}m ago";
        return time.ToLocalTime().ToString("g", EnglishUiCulture);
    }
}
