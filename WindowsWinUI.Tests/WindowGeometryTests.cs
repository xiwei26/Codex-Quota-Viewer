using CodexQuotaViewer.WinUI;
using Xunit;

namespace CodexQuotaViewer.WinUI.Tests;

public sealed class WindowGeometryTests
{
    [Fact]
    public void PositionsNinetySixDpiWidgetInsideWorkArea()
    {
        var placement = WidgetPlacement.ForWorkArea(new PixelRect(0, 0, 1920, 1040), 96);

        Assert.Equal(1468, placement.VisibleX);
        Assert.Equal(1932, placement.HiddenX);
        Assert.Equal(90, placement.Y);
        Assert.Equal(440, placement.Width);
        Assert.Equal(860, placement.Height);
    }

    [Fact]
    public void ScalesForSecondaryMonitorAndKeepsPhysicalCoordinates()
    {
        var placement = WidgetPlacement.ForWorkArea(new PixelRect(1920, 0, 4480, 1400), 144);

        Assert.Equal(3802, placement.VisibleX);
        Assert.Equal(4498, placement.HiddenX);
        Assert.Equal(55, placement.Y);
        Assert.Equal(660, placement.Width);
        Assert.Equal(1290, placement.Height);
    }

    [Fact]
    public void AvoidsTaskbarByUsingTheProvidedWorkArea()
    {
        var placement = WidgetPlacement.ForWorkArea(new PixelRect(48, 0, 1280, 720), 96);

        Assert.Equal(828, placement.VisibleX);
        Assert.Equal(1292, placement.HiddenX);
        Assert.Equal(12, placement.Y);
        Assert.Equal(696, placement.Height);
    }

    [Fact]
    public void ShrinksBelowNominalMinimumsWhenWorkAreaIsTiny()
    {
        var workArea = new PixelRect(100, 50, 380, 410);

        var placement = WidgetPlacement.ForWorkArea(workArea, 96);

        Assert.Equal(112, placement.VisibleX);
        Assert.Equal(392, placement.HiddenX);
        Assert.Equal(62, placement.Y);
        Assert.Equal(256, placement.Width);
        Assert.Equal(336, placement.Height);
        Assert.True(placement.VisibleX >= workArea.Left);
        Assert.True(placement.VisibleX + placement.Width <= workArea.Right);
        Assert.True(placement.Y + placement.Height <= workArea.Bottom);
    }

    [Fact]
    public void RejectsAnEmptyWorkArea()
    {
        Assert.Throws<ArgumentOutOfRangeException>(() =>
            WidgetPlacement.ForWorkArea(new PixelRect(0, 0, 0, 720), 96));
    }

    [Theory]
    [InlineData(-1, 0)]
    [InlineData(0, 0)]
    [InlineData(0.5, 0.875)]
    [InlineData(1, 1)]
    [InlineData(2, 1)]
    public void EasingIsClamped(double progress, double expected)
    {
        Assert.Equal(expected, WidgetPlacement.EaseOutCubic(progress), precision: 6);
    }
}
