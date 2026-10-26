# Pack a Windows release of SolidLauncher: a portable zip and an MSI installer.
#
# Usage: packaging/windows/package.ps1 -Version <version> -Arch <x64|arm64> -Binary <path>
#
# Writes dist/solid-launcher-<version>-windows-<arch>.zip
#        dist/solid-launcher-<version>-windows-<arch>.msi
#
# The zip is for people who want no installer, or whose machine will not run one. The MSI
# installs per-user into %LOCALAPPDATA%\Programs\SolidLauncher, asks for no administrator
# rights, and carries the dependency checks described in solid-launcher.wxs.
#
# Needs the WiX 5 CLI: dotnet tool install --global wix
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
New-Item -ItemType Directory -Force -Path $Dist | Out-Null

if (-not (Test-Path $Binary)) { throw "no binary at $Binary" }

# ---- portable zip ------------------------------------------------------------------------
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

# ---- licence, as the installer's UI wants it -----------------------------------------------
# WixUI_Minimal shows an RTF. The MIT licence is plain text, so it is wrapped rather than
# rewritten: every line of the original is kept, only escaped and line-broken for RTF.
$LicenceRtf = Join-Path $Dist 'licence.rtf'
$lines = Get-Content (Join-Path $Root 'LICENSE-MIT')
$body = ($lines | ForEach-Object {
        $_ -replace '\\', '\\\\' -replace '([{}])', '\$1'
    }) -join '\par' + '\par'
"{\rtf1\ansi\deff0{\fonttbl{\f0\fnil\fcharset0 Segoe UI;}}\fs18 $body}" |
    Set-Content -Path $LicenceRtf -Encoding ASCII

# ---- MSI -----------------------------------------------------------------------------------
# MSI ProductVersion takes at most four numeric fields, so a pre-release suffix such as
# 0.3.0-rc.1 is trimmed to 0.3.0 for the installer's own version. The file name keeps the
# full version.
$MsiVersion = ($Version -split '-')[0]

$Msi = Join-Path $Dist "$Name.msi"
if (Test-Path $Msi) { Remove-Item -Force $Msi }

& wix build `
    -arch $Arch `
    -ext WixToolset.UI.wixext `
    -d "Version=$MsiVersion" `
    -d "BinaryPath=$((Resolve-Path $Binary).Path)" `
    -d "SourceRoot=$Root" `
    -d "IconPath=$(Join-Path $Root 'assets\solid-launcher.ico')" `
    -d "LicenceRtf=$LicenceRtf" `
    -o $Msi `
    (Join-Path $PSScriptRoot 'solid-launcher.wxs')
if ($LASTEXITCODE -ne 0) { throw "wix build failed with $LASTEXITCODE" }

Remove-Item -Force $LicenceRtf
Write-Host "wrote $Msi"

foreach ($f in $Zip, $Msi) {
    (Get-FileHash -Algorithm SHA256 $f).Hash.ToLower() + "  " + (Split-Path -Leaf $f)
}
