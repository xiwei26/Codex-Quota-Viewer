using System.Runtime.InteropServices;

namespace CodexQuotaViewer.WinUI;

internal static class NativeMethods
{
    internal const int GwlExStyle = -20;
    internal const int GwlpWndProc = -4;
    internal const long WsExToolWindow = 0x00000080L;
    internal const long WsExAppWindow = 0x00040000L;
    internal const uint WmApp = 0x8000;
    internal const uint WmTrayIcon = WmApp + 41;
    internal const uint WmLButtonUp = 0x0202;
    internal const uint WmRButtonUp = 0x0205;
    internal const uint WmContextMenu = 0x007B;
    internal const uint NinSelect = 0x0400;
    internal const uint NinKeySelect = 0x0401;
    internal const uint WmNull = 0x0000;
    internal const uint MonitorDefaultToNearest = 2;
    internal const uint NifMessage = 0x00000001;
    internal const uint NifIcon = 0x00000002;
    internal const uint NifTip = 0x00000004;
    internal const uint NimAdd = 0x00000000;
    internal const uint NimDelete = 0x00000002;
    internal const uint NimSetVersion = 0x00000004;
    internal const uint NotifyIconVersion4 = 4;
    internal const uint ImageIcon = 1;
    internal const uint LrLoadFromFile = 0x00000010;
    internal const uint LrDefaultSize = 0x00000040;
    internal const uint MfString = 0x00000000;
    internal const uint MfSeparator = 0x00000800;
    internal const uint TpmRightAlign = 0x0008;
    internal const uint TpmBottomAlign = 0x0020;
    internal const uint TpmReturnCmd = 0x0100;
    internal const uint TpmNonotify = 0x0080;

    [StructLayout(LayoutKind.Sequential)]
    internal struct Point
    {
        internal int X;
        internal int Y;
    }

    [StructLayout(LayoutKind.Sequential)]
    internal struct Rect
    {
        internal int Left;
        internal int Top;
        internal int Right;
        internal int Bottom;
    }

    [StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)]
    internal struct MonitorInfo
    {
        internal uint Size;
        internal Rect Monitor;
        internal Rect Work;
        internal uint Flags;
    }

    [StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)]
    internal struct NotifyIconData
    {
        internal uint Size;
        internal nint Window;
        internal uint Id;
        internal uint Flags;
        internal uint CallbackMessage;
        internal nint Icon;

        [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 128)]
        internal string Tip;

        internal uint State;
        internal uint StateMask;

        [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 256)]
        internal string Info;

        internal uint VersionOrTimeout;

        [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 64)]
        internal string InfoTitle;

        internal uint InfoFlags;
        internal Guid Guid;
        internal nint BalloonIcon;
    }

    internal delegate nint WindowProc(nint window, uint message, nint wParam, nint lParam);

    [DllImport("shell32.dll", CharSet = CharSet.Unicode)]
    [return: MarshalAs(UnmanagedType.Bool)]
    internal static extern bool Shell_NotifyIcon(uint message, ref NotifyIconData data);

    [DllImport("user32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
    internal static extern nint LoadImage(nint instance, string name, uint type, int width, int height, uint load);

    [DllImport("user32.dll", SetLastError = true)]
    [return: MarshalAs(UnmanagedType.Bool)]
    internal static extern bool DestroyIcon(nint icon);

    [DllImport("user32.dll", EntryPoint = "SetWindowLongPtrW", SetLastError = true)]
    internal static extern nint SetWindowLongPtr(nint window, int index, nint newValue);

    [DllImport("user32.dll", EntryPoint = "GetWindowLongPtrW", SetLastError = true)]
    internal static extern nint GetWindowLongPtr(nint window, int index);

    [DllImport("user32.dll")]
    internal static extern nint CallWindowProc(nint previous, nint window, uint message, nint wParam, nint lParam);

    [DllImport("user32.dll")]
    [return: MarshalAs(UnmanagedType.Bool)]
    internal static extern bool GetCursorPos(out Point point);

    [DllImport("user32.dll")]
    internal static extern nint MonitorFromPoint(Point point, uint flags);

    [DllImport("user32.dll", CharSet = CharSet.Unicode)]
    [return: MarshalAs(UnmanagedType.Bool)]
    internal static extern bool GetMonitorInfo(nint monitor, ref MonitorInfo info);

    [DllImport("shcore.dll")]
    internal static extern int GetDpiForMonitor(nint monitor, int dpiType, out uint dpiX, out uint dpiY);

    [DllImport("user32.dll")]
    internal static extern uint GetDpiForWindow(nint window);

    [DllImport("user32.dll", SetLastError = true)]
    internal static extern nint CreatePopupMenu();

    [DllImport("user32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
    [return: MarshalAs(UnmanagedType.Bool)]
    internal static extern bool AppendMenu(nint menu, uint flags, nuint id, string? text);

    [DllImport("user32.dll", SetLastError = true)]
    internal static extern uint TrackPopupMenuEx(nint menu, uint flags, int x, int y, nint window, nint parameters);

    [DllImport("user32.dll", SetLastError = true)]
    [return: MarshalAs(UnmanagedType.Bool)]
    internal static extern bool DestroyMenu(nint menu);

    [DllImport("user32.dll")]
    [return: MarshalAs(UnmanagedType.Bool)]
    internal static extern bool SetForegroundWindow(nint window);

    [DllImport("user32.dll")]
    internal static extern nint GetForegroundWindow();

    [DllImport("user32.dll")]
    internal static extern uint GetWindowThreadProcessId(nint hWnd, out uint lpdwProcessId);

    [DllImport("kernel32.dll")]
    internal static extern uint GetCurrentThreadId();

    [DllImport("user32.dll")]
    [return: MarshalAs(UnmanagedType.Bool)]
    internal static extern bool AttachThreadInput(uint idAttach, uint idAttachTo, [MarshalAs(UnmanagedType.Bool)] bool fAttach);

    [DllImport("user32.dll")]
    [return: MarshalAs(UnmanagedType.Bool)]
    internal static extern bool BringWindowToTop(nint hWnd);

    [DllImport("user32.dll")]
    [return: MarshalAs(UnmanagedType.Bool)]
    internal static extern bool PostMessage(nint window, uint message, nint wParam, nint lParam);

    [DllImport("user32.dll", CharSet = CharSet.Unicode)]
    internal static extern uint RegisterWindowMessage(string message);

    internal static void ForceForeground(nint window)
    {
        if (window == nint.Zero)
        {
            return;
        }

        var foregroundWindow = GetForegroundWindow();
        if (foregroundWindow == window)
        {
            return;
        }

        var currentThreadId = GetCurrentThreadId();
        var foregroundThreadId = foregroundWindow != nint.Zero
            ? GetWindowThreadProcessId(foregroundWindow, out _)
            : 0;

        var attached = false;
        if (foregroundThreadId != 0 && foregroundThreadId != currentThreadId)
        {
            attached = AttachThreadInput(currentThreadId, foregroundThreadId, true);
        }

        try
        {
            BringWindowToTop(window);
            SetForegroundWindow(window);
        }
        finally
        {
            if (attached)
            {
                AttachThreadInput(currentThreadId, foregroundThreadId, false);
            }
        }
    }

    internal static PixelRect WorkAreaFromCursor(nint fallbackWindow, out uint dpi)
    {
        GetCursorPos(out var cursor);
        var monitor = MonitorFromPoint(cursor, MonitorDefaultToNearest);
        var info = new MonitorInfo { Size = (uint)Marshal.SizeOf<MonitorInfo>() };
        if (!GetMonitorInfo(monitor, ref info))
        {
            throw new InvalidOperationException("Could not read the current display work area.");
        }
        dpi = GetDpiForMonitor(monitor, 0, out var dpiX, out _) == 0
            ? dpiX
            : Math.Max(96, GetDpiForWindow(fallbackWindow));
        return new PixelRect(info.Work.Left, info.Work.Top, info.Work.Right, info.Work.Bottom);
    }
}
