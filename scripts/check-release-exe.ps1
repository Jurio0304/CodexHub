param(
  [string]$ExePath = "src-tauri\target\release\codexhub.exe",
  [int]$Seconds = 5
)

$ErrorActionPreference = "Stop"

$Root = Resolve-Path (Join-Path $PSScriptRoot "..")
$ResolvedExe = Resolve-Path (Join-Path $Root $ExePath)
$TempRoot = Join-Path ([System.IO.Path]::GetTempPath()) ("codexhub-release-check-" + [System.Guid]::NewGuid().ToString("N"))
$TempAppData = Join-Path $TempRoot "AppData\Roaming"
$TempLocalAppData = Join-Path $TempRoot "AppData\Local"
$TempProfile = Join-Path $TempRoot "UserProfile"

New-Item -ItemType Directory -Force -Path $TempAppData, $TempLocalAppData, $TempProfile | Out-Null

$process = $null
try {
  $startInfo = [System.Diagnostics.ProcessStartInfo]::new()
  $startInfo.FileName = $ResolvedExe.Path
  $startInfo.WorkingDirectory = Split-Path -Parent $ResolvedExe.Path
  $startInfo.UseShellExecute = $false
  $startInfo.WindowStyle = [System.Diagnostics.ProcessWindowStyle]::Hidden
  $startInfo.Environment["APPDATA"] = $TempAppData
  $startInfo.Environment["LOCALAPPDATA"] = $TempLocalAppData
  $startInfo.Environment["USERPROFILE"] = $TempProfile
  $startInfo.Environment["HOME"] = $TempProfile

  $process = [System.Diagnostics.Process]::Start($startInfo)
  if ($null -eq $process) {
    throw "Failed to start $($ResolvedExe.Path)"
  }

  Start-Sleep -Seconds $Seconds
  if ($process.HasExited) {
    throw "CodexHub exited during startup check with code $($process.ExitCode)."
  }

  Write-Host "Release exe startup check passed for PID $($process.Id)."
} finally {
  if ($null -ne $process -and -not $process.HasExited) {
    $process.Kill()
    $process.WaitForExit(5000) | Out-Null
  }
  if (Test-Path -LiteralPath $TempRoot) {
    Remove-Item -LiteralPath $TempRoot -Recurse -Force
  }
}
