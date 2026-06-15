param(
  [string]$Remote = "origin",
  [string]$Branch = "master",
  [string]$Message = ""
)

$ErrorActionPreference = "Stop"

function Fail($Text) {
  Write-Host "ERROR: $Text" -ForegroundColor Red
  exit 1
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

git rev-parse --is-inside-work-tree *> $null
if ($LASTEXITCODE -ne 0) {
  Fail "Run this script inside the repository."
}

$currentBranch = (git branch --show-current).Trim()
if ($currentBranch -ne $Branch) {
  Fail "Current branch is '$currentBranch', expected '$Branch'."
}

$packageJson = Read-JsonFile "package.json"
$currentVersion = [string]$packageJson.version
$nextVersion = Bump-Patch $currentVersion
$tag = "v$nextVersion"

if ((git tag --list $tag).Trim()) {
  Fail "Tag already exists locally: $tag"
}
if ((git ls-remote --tags $Remote $tag).Trim()) {
  Fail "Tag already exists on remote: $tag"
}

Write-Host "Bumping version: $currentVersion -> $nextVersion" -ForegroundColor Cyan

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

git add -A
git commit -m $Message
git tag -a $tag -m $Message
git push $Remote $Branch
git push $Remote $tag

Write-Host ""
Write-Host "Pushed $tag. GitHub Actions release workflows should start from the tag push." -ForegroundColor Green
