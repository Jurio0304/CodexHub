param(
  [ValidateSet("stable", "dev")]
  [string]$Channel = "stable",
  [switch]$SkipBuild
)

$ErrorActionPreference = "Stop"

$Root = Resolve-Path (Join-Path $PSScriptRoot "..")
$PackageJson = Get-Content -LiteralPath (Join-Path $Root "package.json") -Raw | ConvertFrom-Json
$Version = $PackageJson.version
$ChannelConfig = @{
  stable = @{
    BuildScript = "build:tauri"
    StageName = "CodexHub-v$Version-windows-x64-portable"
    ExeName = "CodexHub.exe"
    ProductName = "CodexHub"
  }
  dev = @{
    BuildScript = "build:tauri:dev"
    StageName = "CodexHub-Dev-v$Version-windows-x64-portable"
    ExeName = "CodexHub Dev.exe"
    ProductName = "CodexHub Dev"
  }
}
$SelectedChannel = $ChannelConfig[$Channel]
$StageName = $SelectedChannel.StageName
$ReleaseRoot = Join-Path $Root "release-artifacts"
$StageDir = Join-Path $ReleaseRoot $StageName
$ZipPath = Join-Path $ReleaseRoot "$StageName.zip"
$ChecksumPath = Join-Path $ReleaseRoot "SHA256SUMS.txt"
$ExePath = Join-Path $Root "src-tauri\target\release\codexhub.exe"

function Assert-ChildPath {
  param(
    [Parameter(Mandatory = $true)][string]$Parent,
    [Parameter(Mandatory = $true)][string]$Child
  )
  $parentPath = [System.IO.Path]::GetFullPath($Parent).TrimEnd('\') + '\'
  $childPath = [System.IO.Path]::GetFullPath($Child)
  if (-not $childPath.StartsWith($parentPath, [System.StringComparison]::OrdinalIgnoreCase)) {
    throw "Refusing to operate outside $parentPath`: $childPath"
  }
}

function Get-RelativeReleasePath {
  param(
    [Parameter(Mandatory = $true)][string]$Base,
    [Parameter(Mandatory = $true)][string]$Path
  )
  $basePath = [System.IO.Path]::GetFullPath($Base).TrimEnd('\') + '\'
  $targetPath = [System.IO.Path]::GetFullPath($Path)
  $baseUri = [System.Uri]::new($basePath)
  $targetUri = [System.Uri]::new($targetPath)
  [System.Uri]::UnescapeDataString($baseUri.MakeRelativeUri($targetUri).ToString())
}

if (-not $SkipBuild) {
  Push-Location $Root
  try {
    pnpm $($SelectedChannel.BuildScript)
  } finally {
    Pop-Location
  }
}

if (-not (Test-Path -LiteralPath $ExePath)) {
  throw "Release executable not found: $ExePath"
}

New-Item -ItemType Directory -Force -Path $ReleaseRoot | Out-Null
Assert-ChildPath -Parent $ReleaseRoot -Child $StageDir
Assert-ChildPath -Parent $ReleaseRoot -Child $ZipPath
Assert-ChildPath -Parent $ReleaseRoot -Child $ChecksumPath

if (Test-Path -LiteralPath $StageDir) {
  Remove-Item -LiteralPath $StageDir -Recurse -Force
}
if (Test-Path -LiteralPath $ZipPath) {
  Remove-Item -LiteralPath $ZipPath -Force
}

New-Item -ItemType Directory -Force -Path $StageDir | Out-Null
Copy-Item -LiteralPath $ExePath -Destination (Join-Path $StageDir $($SelectedChannel.ExeName))

foreach ($relativePath in @(
  "README.md",
  "LICENSE",
  "SECURITY.md",
  "docs\known-limitations.md",
  "docs\zh-CN\README.md"
)) {
  $source = Join-Path $Root $relativePath
  if (Test-Path -LiteralPath $source) {
    $destination = Join-Path $StageDir $relativePath
    New-Item -ItemType Directory -Force -Path (Split-Path -Parent $destination) | Out-Null
    Copy-Item -LiteralPath $source -Destination $destination
  }
}

$portableReadme = @"
CodexHub portable build
Version: $Version
Channel: $Channel
Product: $($SelectedChannel.ProductName)

Run $($SelectedChannel.ExeName) to start the desktop app.

This archive intentionally does not include local app state, SSH config, hosts,
profiles, task logs, private keys, tokens, or generated installer files.

See README.md and docs\known-limitations.md for setup and current boundaries.
"@
Set-Content -LiteralPath (Join-Path $StageDir "PORTABLE_README.txt") -Value $portableReadme -Encoding UTF8

Push-Location $Root
try {
  pnpm audit:public
} finally {
  Pop-Location
}

Compress-Archive -LiteralPath $StageDir -DestinationPath $ZipPath -Force

$hashes = @(
  Get-FileHash -Algorithm SHA256 -LiteralPath (Join-Path $StageDir $($SelectedChannel.ExeName))
  Get-FileHash -Algorithm SHA256 -LiteralPath $ZipPath
)
$hashLines = $hashes | ForEach-Object {
  $relative = (Get-RelativeReleasePath -Base $ReleaseRoot -Path $_.Path).Replace('\', '/')
  "$($_.Hash.ToLowerInvariant())  $relative"
}
Set-Content -LiteralPath $ChecksumPath -Value $hashLines -Encoding ASCII

Write-Host "Portable package: $ZipPath"
Write-Host "Checksums: $ChecksumPath"
