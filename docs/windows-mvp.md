# Windows Tray MVP

The Windows MVP is a Tauri-based system tray app for Codex Quota Viewer.

## Features

- Shows the current active Codex account quota in the Windows system tray menu.
- Supports manual quota refresh.
- Opens a General settings window from the tray.
- Persists refresh interval, language, tray style, and launch-at-login settings.
- Supports automatic quota refresh when the refresh interval is not `Manual`.
- Opens the bundled Session Manager on `http://127.0.0.1:4318`.
- Opens the local Codex folder.
- Quits cleanly and stops only the Session Manager process it started.
- Saves multiple ChatGPT and API accounts in a local Windows account vault.
- Imports the current local ChatGPT Codex login as a saved account.
- Adds OpenAI-compatible API accounts from the Windows settings window.
- Safely activates saved accounts into the resolved Codex home with a restore
  point for `auth.json`, `config.toml`, and any session JSONL rollout metadata
  files that need provider synchronization.
- Rolls back the latest Safe Switch restore point from the Accounts page or
  tray menu.
- Closes the Windows Codex desktop process before Safe Switch or Provider-mode
  file changes and reopens it afterward when it was running.
- Automatically repairs local official thread metadata through the bundled
  Session Manager after Safe Switch and Provider-mode changes.
- Synchronizes historical session JSONL rollout provider metadata during Safe
  Switch and Provider-mode entry, with rollback coverage.
- Uses a saved API account as a third-party Provider while keeping the current
  ChatGPT login active.
- Restores the previous `auth.json` and `config.toml` when leaving
  third-party Provider mode.
- Shows saved accounts in an `All Accounts` tray submenu.

## Local Data

The MVP reads the active Codex profile from `%USERPROFILE%\.codex` unless
`CODEX_HOME` is set.

## Safe Switch And Rollback

Windows account activation now creates a restore point before writing the
selected account into the resolved Codex home. The restore point covers:

- `auth.json`
- `config.toml`

If activation fails after the restore point is created, the app automatically
restores the previous files. You can also use **Rollback Last Change** from the
Accounts page or tray menu to restore the latest Safe Switch restore point.

The Windows app closes the Codex desktop process before Safe Switch writes and
reopens it afterward when it was running. It filters out CLI `bin\codex.exe`
processes so command-line Codex sessions are not targeted by the desktop
control step.

After a Safe Switch, the app asks the bundled Session Manager to rescan local
sessions and repair official Codex thread metadata. It also rewrites
`session_meta.payload.model_provider` in historical session JSONL files when the
target account uses a different provider, matching the macOS Safe Switch
behavior. Use **Repair Now** from the Accounts page or tray menu when you want
to run the same repair flow manually.

## Third-party Provider Mode

Windows can now mirror the macOS 1.2.0 Provider-mode workflow at the account
and local-session metadata level. In **Settings... -> Accounts**, choose a
saved API account and use **Use as Provider**. The app creates a restore point
for the current `auth.json`, `config.toml`, Provider-mode state, and any
historical session JSONL files that need rollout provider synchronization. It
keeps the active Codex auth in ChatGPT mode, writes a third-party
OpenAI-compatible provider into `config.toml`, and updates rollout provider
metadata to match.

Use **Switch Back from Provider** to restore the Provider-mode restore point.
Provider-mode entry and exit also use the same Codex desktop close/reopen
guard. Older Provider-mode state files created by earlier Windows builds can
still be exited through the legacy `.bak` restore path.

## Not Included In The MVP

- Windows does not yet expose the macOS local thread sync inspector UI that
  summarizes provider counts before a switch.

## Build

Prerequisites:

- Node available through `PATH`.
- Rust/Cargo available through `PATH`.
- Windows native build tools required by Tauri.

Run on Windows:

```powershell
scripts\build-windows-tray.ps1
```

The build script stages the bundled Session Manager, installs its production
dependencies, and prepares `WindowsTray\src-tauri\NodeRuntime\node.exe` in the
ignored staging directory. If `node.exe` is not already staged, the script first
copies the local Node executable from `PATH`; if Node is not installed locally,
it downloads the official Windows Node v22 runtime.
