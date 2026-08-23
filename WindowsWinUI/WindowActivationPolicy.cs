namespace CodexQuotaViewer.WinUI;

public static class WindowActivationPolicy
{
    public static bool ShouldHide(
        bool isShown,
        bool isContextMenuVisible,
        bool isWindowActive,
        bool isForegroundWindow,
        DateTimeOffset now,
        DateTimeOffset ignoreDeactivationUntil) =>
        isShown &&
        !isContextMenuVisible &&
        !isWindowActive &&
        !isForegroundWindow &&
        now >= ignoreDeactivationUntil;

    public static TimeSpan DelayBeforeRecheck(
        DateTimeOffset now,
        DateTimeOffset ignoreDeactivationUntil)
    {
        var guardRemaining = ignoreDeactivationUntil > now
            ? ignoreDeactivationUntil - now
            : TimeSpan.Zero;
        return guardRemaining + TimeSpan.FromMilliseconds(100);
    }
}
