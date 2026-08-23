using System.Runtime.InteropServices;

namespace CodexQuotaViewer.WinUI;

public enum TrayCommand
{
    Refresh = 1,
    Settings = 2,
    SessionManager = 3,
    Repair = 4,
    OpenCodexFolder = 5,
    Rollback = 6,
    Quit = 7
}

public sealed class TrayIconService : IDisposable
{
    private readonly nint _window;
    private readonly NativeMethods.WindowProc _windowProc;
    private readonly nint _previousWindowProc;
    private readonly uint _taskbarCreatedMessage;
    private NativeMethods.NotifyIconData _iconData;
    private nint _icon;
    private bool _disposed;

    public event EventHandler? LeftClicked;
    public event EventHandler<TrayCommand>? CommandInvoked;
    public event EventHandler<bool>? ContextMenuVisibilityChanged;

    public TrayIconService(nint window)
    {
        _window = window;
        _taskbarCreatedMessage = NativeMethods.RegisterWindowMessage("TaskbarCreated");
        _windowProc = WndProc;
        _previousWindowProc = NativeMethods.SetWindowLongPtr(
            window,
            NativeMethods.GwlpWndProc,
            Marshal.GetFunctionPointerForDelegate(_windowProc));

        var iconPath = Path.Combine(AppContext.BaseDirectory, "Assets", "icon.ico");
        _icon = NativeMethods.LoadImage(
            0,
            iconPath,
            NativeMethods.ImageIcon,
            0,
            0,
            NativeMethods.LrLoadFromFile | NativeMethods.LrDefaultSize);
        if (_icon == 0)
        {
            throw new InvalidOperationException($"Could not load tray icon: {iconPath}");
        }

        _iconData = new NativeMethods.NotifyIconData
        {
            Size = (uint)Marshal.SizeOf<NativeMethods.NotifyIconData>(),
            Window = window,
            Id = 1,
            Flags = NativeMethods.NifMessage | NativeMethods.NifIcon | NativeMethods.NifTip,
            CallbackMessage = NativeMethods.WmTrayIcon,
            Icon = _icon,
            Tip = "Codex Quota Viewer",
            Info = string.Empty,
            InfoTitle = string.Empty
        };
        InstallIcon(required: true);
    }

    private nint WndProc(nint window, uint message, nint wParam, nint lParam)
    {
        if (_taskbarCreatedMessage != 0 && message == _taskbarCreatedMessage)
        {
            InstallIcon(required: false);
            return 0;
        }
        if (message == NativeMethods.WmTrayIcon)
        {
            // NOTIFYICON_VERSION_4 packs the event in LOWORD(lParam).
            switch ((uint)(lParam.ToInt64() & 0xffff))
            {
                case NativeMethods.WmLButtonUp:
                case NativeMethods.NinSelect:
                case NativeMethods.NinKeySelect:
                    LeftClicked?.Invoke(this, EventArgs.Empty);
                    return 0;
                case NativeMethods.WmRButtonUp:
                case NativeMethods.WmContextMenu:
                    ShowContextMenu();
                    return 0;
            }
        }
        return NativeMethods.CallWindowProc(_previousWindowProc, window, message, wParam, lParam);
    }

    private void InstallIcon(bool required)
    {
        if (!NativeMethods.Shell_NotifyIcon(NativeMethods.NimAdd, ref _iconData))
        {
            if (required)
            {
                throw new InvalidOperationException("Windows rejected the notification area icon.");
            }
            return;
        }
        _iconData.VersionOrTimeout = NativeMethods.NotifyIconVersion4;
        NativeMethods.Shell_NotifyIcon(NativeMethods.NimSetVersion, ref _iconData);
    }

    private void ShowContextMenu()
    {
        var menu = NativeMethods.CreatePopupMenu();
        if (menu == 0)
        {
            return;
        }
        ContextMenuVisibilityChanged?.Invoke(this, true);
        try
        {
            Add(menu, TrayCommand.Refresh, "Refresh quota");
            Add(menu, TrayCommand.Settings, "Settings…");
            NativeMethods.AppendMenu(menu, NativeMethods.MfSeparator, 0, null);
            Add(menu, TrayCommand.SessionManager, "Open Session Manager");
            Add(menu, TrayCommand.Repair, "Repair now");
            Add(menu, TrayCommand.Rollback, "Rollback last change");
            Add(menu, TrayCommand.OpenCodexFolder, "Open Codex folder");
            NativeMethods.AppendMenu(menu, NativeMethods.MfSeparator, 0, null);
            Add(menu, TrayCommand.Quit, "Quit");

            NativeMethods.GetCursorPos(out var cursor);
            NativeMethods.SetForegroundWindow(_window);
            var command = NativeMethods.TrackPopupMenuEx(
                menu,
                NativeMethods.TpmRightAlign | NativeMethods.TpmBottomAlign |
                NativeMethods.TpmReturnCmd | NativeMethods.TpmNonotify,
                cursor.X,
                cursor.Y,
                _window,
                0);
            NativeMethods.PostMessage(_window, NativeMethods.WmNull, 0, 0);
            if (Enum.IsDefined(typeof(TrayCommand), (int)command))
            {
                CommandInvoked?.Invoke(this, (TrayCommand)command);
            }
        }
        finally
        {
            NativeMethods.DestroyMenu(menu);
            ContextMenuVisibilityChanged?.Invoke(this, false);
        }
    }

    private static void Add(nint menu, TrayCommand command, string text) =>
        NativeMethods.AppendMenu(menu, NativeMethods.MfString, (nuint)command, text);

    public void Dispose()
    {
        if (_disposed)
        {
            return;
        }
        _disposed = true;
        NativeMethods.Shell_NotifyIcon(NativeMethods.NimDelete, ref _iconData);
        if (_previousWindowProc != 0)
        {
            NativeMethods.SetWindowLongPtr(_window, NativeMethods.GwlpWndProc, _previousWindowProc);
        }
        if (_icon != 0)
        {
            NativeMethods.DestroyIcon(_icon);
            _icon = 0;
        }
    }
}
