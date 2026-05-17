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
  point for `auth.json` and `config.toml`.
- Rolls back the latest Safe Switch restore point from the Accounts page or
  tray menu.
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

This phase does not yet close/reopen Codex automatically or run the macOS
thread/provider repair flow.

## Third-party Provider Mode

Windows can now mirror the macOS 1.2.0 Provider-mode workflow at the account
file level. In **Settings... -> Accounts**, choose a saved API account and use
**Use as Provider**. The app backs up the current `auth.json` and
`config.toml`, keeps the active Codex auth in ChatGPT mode, and writes a
third-party OpenAI-compatible provider into `config.toml`.

Use **Switch Back from Provider** to restore the previous files. This Windows
phase does not yet close/reopen Codex automatically or run the macOS Safe
Switch thread/provider repair flow.

## Not Included In The MVP

- Codex close/reopen orchestration around account switching.
- Thread/provider repair after account activation or Provider-mode changes.

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
