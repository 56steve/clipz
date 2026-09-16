<#
.SYNOPSIS
  Package an already-built Clipz.exe as an unsigned MSIX.

.DESCRIPTION
  Tauri cannot emit MSIX, so this stages the payload the way makeappx expects
  and calls it. Runs on Windows only: makeappx.exe ships with the Windows SDK.

  The output is UNSIGNED on purpose. The Microsoft Store signs submissions with
  the publisher identity in the manifest, so signing here would only be replaced.
  To sideload it for testing you must sign it yourself and trust that certificate.

.PARAMETER Configuration
  release (default) or debug, matching the cargo profile that was built.
#>
param(
    [ValidateSet('release', 'debug')]
    [string]$Configuration = 'release'
)

$ErrorActionPreference = 'Stop'

$repoRoot = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
$version = (Get-Content (Join-Path $repoRoot 'src-tauri\tauri.conf.json') -Raw | ConvertFrom-Json).version
# MSIX versions are always four parts; the app's own version carries three.
$msixVersion = "$version.0"
Write-Host "Packaging Clipz $version as MSIX $msixVersion"

$exe = Join-Path $repoRoot "src-tauri\target\$Configuration\clipz.exe"
if (-not (Test-Path $exe)) {
    throw "Built executable not found at $exe. Run 'npx tauri build' first."
}

$stage = Join-Path $repoRoot 'target-msix\stage'
if (Test-Path $stage) { Remove-Item $stage -Recurse -Force }
New-Item -ItemType Directory -Path (Join-Path $stage 'Assets') -Force | Out-Null

# The manifest names the executable Clipz.exe; cargo emits lowercase clipz.exe.
Copy-Item $exe (Join-Path $stage 'Clipz.exe')

$icons = Join-Path $repoRoot 'src-tauri\icons'
foreach ($asset in @('StoreLogo.png', 'Square150x150Logo.png', 'Square44x44Logo.png')) {
    $source = Join-Path $icons $asset
    if (-not (Test-Path $source)) { throw "Missing required MSIX asset: $source" }
    Copy-Item $source (Join-Path $stage "Assets\$asset")
}

(Get-Content (Join-Path $PSScriptRoot 'AppxManifest.template.xml') -Raw).Replace('{{VERSION}}', $msixVersion) |
    Set-Content (Join-Path $stage 'AppxManifest.xml') -Encoding UTF8

# makeappx lives in a version-stamped SDK folder; take the newest x64 one.
$makeappx = Get-ChildItem 'C:\Program Files (x86)\Windows Kits\10\bin' -Recurse -Filter 'makeappx.exe' -ErrorAction SilentlyContinue |
    Where-Object { $_.FullName -match '\\x64\\' } |
    Sort-Object FullName -Descending |
    Select-Object -First 1
if (-not $makeappx) {
    throw 'makeappx.exe not found. Install the Windows 10/11 SDK.'
}

$output = Join-Path $repoRoot "target-msix\Clipz_$version`_x64.msix"
& $makeappx.FullName pack /d $stage /p $output /o
if ($LASTEXITCODE -ne 0) { throw "makeappx failed with exit code $LASTEXITCODE" }

Write-Host "MSIX written to $output"
