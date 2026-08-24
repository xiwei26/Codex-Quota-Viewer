using System.Globalization;

namespace CodexQuotaViewer.WinUI;

internal static class RuntimeLog
{
    private static readonly object Sync = new();
    private static readonly string LogDirectory = Path.Combine(
        Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData),
        "com.halfmelon.codexquotaviewer.windows",
        "Logs");

    internal static string FilePath => Path.Combine(LogDirectory, "winui-runtime.log");

    internal static void Write(string message)
    {
        try
        {
            lock (Sync)
            {
                Directory.CreateDirectory(LogDirectory);
                var line = string.Create(
                    CultureInfo.InvariantCulture,
                    $"{DateTimeOffset.Now:O} pid={Environment.ProcessId} tid={Environment.CurrentManagedThreadId} {message}{Environment.NewLine}");
                File.AppendAllText(FilePath, line);
            }
        }
        catch
        {
            // Diagnostics must never prevent the tray app from starting or responding.
        }
    }
}
