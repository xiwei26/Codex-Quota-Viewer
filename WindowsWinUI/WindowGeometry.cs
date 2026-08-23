namespace CodexQuotaViewer.WinUI;

public readonly record struct PixelRect(int Left, int Top, int Right, int Bottom)
{
    public int Width => Right - Left;
    public int Height => Bottom - Top;
}

public readonly record struct WidgetPlacement(
    int VisibleX,
    int HiddenX,
    int Y,
    int Width,
    int Height)
{
    public static WidgetPlacement ForWorkArea(
        PixelRect workArea,
        uint dpi,
        double desiredWidthDip = 360,
        double desiredHeightDip = 520,
        double marginDip = 12)
    {
        if (workArea.Width <= 0 || workArea.Height <= 0)
        {
            throw new ArgumentOutOfRangeException(nameof(workArea), "The monitor work area must have a positive size.");
        }

        var scale = Math.Max(0.5, dpi / 96.0);
        var margin = Math.Max(4, (int)Math.Round(marginDip * scale));
        var horizontalMargin = Math.Min(margin, Math.Max(0, (workArea.Width - 1) / 2));
        var verticalMargin = Math.Min(margin, Math.Max(0, (workArea.Height - 1) / 2));
        var availableWidth = workArea.Width - (horizontalMargin * 2);
        var availableHeight = workArea.Height - (verticalMargin * 2);
        var width = Math.Min(
            Math.Max(300, (int)Math.Round(desiredWidthDip * scale)),
            availableWidth);
        var height = Math.Min(
            Math.Max(400, (int)Math.Round(desiredHeightDip * scale)),
            availableHeight);
        var x = workArea.Right - horizontalMargin - width;
        var y = workArea.Bottom - verticalMargin - height;
        return new WidgetPlacement(x, workArea.Right + horizontalMargin, y, width, height);
    }

    public static double EaseOutCubic(double progress)
    {
        progress = Math.Clamp(progress, 0, 1);
        return 1 - Math.Pow(1 - progress, 3);
    }
}
