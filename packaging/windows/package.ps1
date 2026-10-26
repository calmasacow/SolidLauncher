# Pack a Windows release of SolidLauncher.
#
# Usage: packaging/windows/package.ps1 -Version <version> -Arch <x64|arm64> -Binary <path>
# Writes dist/solid-launcher-<version>-windows-<arch>.zip
#
# A portable zip, to match how this launcher installs SolidCraft itself: unpack it
# anywhere and run it, with no installer and no administrator rights.
param(
    [Parameter(Mandatory = $true)][string]$Version,
    [Parameter(Mandatory = $true)][ValidateSet('x64', 'arm64')][string]$Arch,
    [Parameter(Mandatory = $true)][string]$Binary
)
$ErrorActionPreference = 'Stop'

$Root = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
Set-Location $Root

$Name = "solid-launcher-$Version-windows-$Arch"
$Dist = Join-Path $Root 'dist'
$Stage = Join-Path $Dist $Name

if (Test-Path $Stage) { Remove-Item -Recurse -Force $Stage }
New-Item -ItemType Directory -Force -Path $Stage | Out-Null

Copy-Item $Binary (Join-Path $Stage 'solid-launcher.exe')
foreach ($file in 'README.md', 'LICENSE-MIT', 'LICENSE-APACHE', 'ATTRIBUTION.md') {
    Copy-Item (Join-Path $Root $file) $Stage
}

$Zip = Join-Path $Dist "$Name.zip"
if (Test-Path $Zip) { Remove-Item -Force $Zip }
Compress-Archive -Path $Stage -DestinationPath $Zip
Remove-Item -Recurse -Force $Stage

Write-Host "wrote $Zip"
(Get-FileHash -Algorithm SHA256 $Zip).Hash.ToLower() + "  $Name.zip"
