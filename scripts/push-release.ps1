param(
  [string]$Remote = "origin",
  [string]$Branch = "",
  [string]$Message = "",
  [switch]$NoPull,
  [switch]$DryRun
)

$ErrorActionPreference = "Stop"

function Fail($Text) {
  Write-Host "ERROR: $Text" -ForegroundColor Red
  exit 1
}

function Run-Git([string[]]$GitArgs) {
  Write-Host "git $($GitArgs -join ' ')" -ForegroundColor DarkGray
  & git @GitArgs
  if ($LASTEXITCODE -ne 0) {
    Fail "git $($GitArgs -join ' ') failed."
  }
}

function Read-JsonFile($Path) {
  if (!(Test-Path -LiteralPath $Path)) {
    Fail "Missing file: $Path"
  }
  Get-Content -LiteralPath $Path -Raw -Encoding UTF8 | ConvertFrom-Json
}

function Write-Utf8NoBom($Path, $Text) {
  $Utf8NoBom = New-Object System.Text.UTF8Encoding($false)
  [System.IO.File]::WriteAllText((Resolve-Path -LiteralPath $Path), $Text, $Utf8NoBom)
}

function Bump-Patch($Version) {
  if ($Version -notmatch '^(\d+)\.(\d+)\.(\d+)$') {
    Fail "Version must be x.y.z, got: $Version"
  }
  $major = [int]$Matches[1]
  $minor = [int]$Matches[2]
  $patch = [int]$Matches[3] + 1
  "$major.$minor.$patch"
}

function Update-JsonVersion($Path, $Version) {
  $raw = Get-Content -LiteralPath $Path -Raw -Encoding UTF8
  $updated = $raw -replace '("version"\s*:\s*")[^"]+(")', "`${1}$Version`${2}"
  Write-Utf8NoBom $Path $updated
}

function Update-PackageLockVersion($Path, $Version) {
  $raw = Get-Content -LiteralPath $Path -Raw -Encoding UTF8
  $updated = [System.Text.RegularExpressions.Regex]::Replace(
    $raw,
    '("name"\s*:\s*"osu-beatmap-seekman"\s*,\s*"version"\s*:\s*")[^"]+(")',
    "`${1}$Version`${2}",
    2
  )
  Write-Utf8NoBom $Path $updated
}

function Update-CargoVersion($Path, $Version) {
  $raw = Get-Content -LiteralPath $Path -Raw -Encoding UTF8
  $updated = $raw -replace '(?m)^(version\s*=\s*")[^"]+(")', "`${1}$Version`${2}"
  Write-Utf8NoBom $Path $updated
}

function Update-CargoLockVersion($Path, $Version) {
  $raw = Get-Content -LiteralPath $Path -Raw -Encoding UTF8
  $updated = [System.Text.RegularExpressions.Regex]::Replace(
    $raw,
    '(name\s*=\s*"osu_beatmap_seekman"\s*\r?\nversion\s*=\s*")[^"]+(")',
    "`${1}$Version`${2}",
    1
  )
  Write-Utf8NoBom $Path $updated
}

$startDir = if ($PSScriptRoot) { $PSScriptRoot } else { (Get-Location).Path }
$repoRoot = (& git -C $startDir rev-parse --show-toplevel 2>$null).Trim()
if (!$repoRoot) {
  $repoRoot = (& git rev-parse --show-toplevel 2>$null).Trim()
}
if (!$repoRoot) {
  Fail "Run this script inside the repository, or from the scripts folder."
}
Set-Location -LiteralPath $repoRoot

$currentBranch = (git branch --show-current).Trim()
if (!$currentBranch) {
  Fail "Could not determine current branch."
}
if ([string]::IsNullOrWhiteSpace($Branch)) {
  $Branch = $currentBranch
}
if ($currentBranch -ne $Branch) {
  Fail "Current branch is '$currentBranch', expected '$Branch'. Use -Branch $currentBranch or switch branches."
}

Run-Git @("fetch", $Remote, $Branch, "--tags")
if (!$NoPull) {
  Run-Git @("pull", "--rebase", "--autostash", $Remote, $Branch)
}

$packageJson = Read-JsonFile "package.json"
$currentVersion = [string]$packageJson.version
$nextVersion = Bump-Patch $currentVersion
$tag = "v$nextVersion"

$localTag = (git tag --list $tag) -join "`n"
if ($localTag.Trim()) {
  Fail "Tag already exists locally: $tag"
}
$remoteTag = (git ls-remote --tags $Remote $tag) -join "`n"
if ($remoteTag.Trim()) {
  Fail "Tag already exists on remote: $tag"
}

Write-Host "Bumping version: $currentVersion -> $nextVersion" -ForegroundColor Cyan

if ($DryRun) {
  Write-Host ""
  Write-Host "Dry run complete. No files were changed, no commit was created, and nothing was pushed." -ForegroundColor Yellow
  Write-Host "Next tag would be: $tag"
  exit 0
}

Update-JsonVersion "package.json" $nextVersion
if (Test-Path -LiteralPath "package-lock.json") {
  Update-PackageLockVersion "package-lock.json" $nextVersion
}
Update-JsonVersion "src-tauri/tauri.conf.json" $nextVersion
Update-CargoVersion "src-tauri/Cargo.toml" $nextVersion
if (Test-Path -LiteralPath "src-tauri/Cargo.lock") {
  Update-CargoLockVersion "src-tauri/Cargo.lock" $nextVersion
}

$status = (git status --porcelain)
if (!$status) {
  Fail "No changes to commit after version bump."
}

if ([string]::IsNullOrWhiteSpace($Message)) {
  $Message = "Release $tag"
}

Run-Git @("add", "-A")
Run-Git @("commit", "-m", $Message)
Run-Git @("tag", "-a", $tag, "-m", $Message)
Run-Git @("push", $Remote, $Branch)
Run-Git @("push", $Remote, $tag)

Write-Host ""
Write-Host "Pushed $tag. GitHub Actions release workflows should start from the tag push." -ForegroundColor Green
