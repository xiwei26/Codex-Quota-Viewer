using CodexQuotaViewer.WinUI;
using Xunit;

namespace CodexQuotaViewer.WinUI.Tests;

public sealed class WidgetBehaviorTests
{
    [Fact]
    public void UsesActiveApiRowWhenQuotaRefreshHasNoSnapshot()
    {
        var state = StateWithActiveAccount("Workspace API", "api");

        var presentation = DashboardPresentation.Resolve(state);

        Assert.Equal("Workspace API", presentation.AccountName);
        Assert.Contains("Active API account", presentation.AccountStatus);
        Assert.True(presentation.IsApiAccount);
        Assert.StartsWith("API accounts", presentation.UnavailableQuotaMessage);
        Assert.StartsWith("Quota refresh failed.", DashboardPresentation.RefreshErrorNotice(state));
    }

    [Fact]
    public void UsesActiveChatGptRowWhenQuotaRefreshHasNoSnapshot()
    {
        var state = StateWithActiveAccount("Personal", "chatGpt");

        var presentation = DashboardPresentation.Resolve(state);

        Assert.Equal("Personal", presentation.AccountName);
        Assert.Contains("Active ChatGPT account", presentation.AccountStatus);
        Assert.False(presentation.IsApiAccount);
        Assert.Contains("this ChatGPT account", presentation.UnavailableQuotaMessage);
    }

    [Fact]
    public void DeactivationGuardAndReactivationPreventHide()
    {
        var now = DateTimeOffset.UtcNow;
        var guard = now.AddMilliseconds(450);

        Assert.False(WindowActivationPolicy.ShouldHide(true, false, false, false, now, guard));
        Assert.False(WindowActivationPolicy.ShouldHide(true, false, true, false, guard, guard));
        Assert.False(WindowActivationPolicy.ShouldHide(true, false, false, true, guard, guard));
        Assert.True(WindowActivationPolicy.ShouldHide(true, false, false, false, guard, guard));
        Assert.Equal(TimeSpan.FromMilliseconds(550), WindowActivationPolicy.DelayBeforeRecheck(now, guard));
    }

    private static DashboardState StateWithActiveAccount(string displayName, string kind) => new()
    {
        ActiveAccountId = "acct-active",
        Accounts =
        [
            new AccountView
            {
                Id = "acct-active",
                DisplayName = displayName,
                Kind = kind,
                Active = true
            }
        ],
        LastError = new CoreHostError
        {
            Code = "quotaTimeout",
            Message = "Quota refresh timed out"
        }
    };
}
