using System.Runtime.ExceptionServices;

namespace CodexQuotaViewer.WinUI;

public static class SettingsSaveTransaction
{
    public static async Task ApplyAsync<TSettings, TRunSnapshot>(
        TSettings previousSettings,
        TSettings newSettings,
        bool launchAtLoginEnabled,
        Func<TRunSnapshot> captureRunValue,
        Action<bool> applyRunValue,
        Action<TRunSnapshot> restoreRunValue,
        Func<TSettings, Task> saveSettings)
    {
        var runSnapshot = captureRunValue();
        var runMutationAttempted = false;
        var settingsSaveAttempted = false;
        try
        {
            runMutationAttempted = true;
            applyRunValue(launchAtLoginEnabled);
            settingsSaveAttempted = true;
            await saveSettings(newSettings);
        }
        catch (Exception failure)
        {
            var rollbackFailures = new List<Exception>();
            if (settingsSaveAttempted)
            {
                try
                {
                    await saveSettings(previousSettings);
                }
                catch (Exception rollbackFailure)
                {
                    rollbackFailures.Add(rollbackFailure);
                }
            }
            if (runMutationAttempted)
            {
                try
                {
                    restoreRunValue(runSnapshot);
                }
                catch (Exception rollbackFailure)
                {
                    rollbackFailures.Add(rollbackFailure);
                }
            }

            if (rollbackFailures.Count > 0)
            {
                throw new AggregateException(
                    "Saving settings failed and rollback was incomplete.",
                    new[] { failure }.Concat(rollbackFailures));
            }
            ExceptionDispatchInfo.Capture(failure).Throw();
            throw;
        }
    }
}
