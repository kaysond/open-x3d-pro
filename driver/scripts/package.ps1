<#
.SYNOPSIS
  Stages the driver package, builds openx3d.cat and signs it.
.DESCRIPTION
  Copies openx3d.dll (from -BuildDir) and openx3d.inf into -OutDir and runs
  Inf2Cat. With -PfxPath it also signs the DLL and the catalog and verifies both.
  signtool/Inf2Cat come from PATH or the newest Windows Kits\10\bin\<ver>.
.EXAMPLE
  .\package.ps1 -BuildDir ..\openx3d\x64\Release -OutDir ..\out -PfxPath C:\keys\openx3d.pfx -PfxPassword 'pw'
#>
param(
    [Parameter(Mandatory)][string]$BuildDir,
    [Parameter(Mandatory)][string]$OutDir,
    [string]$PfxPath,
    [string]$PfxPassword
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

$sign = [bool]$PfxPath
if ($sign) {
    if (-not $PfxPassword) { throw '-PfxPassword is required with -PfxPath' }
    $PfxPath = (Resolve-Path $PfxPath).Path
    $signtool = Find-Tool 'signtool.exe' 'x64'
    $signArgs = @('sign', '/fd', 'sha256', '/td', 'sha256', '/tr', 'http://timestamp.digicert.com', '/f', $PfxPath, '/p', $PfxPassword)
    Invoke-Tool $signtool ($signArgs + (Join-Path $OutDir 'openx3d.dll'))
}

# One OS per decorated [Models] section; the INF has no model for plain 10_X64.
$inf2cat = Find-Tool 'Inf2Cat.exe' 'x86'
Invoke-Tool $inf2cat @("/driver:$OutDir", '/os:10_VB_X64,10_CO_X64', '/verbose')

if ($sign) {
    $cat = Join-Path $OutDir 'openx3d.cat'
    Invoke-Tool $signtool ($signArgs + $cat)
    foreach ($f in (Join-Path $OutDir 'openx3d.dll'), $cat) {
        & $signtool verify /pa /v $f
        if ($LASTEXITCODE -ne 0) {
            Write-Warning "signtool verify failed for $f. Expected unless the signing root is trusted on this machine."
        }
    }
} else {
    Write-Warning 'No -PfxPath: the package is unsigned and Windows will refuse to install it.'
}
Get-ChildItem $OutDir | Format-Table Name, Length -AutoSize
