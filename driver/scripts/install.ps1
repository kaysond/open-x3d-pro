#Requires -RunAsAdministrator
<#
.SYNOPSIS
  Trusts the beta certificate, installs openx3d.inf and prints the resulting binding.
.PARAMETER Dir
  Folder holding openx3d.inf/.dll/.cat/.cer (default: this script's folder).
#>
param([string]$Dir = $PSScriptRoot)
$ErrorActionPreference = 'Stop'

$cer = Join-Path $Dir 'openx3d.cer'
$inf = Join-Path $Dir 'openx3d.inf'
foreach ($f in $cer, $inf, (Join-Path $Dir 'openx3d.cat'), (Join-Path $Dir 'openx3d.dll')) {
    if (-not (Test-Path $f)) { throw "missing $f" }
}

$x509 = New-Object System.Security.Cryptography.X509Certificates.X509Certificate2 $cer
# Only a self-signed (nightly) certificate goes into the stores; a CA-issued one is shared with
# other publishers, so Windows' one-time publisher prompt is the right trust boundary.
$stores = if ($x509.Subject -eq $x509.Issuer) { 'Root', 'TrustedPublisher' } else { @() }
foreach ($store in $stores) {
    certutil -addstore -f $store $cer | Out-Null
    if ($LASTEXITCODE -ne 0) { throw "certutil -addstore $store failed ($LASTEXITCODE)" }
}

pnputil /add-driver $inf /install
$rc = $LASTEXITCODE
switch ($rc) {
    0 { }
    3010 { Write-Warning 'Installed; a reboot is required.' }
    259 { Write-Warning 'Driver added to the store but no device was updated. Is the stick plugged in?' }
    default { throw "pnputil /add-driver failed with exit code $rc" }
}

Start-Sleep -Seconds 3 # let PnP restart the stack and hidclass enumerate Col01/Col02

$devices = Get-PnpDevice -PresentOnly -ErrorAction SilentlyContinue |
    Where-Object { $_.InstanceId -like 'USB\VID_046D&PID_C215*' -or $_.InstanceId -like 'HID\VID_046D&PID_C215*' }
if (-not $devices) { Write-Warning 'No Extreme 3D Pro present.' }
foreach ($d in $devices) {
    Write-Output ''
    Write-Output "== $($d.InstanceId) [$($d.Status)] $($d.FriendlyName)"
    foreach ($key in 'DEVPKEY_Device_Service', 'DEVPKEY_Device_Stack', 'DEVPKEY_Device_DriverInfPath', 'DEVPKEY_Device_DriverRank') {
        $v = (Get-PnpDeviceProperty -InstanceId $d.InstanceId -KeyName $key -ErrorAction SilentlyContinue).Data
        if ($key -eq 'DEVPKEY_Device_DriverRank' -and $null -ne $v) { $v = '0x{0:X8}' -f [uint32]$v }
        Write-Output ('   {0,-14} {1}' -f $key.Replace('DEVPKEY_Device_', ''), ($v -join ', '))
    }
    # /stack needs Win10 2004+; older pnputil just prints usage
    pnputil /enum-devices /instanceid $d.InstanceId /stack /drivers
}

Write-Output ''
Write-Output '== setupapi.dev.log (recent ranking lines)'
Get-Content "$env:windir\INF\setupapi.dev.log" -Tail 5000 |
    Select-String -Pattern 'VID_046D&PID_C215|openx3d|Rank|Signer Score' |
    Select-Object -Last 40 | ForEach-Object { $_.Line }

Write-Output ''
Write-Output 'Expected: USB node service mshidumdf, stack WUDFRd + mshidumdf; HID ...&Col01 and &Col02 present.'
