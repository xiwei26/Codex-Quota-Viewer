using Microsoft.UI.Dispatching;

namespace CodexQuotaViewer.WinUI;

public sealed class SingleInstanceService : IDisposable
{
    private const string MutexName = @"Local\CodexQuotaViewer.WinUI.Singleton.v1";
    private const string WakeEventName = @"Local\CodexQuotaViewer.WinUI.Wake.v1";
    private readonly Mutex _mutex;
    private readonly EventWaitHandle? _wakeEvent;
    private readonly DispatcherQueue _dispatcher;
    private readonly CancellationTokenSource _stop = new();
    private Task? _listener;

    public bool IsPrimary { get; }

    public SingleInstanceService(DispatcherQueue dispatcher)
    {
        _dispatcher = dispatcher;
        var wakeEvent = new EventWaitHandle(false, EventResetMode.AutoReset, WakeEventName);
        _mutex = new Mutex(initiallyOwned: true, MutexName, out var createdNew);
        IsPrimary = createdNew;
        if (!createdNew)
        {
            wakeEvent.Set();
            wakeEvent.Dispose();
            return;
        }

        _wakeEvent = wakeEvent;
    }

    public void StartListening(Action activated)
    {
        if (!IsPrimary)
        {
            throw new InvalidOperationException("Only the primary instance can listen for activation.");
        }
        if (_listener is not null)
        {
            throw new InvalidOperationException("The single-instance listener is already running.");
        }
        var wakeEvent = _wakeEvent
            ?? throw new InvalidOperationException("The primary activation event is unavailable.");

        _listener = Task.Run(() =>
        {
            while (!_stop.IsCancellationRequested)
            {
                if (!wakeEvent.WaitOne(TimeSpan.FromMilliseconds(500)))
                {
                    continue;
                }
                if (_stop.IsCancellationRequested)
                {
                    break;
                }
                _dispatcher.TryEnqueue(() => activated());
            }
        });
    }

    public void Dispose()
    {
        _stop.Cancel();
        _wakeEvent?.Set();
        try
        {
            _listener?.Wait(TimeSpan.FromSeconds(1));
        }
        catch
        {
        }
        _wakeEvent?.Dispose();
        if (IsPrimary)
        {
            try { _mutex.ReleaseMutex(); } catch { }
        }
        _mutex.Dispose();
        _stop.Dispose();
    }
}
