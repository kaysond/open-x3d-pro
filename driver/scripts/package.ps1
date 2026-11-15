<#
.SYNOPSIS
  Stages the driver package and builds its unsigned openx3d.cat.
.DESCRIPTION
  Copies openx3d.dll (from -BuildDir) and openx3d.inf into -OutDir and runs Inf2Cat.
  The catalog is signed only on the target PC, by sign-catalog.ps1.
  Inf2Cat comes from PATH or the newest Windows Kits\10\bin\<ver>.
.EXAMPLE
  .\package.ps1 -BuildDir ..\openx3d\x64\Release -OutDir ..\out
#>
param(
    [Parameter(Mandatory)][string]$BuildDir,
    [Parameter(Mandatory)][string]$OutDir
)
$ErrorActionPreference = 'Stop'

$dll = Join-Path $BuildDir 'openx3d.dll'
$inf = Join-Path $PSScriptRoot '..\openx3d\openx3d.inf'
foreach ($f in $dll, $inf) { if (-not (Test-Path $f)) { throw "missing $f" } }

New-Item -ItemType Directory -Force -Path $OutDir | Out-Null
$OutDir = (Resolve-Path $OutDir).Path
Copy-Item $dll, $inf $OutDir -Force
Remove-Item (Join-Path $OutDir 'openx3d.cat') -ErrorAction SilentlyContinue

# One OS per decorated [Models] section; the INF has no model for plain 10_X64.
$inf2cat = (Get-Command Inf2Cat.exe -ErrorAction SilentlyContinue).Source
if (-not $inf2cat) {
    $inf2cat = (Get-ChildItem "${env:ProgramFiles(x86)}\Windows Kits\10\bin\*\x86\Inf2Cat.exe" -ErrorAction SilentlyContinue |
        Sort-Object { $_.VersionInfo.FileVersionRaw } -Descending | Select-Object -First 1).FullName
}
if (-not $inf2cat) { throw 'Inf2Cat.exe not found: install the WDK or put it on PATH' }
& $inf2cat "/driver:$OutDir" '/os:10_VB_X64,10_CO_X64' /verbose
if ($LASTEXITCODE -ne 0) { throw "Inf2Cat failed with exit code $LASTEXITCODE" }

Get-ChildItem $OutDir | Format-Table Name, Length -AutoSize
