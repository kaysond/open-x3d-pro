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

function Find-Tool([string]$Name, [string]$Arch) {
    $cmd = Get-Command $Name -ErrorAction SilentlyContinue
    if ($cmd) { return $cmd.Source }
    $hit = Get-ChildItem "${env:ProgramFiles(x86)}\Windows Kits\10\bin\*\$Arch\$Name" -ErrorAction SilentlyContinue |
        Sort-Object { $_.VersionInfo.FileVersionRaw } -Descending | Select-Object -First 1
    if (-not $hit) { throw "$Name not found: install the WDK or put it on PATH" }
    return $hit.FullName
}

function Invoke-Tool([string]$Exe, [string[]]$Arguments) {
    & $Exe @Arguments
    if ($LASTEXITCODE -ne 0) { throw "$(Split-Path -Leaf $Exe) failed with exit code $LASTEXITCODE" }
}

$dll = Join-Path $BuildDir 'openx3d.dll'
$inf = Join-Path $PSScriptRoot '..\openx3d\openx3d.inf'
foreach ($f in $dll, $inf) { if (-not (Test-Path $f)) { throw "missing $f" } }

New-Item -ItemType Directory -Force -Path $OutDir | Out-Null
$OutDir = (Resolve-Path $OutDir).Path
Copy-Item $dll, $inf $OutDir -Force
Remove-Item (Join-Path $OutDir 'openx3d.cat') -ErrorAction SilentlyContinue

# One OS per decorated [Models] section; the INF has no model for plain 10_X64.
$inf2cat = Find-Tool 'Inf2Cat.exe' 'x86'
Invoke-Tool $inf2cat @("/driver:$OutDir", '/os:10_VB_X64,10_CO_X64', '/verbose')

Get-ChildItem $OutDir | Format-Table Name, Length -AutoSize
