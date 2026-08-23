param(
  [ValidateSet("Debug", "Release")]
  [string] $Configuration = "Release",
  [switch] $SkipTests,
  [switch] $SkipSessionManager
)

$ErrorActionPreference = "Stop"
$repoRoot = Resolve-Path (Join-Path $PSScriptRoot "..")
$project = Join-Path $repoRoot "WindowsWinUI\CodexQuotaViewer.WinUI.csproj"
$testProject = Join-Path $repoRoot "WindowsWinUI.Tests\CodexQuotaViewer.WinUI.Tests.csproj"
$coreManifest = Join-Path $repoRoot "WindowsCoreHost\Cargo.toml"
$publishRoot = Join-Path $repoRoot "dist\CodexQuotaViewer.WinUI"

function Invoke-Checked {
  param(
    [Parameter(Mandatory = $true)] [scriptblock] $Command,
    [Parameter(Mandatory = $true)] [string] $Description
  )
  & $Command
  if ($LASTEXITCODE -ne 0) {
    throw "$Description failed with exit code $LASTEXITCODE"
  }
}

if (!$SkipTests) {
  Invoke-Checked { cargo test --manifest-path $coreManifest } "CoreHost tests"
  Invoke-Checked { dotnet test $testProject -c $Configuration } "Window geometry tests"
}

if (!$SkipSessionManager) {
  & (Join-Path $PSScriptRoot "build-session-manager-windows.ps1")
  if ($LASTEXITCODE -ne 0) {
    throw "Session Manager build failed with exit code $LASTEXITCODE"
  }
}

Invoke-Checked {
  dotnet publish $project -c $Configuration -p:Platform=x64 -r win-x64 --self-contained false -o $publishRoot
} "WinUI publish"

if (!$SkipSessionManager) {
  $sessionSource = Join-Path $repoRoot "WindowsTray\src-tauri\SessionManager"
  $sessionTarget = Join-Path $publishRoot "SessionManager"
  if (!(Test-Path (Join-Path $sessionSource "dist\server\index.js"))) {
    throw "Built Session Manager entry point is missing."
  }
  New-Item -ItemType Directory -Force $sessionTarget | Out-Null
  Copy-Item -Path (Join-Path $sessionSource "*") -Destination $sessionTarget -Recurse -Force

  $node = Get-Command node -ErrorAction Stop
  $nodeTarget = Join-Path $publishRoot "NodeRuntime"
  New-Item -ItemType Directory -Force $nodeTarget | Out-Null
  Copy-Item -LiteralPath $node.Source -Destination (Join-Path $nodeTarget "node.exe") -Force
}

$appExe = Join-Path $publishRoot "CodexQuotaViewer.WinUI.exe"
$coreExe = Join-Path $publishRoot "codex-quota-viewer-core-host.exe"
if (!(Test-Path $appExe) -or !(Test-Path $coreExe)) {
  throw "Published native app or CoreHost is missing."
}

Write-Host "Native WinUI widget published to $publishRoot"
Write-Host "Run $appExe; it starts hidden in the notification area."
