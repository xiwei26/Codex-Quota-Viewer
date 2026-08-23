param(
  [ValidateSet("Debug", "Release")]
  [string] $Configuration = "Release"
)

$ErrorActionPreference = "Stop"
$repoRoot = Resolve-Path (Join-Path $PSScriptRoot "..")
$publishedExe = Join-Path $repoRoot "dist\CodexQuotaViewer.WinUI\CodexQuotaViewer.WinUI.exe"
$buildExe = Join-Path $repoRoot "WindowsWinUI\bin\x64\$Configuration\net8.0-windows10.0.19041.0\win-x64\CodexQuotaViewer.WinUI.exe"
$executable = if (Test-Path $publishedExe) { $publishedExe } else { $buildExe }

if (!(Test-Path $executable)) {
  throw "Native widget has not been built. Run scripts\build-windows-winui.ps1 first."
}

Start-Process -FilePath $executable -WindowStyle Hidden
Write-Host "Codex Quota Viewer is running in the notification area."
