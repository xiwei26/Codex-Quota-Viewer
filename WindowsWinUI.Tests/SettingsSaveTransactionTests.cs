using CodexQuotaViewer.WinUI;
using Xunit;

namespace CodexQuotaViewer.WinUI.Tests;

public sealed class SettingsSaveTransactionTests
{
    [Theory]
    [InlineData(true)]
    [InlineData(false)]
    public async Task RegistryFailureRestoresExactRunSnapshotWithoutSaving(bool enable)
    {
        var runValue = "old-command";
        var settings = "old-settings";
        var saveCalls = 0;

        await Assert.ThrowsAsync<InvalidOperationException>(() =>
            SettingsSaveTransaction.ApplyAsync(
                "old-settings",
                "new-settings",
                enable,
                () => runValue,
                _ =>
                {
                    runValue = "partially-mutated";
                    throw new InvalidOperationException("registry failed");
                },
                snapshot => runValue = snapshot,
                value =>
                {
                    saveCalls++;
                    settings = value;
                    return Task.CompletedTask;
                }));

        Assert.Equal("old-command", runValue);
        Assert.Equal("old-settings", settings);
        Assert.Equal(0, saveCalls);
    }

    [Theory]
    [InlineData(true)]
    [InlineData(false)]
    public async Task SettingsFailureRestoresSettingsAndExactRunSnapshot(bool enable)
    {
        var runValue = "old-command";
        var settings = "old-settings";
        var saveCalls = 0;

        await Assert.ThrowsAsync<InvalidOperationException>(() =>
            SettingsSaveTransaction.ApplyAsync(
                "old-settings",
                "new-settings",
                enable,
                () => runValue,
                enabled => runValue = enabled ? "new-command" : null,
                snapshot => runValue = snapshot,
                value =>
                {
                    saveCalls++;
                    settings = value;
                    if (value == "new-settings")
                    {
                        throw new InvalidOperationException("settings failed");
                    }
                    return Task.CompletedTask;
                }));

        Assert.Equal("old-command", runValue);
        Assert.Equal("old-settings", settings);
        Assert.Equal(2, saveCalls);
    }
}
