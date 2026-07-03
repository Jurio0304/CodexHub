param(
  [ValidateSet("dev", "stable")]
  [string]$Channel = "dev",
  [switch]$SkipTauriBuild,
  [switch]$SkipPortable,
  [switch]$NoLive,
  [string]$LiveSshAlias = "",
  [switch]$UserTested,
  [switch]$OpenApp,
  [int]$StartupSeconds = 5
)

$ErrorActionPreference = "Stop"

$Root = Resolve-Path (Join-Path $PSScriptRoot "..")
$Results = New-Object System.Collections.Generic.List[object]
$Artifacts = New-Object System.Collections.Generic.List[string]
$ManualItems = New-Object System.Collections.Generic.List[string]
$HasFailure = $false
$Node = $null
$Pnpm = $null
$PowerShellExe = $null
$StableGatePassed = $false
$StableBuildReady = $false

function Add-Result {
  param(
    [Parameter(Mandatory = $true)][string]$Name,
    [Parameter(Mandatory = $true)][ValidateSet("PASS", "FAIL", "SKIP")][string]$Status,
    [string]$Detail = ""
  )

  $Results.Add([pscustomobject]@{
    Name = $Name
    Status = $Status
    Detail = $Detail
  }) | Out-Null

  $prefix = "[$Status]"
  if ($Detail) {
    Write-Host "$prefix $Name - $Detail"
  } else {
    Write-Host "$prefix $Name"
  }
}

function Invoke-Step {
  param(
    [Parameter(Mandatory = $true)][string]$Name,
    [Parameter(Mandatory = $true)][scriptblock]$ScriptBlock
  )

  try {
    & $ScriptBlock
    Add-Result -Name $Name -Status "PASS"
  } catch {
    $script:HasFailure = $true
    Add-Result -Name $Name -Status "FAIL" -Detail $_.Exception.Message
  }
}

function Skip-Step {
  param(
    [Parameter(Mandatory = $true)][string]$Name,
    [Parameter(Mandatory = $true)][string]$Reason
  )
  Add-Result -Name $Name -Status "SKIP" -Detail $Reason
}

function Add-CodexBundledRuntimeToPath {
  $candidatePaths = @(
    (Join-Path $env:USERPROFILE ".cache\codex-runtimes\codex-primary-runtime\dependencies\node\bin"),
    (Join-Path $env:USERPROFILE ".cache\codex-runtimes\codex-primary-runtime\bin")
  )

  foreach ($candidate in $candidatePaths) {
    if ((Test-Path -LiteralPath $candidate) -and -not (($env:PATH -split ";") -contains $candidate)) {
      $env:PATH = "$candidate;$env:PATH"
    }
  }
}

function Resolve-Tool {
  param(
    [Parameter(Mandatory = $true)][string[]]$Names,
    [Parameter(Mandatory = $true)][string]$InstallHint
  )

  foreach ($name in $Names) {
    $command = Get-Command $name -ErrorAction SilentlyContinue | Select-Object -First 1
    if ($null -ne $command) {
      return $command.Source
    }
  }

  throw $InstallHint
}

function Invoke-ProcessChecked {
  param(
    [Parameter(Mandatory = $true)][string]$FilePath,
    [Parameter(Mandatory = $true)][string[]]$Arguments
  )

  Push-Location $Root
  try {
    & $FilePath @Arguments
    if ($LASTEXITCODE -ne 0) {
      throw "$FilePath $($Arguments -join ' ') exited with code $LASTEXITCODE"
    }
  } finally {
    Pop-Location
  }
}

function Invoke-Pnpm {
  param([Parameter(Mandatory = $true)][string[]]$Arguments)
  Invoke-ProcessChecked -FilePath $script:Pnpm -Arguments $Arguments
}

function Add-ArtifactIfExists {
  param([Parameter(Mandatory = $true)][string]$Path)
  $resolved = Join-Path $Root $Path
  if (Test-Path -LiteralPath $resolved) {
    $Artifacts.Add($resolved) | Out-Null
  }
}

function Read-JsonFile {
  param([Parameter(Mandatory = $true)][string]$Path)
  Get-Content -LiteralPath (Join-Path $Root $Path) -Raw | ConvertFrom-Json
}

Write-Host "CodexHub release validation"
Write-Host "Channel: $Channel"
Write-Host "Root: $Root"
Write-Host ""

Invoke-Step -Name "Node and pnpm toolchain" -ScriptBlock {
  Add-CodexBundledRuntimeToPath
  $script:Node = Resolve-Tool -Names @("node.exe", "node.cmd", "node") -InstallHint "Node.js was not found on PATH or in the bundled Codex runtime. Install Node.js 20+ and pnpm, or ensure the Codex bundled runtime exists under %USERPROFILE%\.cache\codex-runtimes\codex-primary-runtime."
  $script:Pnpm = Resolve-Tool -Names @("pnpm.cmd", "pnpm.exe", "pnpm") -InstallHint "pnpm was not found on PATH or in the bundled Codex runtime. Install pnpm, or ensure the Codex bundled runtime bin directory is available."
  $script:PowerShellExe = Resolve-Tool -Names @("powershell.exe", "powershell") -InstallHint "Windows PowerShell was not found on PATH."

  & $script:Node --version
  & $script:Pnpm --version
}

$ToolsReady = ($null -ne $Node -and $null -ne $Pnpm)

Invoke-Step -Name "Channel contract" -ScriptBlock {
  $packageJson = Read-JsonFile "package.json"
  $stableConfig = Read-JsonFile "src-tauri\tauri.conf.json"
  $devConfig = Read-JsonFile "src-tauri\tauri.dev.conf.json"

  if ($stableConfig.productName -ne "CodexHub") {
    throw "Stable productName must be CodexHub."
  }
  if ($stableConfig.identifier -ne "app.codexhub.desktop") {
    throw "Stable identifier must be app.codexhub.desktop."
  }
  if ($devConfig.productName -ne "CodexHub Dev") {
    throw "Dev productName must be CodexHub Dev."
  }
  if ($devConfig.identifier -ne "dev.codexhub.desktop") {
    throw "Dev identifier must be dev.codexhub.desktop."
  }
  if ($stableConfig.identifier -eq $devConfig.identifier) {
    throw "Stable and dev identifiers must be different."
  }
  if ($packageJson.version -ne $stableConfig.version -or $packageJson.version -ne $devConfig.version) {
    throw "package.json and Tauri channel versions must match."
  }
}

if ($Channel -eq "stable") {
  Invoke-Step -Name "Stable release gates" -ScriptBlock {
    if ($SkipTauriBuild) {
      throw "Stable validation cannot use -SkipTauriBuild."
    }
    if (-not $UserTested) {
      throw "Stable validation requires -UserTested after the owner completes full manual acceptance."
    }
    $script:StableGatePassed = $true
  }
} else {
  Skip-Step -Name "Stable release gates" -Reason "dev channel is not publishable and does not create release artifacts."
}

if ($ToolsReady) {
  Invoke-Step -Name "pnpm smoke" -ScriptBlock { Invoke-Pnpm -Arguments @("smoke") }
  Invoke-Step -Name "pnpm typecheck" -ScriptBlock { Invoke-Pnpm -Arguments @("typecheck") }
  Invoke-Step -Name "Public leak audit" -ScriptBlock { Invoke-Pnpm -Arguments @("audit:public") }
} else {
  Skip-Step -Name "pnpm smoke" -Reason "node/pnpm unavailable."
  Skip-Step -Name "pnpm typecheck" -Reason "node/pnpm unavailable."
  Skip-Step -Name "Public leak audit" -Reason "node/pnpm unavailable."
}

if ($Channel -eq "stable" -and $ToolsReady -and $StableGatePassed) {
  Invoke-Step -Name "pnpm build:web" -ScriptBlock { Invoke-Pnpm -Arguments @("build:web") }

  $cargo = Get-Command "cargo" -ErrorAction SilentlyContinue | Select-Object -First 1
  if ($null -ne $cargo) {
    Invoke-Step -Name "cargo test" -ScriptBlock {
      Invoke-ProcessChecked -FilePath $cargo.Source -Arguments @("test", "--manifest-path", "src-tauri\Cargo.toml")
    }
  } else {
    $HasFailure = $true
    Add-Result -Name "cargo test" -Status "FAIL" -Detail "cargo was not found. Install Rust stable MSVC before stable validation."
  }

  Invoke-Step -Name "Tauri stable release build" -ScriptBlock {
    Invoke-Pnpm -Arguments @("build:tauri")
    $script:StableBuildReady = $true
    Add-ArtifactIfExists "src-tauri\target\release\codexhub.exe"
  }

  if ($StableBuildReady -and -not $SkipPortable) {
    Invoke-Step -Name "Portable stable package" -ScriptBlock {
      Invoke-ProcessChecked -FilePath $script:PowerShellExe -Arguments @("-NoProfile", "-ExecutionPolicy", "Bypass", "-File", ".\scripts\package-portable.ps1", "-Channel", "stable", "-SkipBuild")
      $packageJson = Read-JsonFile "package.json"
      Add-ArtifactIfExists "release-artifacts\CodexHub-v$($packageJson.version)-windows-x64-portable.zip"
      Add-ArtifactIfExists "release-artifacts\SHA256SUMS.txt"
    }

    Invoke-Step -Name "Release exe startup check" -ScriptBlock {
      Invoke-ProcessChecked -FilePath $script:PowerShellExe -Arguments @("-NoProfile", "-ExecutionPolicy", "Bypass", "-File", ".\scripts\check-release-exe.ps1", "-Seconds", "$StartupSeconds")
    }
  } elseif ($StableBuildReady) {
    Skip-Step -Name "Portable stable package" -Reason "approved Windows public release path is the updater-enabled setup installer."

    Invoke-Step -Name "Release exe startup check" -ScriptBlock {
      Invoke-ProcessChecked -FilePath $script:PowerShellExe -Arguments @("-NoProfile", "-ExecutionPolicy", "Bypass", "-File", ".\scripts\check-release-exe.ps1", "-Seconds", "$StartupSeconds")
    }
  } else {
    Skip-Step -Name "Portable stable package" -Reason "stable Tauri build did not produce a fresh release executable."
    Skip-Step -Name "Release exe startup check" -Reason "stable Tauri build did not produce a fresh release executable."
  }
} elseif ($Channel -eq "stable") {
  if (-not $ToolsReady) {
    Skip-Step -Name "Stable build and package" -Reason "node/pnpm unavailable."
  } else {
    Skip-Step -Name "Stable build and package" -Reason "stable release gates failed."
  }
} else {
  Skip-Step -Name "pnpm build:web" -Reason "dev validation is source acceptance; no release or web artifact is required."
  Skip-Step -Name "cargo test" -Reason "dev validation keeps Rust release checks for stable."
  Skip-Step -Name "Tauri stable release build" -Reason "dev channel must not build stable release artifacts."
  Skip-Step -Name "Portable stable package" -Reason "dev channel must not package public release artifacts."
  Skip-Step -Name "Release exe startup check" -Reason "no release exe is produced for dev validation."
}

if ($Channel -eq "dev" -and $OpenApp) {
  if ($ToolsReady) {
    Invoke-Step -Name "Open dev source app" -ScriptBlock {
      Start-Process -FilePath $script:Pnpm -ArgumentList @("dev") -WorkingDirectory $Root
    }
  } else {
    Skip-Step -Name "Open dev source app" -Reason "node/pnpm unavailable."
  }
} elseif ($Channel -eq "dev") {
  Skip-Step -Name "Open dev source app" -Reason "run with -OpenApp, or run pnpm dev manually, when the owner is ready to test."
}

if ($NoLive) {
  Skip-Step -Name "Live SSH acceptance" -Reason "-NoLive was provided."
} elseif ($LiveSshAlias.Trim().Length -gt 0) {
  $ssh = Get-Command "ssh" -ErrorAction SilentlyContinue | Select-Object -First 1
  if ($null -ne $ssh) {
    Invoke-Step -Name "Live SSH acceptance" -ScriptBlock {
      Invoke-ProcessChecked -FilePath $ssh.Source -Arguments @($LiveSshAlias, "echo", "ok")
    }
  } else {
    $HasFailure = $true
    Add-Result -Name "Live SSH acceptance" -Status "FAIL" -Detail "ssh was not found on PATH."
  }
} else {
  Skip-Step -Name "Live SSH acceptance" -Reason "provide -LiveSshAlias <alias> to run a real SSH check."
}

Invoke-Step -Name "git diff --check" -ScriptBlock {
  $git = Resolve-Tool -Names @("git.exe", "git") -InstallHint "git was not found on PATH."
  Invoke-ProcessChecked -FilePath $git -Arguments @("diff", "--check")
}

if ($Channel -eq "dev") {
  $ManualItems.Add("Open the dev source app with: pnpm dev, or rerun this script with -OpenApp.") | Out-Null
  $ManualItems.Add("Owner acceptance: first-run guide, local SSH status, add-server modal, host details, profile apply preview, skill import/install preview, task logs, and settings persistence.") | Out-Null
  $ManualItems.Add("Do not publish dev artifacts, tag dev builds, or copy dev-only notes into README/user docs.") | Out-Null
} else {
  $ManualItems.Add("Owner full manual acceptance must be completed before using -UserTested.") | Out-Null
  if ($SkipPortable) {
    $ManualItems.Add("Inspect the updater-enabled installer, updater archive when applicable, latest.json, SHA256SUMS.txt, and app startup behavior before publishing.") | Out-Null
  } else {
    $ManualItems.Add("Inspect the portable zip, SHA256SUMS.txt, and app startup behavior before publishing.") | Out-Null
  }
  $ManualItems.Add("Run live SSH acceptance only with an explicit sanitized test alias: -LiveSshAlias <alias>.") | Out-Null
  $ManualItems.Add("This script does not push, tag, upload, or create a GitHub Release.") | Out-Null
}

$passed = @($Results | Where-Object { $_.Status -eq "PASS" }).Count
$failed = @($Results | Where-Object { $_.Status -eq "FAIL" }).Count
$skipped = @($Results | Where-Object { $_.Status -eq "SKIP" }).Count

Write-Host ""
Write-Host "Summary"
Write-Host "Passed: $passed"
Write-Host "Failed: $failed"
Write-Host "Skipped: $skipped"

Write-Host "Artifacts:"
if ($Artifacts.Count -eq 0) {
  Write-Host "- none"
} else {
  foreach ($artifact in $Artifacts) {
    Write-Host "- $artifact"
  }
}

Write-Host "Manual test items:"
foreach ($item in $ManualItems) {
  Write-Host "- $item"
}

if ($HasFailure) {
  exit 1
}
