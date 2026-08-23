$ErrorActionPreference = "Stop"
$repoRoot = Resolve-Path (Join-Path $PSScriptRoot "..")
$coreManifest = Join-Path $repoRoot "WindowsCoreHost\Cargo.toml"
$appProject = Join-Path $repoRoot "WindowsWinUI\CodexQuotaViewer.WinUI.csproj"
$testProject = Join-Path $repoRoot "WindowsWinUI.Tests\CodexQuotaViewer.WinUI.Tests.csproj"

cargo test --manifest-path $coreManifest
if ($LASTEXITCODE -ne 0) { throw "CoreHost tests failed." }

dotnet test $testProject -c Release
if ($LASTEXITCODE -ne 0) { throw "Window geometry tests failed." }

dotnet build $appProject -c Release -p:Platform=x64
if ($LASTEXITCODE -ne 0) { throw "Native WinUI build failed." }

$output = Join-Path $repoRoot "WindowsWinUI\bin\x64\Release\net8.0-windows10.0.19041.0\win-x64"
$coreExe = Join-Path $output "codex-quota-viewer-core-host.exe"
$smokeRoot = Join-Path ([System.IO.Path]::GetTempPath()) ("CodexQuotaViewer-CoreHost-Smoke-" + [guid]::NewGuid().ToString("N"))
New-Item -ItemType Directory -Force $smokeRoot | Out-Null
try {
  $env:CODEX_QUOTA_VIEWER_CODEX_HOME = Join-Path $smokeRoot ".codex"
  $env:CODEX_QUOTA_VIEWER_APP_DATA = Join-Path $smokeRoot "app-data"
  $requests = @(
    '{"id":1,"method":"ping","params":{}}',
    '{"id":2,"method":"getSettings","params":{}}',
    '{"id":3,"method":"shutdown","params":{}}'
  )
  $responses = $requests | & $coreExe --resource-root $output
  if ($LASTEXITCODE -ne 0) { throw "CoreHost protocol smoke test failed." }
  $parsed = $responses | ForEach-Object { $_ | ConvertFrom-Json }
  if ($parsed.Count -ne 3 -or !$parsed[0].ok -or !$parsed[1].ok -or !$parsed[2].ok) {
    throw "CoreHost protocol returned an unexpected response."
  }
}
finally {
  Remove-Item Env:CODEX_QUOTA_VIEWER_CODEX_HOME -ErrorAction SilentlyContinue
  Remove-Item Env:CODEX_QUOTA_VIEWER_APP_DATA -ErrorAction SilentlyContinue
  $resolvedSmokeRoot = [System.IO.Path]::GetFullPath($smokeRoot)
  $resolvedTempRoot = [System.IO.Path]::GetFullPath([System.IO.Path]::GetTempPath())
  if ($resolvedSmokeRoot.StartsWith($resolvedTempRoot, [System.StringComparison]::OrdinalIgnoreCase)) {
    Remove-Item -LiteralPath $resolvedSmokeRoot -Recurse -Force
  }
}

Write-Host "Windows WinUI verification passed."
