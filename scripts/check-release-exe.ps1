param(
  [string]$ExePath = "src-tauri\target\release\codexhub.exe",
  [int]$Seconds = 5
)

$ErrorActionPreference = "Stop"

$Root = Resolve-Path (Join-Path $PSScriptRoot "..")
$ResolvedExe = Resolve-Path (Join-Path $Root $ExePath)
$TempRoot = Join-Path ([System.IO.Path]::GetTempPath()) ("codexhub-release-check-" + [System.Guid]::NewGuid().ToString("N"))
$TempProfile = Join-Path $TempRoot "UserProfile"
$TempAppData = Join-Path $TempProfile "AppData\Roaming"
$TempLocalAppData = Join-Path $TempProfile "AppData\Local"
$TempHomeDrive = Split-Path -Qualifier $TempProfile
$TempHomePath = $TempProfile.Substring($TempHomeDrive.Length)

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
  $startInfo.Environment["HOMEDRIVE"] = $TempHomeDrive
  $startInfo.Environment["HOMEPATH"] = $TempHomePath

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
    # Windows PowerShell 5.1 lacks Process.Kill(true), so taskkill closes the WebView tree too.
    & taskkill.exe /PID $process.Id /T /F | Out-Null
    if ($LASTEXITCODE -ne 0 -and -not $process.HasExited) {
      $process.Kill()
    }
    $process.WaitForExit(5000) | Out-Null
  }
  if (Test-Path -LiteralPath $TempRoot) {
    for ($attempt = 1; $attempt -le 10; $attempt++) {
      try {
        Remove-Item -LiteralPath $TempRoot -Recurse -Force
        break
      } catch {
        if ($attempt -eq 10) {
          throw
        }
        Start-Sleep -Milliseconds 250
      }
    }
  }
}
