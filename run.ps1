$ErrorActionPreference = "Stop"

$root = Split-Path -Parent $MyInvocation.MyCommand.Path
$exe = Join-Path $root "src-tauri\target\debug\osu_beatmap_seekman.exe"
$cargoBin = Join-Path $env:USERPROFILE ".cargo\bin"

Set-Location $root

if (Test-Path $cargoBin) {
  $env:PATH = "$cargoBin;$env:PATH"
}

if (Test-Path $exe) {
  $exeTime = (Get-Item $exe).LastWriteTimeUtc
  $sourceTime = Get-ChildItem -Path @(
    (Join-Path $root "src"),
    (Join-Path $root "src-tauri\src")
  ) -Recurse -File |
    Measure-Object -Property LastWriteTimeUtc -Maximum |
    Select-Object -ExpandProperty Maximum

  $configTime = Get-Item @(
    (Join-Path $root "package.json"),
    (Join-Path $root "src-tauri\Cargo.toml"),
    (Join-Path $root "src-tauri\tauri.conf.json")
  ) | Measure-Object -Property LastWriteTimeUtc -Maximum | Select-Object -ExpandProperty Maximum

  if ($sourceTime -gt $exeTime -or $configTime -gt $exeTime) {
    npx tauri build --debug
  }

  Start-Process -FilePath $exe -WorkingDirectory $root
  exit 0
}

npm run dev
