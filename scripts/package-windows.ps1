# Builds what Windows users download, in dist\: an installer (Tidy-<version>-windows-<arch>-setup.exe,
# made with Inno Setup when `iscc` is installed), a portable .zip with Tidy.exe, and SHA-256 sums.
# Usage: scripts\package-windows.ps1 [-Arch x64|arm64]   (default: this machine's)
#        scripts\package-windows.ps1 -Arch x64 -Exe signed\tidy.exe   (package an exe built and signed before)
param(
  [string]$Arch = $(if ($env:PROCESSOR_ARCHITECTURE -eq 'ARM64') { 'arm64' } else { 'x64' }),
  [string]$Exe = ''
)
$ErrorActionPreference = 'Stop'
Set-Location (Join-Path $PSScriptRoot '..')

$target = if ($Arch -eq 'arm64') { 'aarch64-pc-windows-msvc' } else { 'x86_64-pc-windows-msvc' }
$version = (Select-String -Path Cargo.toml -Pattern '^version = "(.+)"' | Select-Object -First 1).Matches[0].Groups[1].Value
if ($Exe) {
  $exe = $Exe
} else {
  rustup target add $target | Out-Null
  cargo build --release -p tidy --target $target
  if ($LASTEXITCODE -ne 0) { throw "cargo build failed" }
  $exe = "target\$target\release\tidy.exe"
}

New-Item -ItemType Directory -Force dist | Out-Null
$name = "Tidy-$version-windows-$Arch"

# Portable: just the app.
$stage = Join-Path $env:TEMP $name
Remove-Item -Recurse -Force $stage -ErrorAction SilentlyContinue
New-Item -ItemType Directory $stage | Out-Null
Copy-Item $exe "$stage\Tidy.exe"
Copy-Item LICENSE "$stage\LICENSE.txt"
Compress-Archive -Path "$stage\*" -DestinationPath "dist\$name-portable.zip" -Force

# The installer, when Inno Setup is here.
$iscc = (Get-Command iscc -ErrorAction SilentlyContinue).Source
if (-not $iscc) { $iscc = "${env:ProgramFiles(x86)}\Inno Setup 6\ISCC.exe" }
if (Test-Path $iscc) {
  & $iscc /Q "/DAppVersion=$version" "/DArch=$Arch" "/DSourceExe=$((Resolve-Path $exe).Path)" "/Odist" installer\windows\tidy.iss
  if ($LASTEXITCODE -ne 0) { throw "Inno Setup failed" }
} else {
  Write-Warning "Inno Setup not found: only the portable .zip was made"
}

Get-ChildItem dist -Filter "$name*" | Where-Object { $_.Extension -ne '.sha256' } | ForEach-Object {
  "$((Get-FileHash $_.FullName -Algorithm SHA256).Hash.ToLower())  $($_.Name)"
} | Set-Content "dist\$name.sha256"
Get-ChildItem dist -Filter "$name*"
