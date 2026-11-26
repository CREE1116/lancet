# Lancet installer for Windows x86_64 (downloads verified prebuilt binary; no Rust needed).
# Usage:
#   irm https://raw.githubusercontent.com/CREE1116/lancet/main/install.ps1 | iex
$ErrorActionPreference = "Stop"

$Repo = "CREE1116/lancet"
$Target = "x86_64-pc-windows-msvc"
$InstallDir = if ($env:LANCET_INSTALL_DIR) { $env:LANCET_INSTALL_DIR } else { Join-Path $env:LOCALAPPDATA "lancet\bin" }

if ($env:LANCET_VERSION) {
  $Tag = $env:LANCET_VERSION
} else {
  $Rel = Invoke-RestMethod -Uri "https://api.github.com/repos/$Repo/releases/latest"
  $Tag = $Rel.tag_name
}
if (-not $Tag) { throw "Could not determine latest lancet release tag." }

$Archive = "lancet-$Tag-$Target.zip"
$BaseUrl = "https://github.com/$Repo/releases/download/$Tag"
$Tmp = Join-Path ([System.IO.Path]::GetTempPath()) ("lancet-" + [System.Guid]::NewGuid().ToString("N"))
New-Item -ItemType Directory -Force -Path $Tmp | Out-Null

try {
  Write-Host "Downloading $Archive ($Tag)..."
  $ZipPath = Join-Path $Tmp $Archive
  $SumPath = Join-Path $Tmp "SHA256SUMS.txt"
  Invoke-WebRequest -Uri "$BaseUrl/$Archive" -OutFile $ZipPath
  Invoke-WebRequest -Uri "$BaseUrl/SHA256SUMS.txt" -OutFile $SumPath

  $ExpectedLine = Get-Content $SumPath | Where-Object { $_ -match "\s$([regex]::Escape($Archive))$" } | Select-Object -First 1
  if (-not $ExpectedLine) { throw "Checksum entry for $Archive not found in SHA256SUMS.txt." }
  $Expected = ($ExpectedLine -split "\s+")[0].ToLowerInvariant()
  $Actual = (Get-FileHash -Path $ZipPath -Algorithm SHA256).Hash.ToLowerInvariant()
  if ($Expected -ne $Actual) { throw "SHA256 mismatch for $Archive (expected $Expected, got $Actual)." }

  Expand-Archive -Path $ZipPath -DestinationPath (Join-Path $Tmp "unpacked") -Force
  New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null
  Copy-Item (Join-Path $Tmp "unpacked\lancet.exe") (Join-Path $InstallDir "lancet.exe") -Force

  Write-Host "Installed lancet $Tag to $InstallDir\lancet.exe"
  if (($env:PATH -split ";") -notcontains $InstallDir) {
    Write-Host "Add $InstallDir to your User PATH to run 'lancet' from any terminal."
  }
} finally {
  Remove-Item -Recurse -Force $Tmp -ErrorAction SilentlyContinue
}
