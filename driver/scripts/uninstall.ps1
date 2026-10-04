#Requires -RunAsAdministrator
<#
.SYNOPSIS
  Removes openx3d from the driver store (the stick falls back to inbox hidusb)
  and deletes the beta certificate from Root and TrustedPublisher.
#>
$ErrorActionPreference = 'Stop'

# pnputil labels are localized, so match file names only: split the listing into
# per-driver blocks and take the oemNN.inf of every block that mentions openx3d.inf.
$blocks = ((pnputil /enum-drivers) -join "`n") -split "`n\s*`n"
$published = foreach ($b in $blocks) {
    if ($b -match '(?i)\bopenx3d\.inf\b' -and $b -match '(?i)\b(oem\d+\.inf)\b') { $Matches[1] }
}

if (-not $published) { Write-Output 'openx3d.inf is not in the driver store.' }
foreach ($oem in $published) {
    Write-Output "Removing $oem"
    pnputil /delete-driver $oem /uninstall /force
    if ($LASTEXITCODE -notin 0, 3010) { Write-Warning "pnputil /delete-driver $oem exited with $LASTEXITCODE" }
}

pnputil /scan-devices | Out-Null

foreach ($store in 'Root', 'TrustedPublisher') {
    Get-ChildItem "Cert:\LocalMachine\$store" |
        Where-Object Subject -eq 'CN=Open X3D Pro (unsigned beta)' |
        ForEach-Object {
            certutil -delstore $store $_.Thumbprint | Out-Null
            Write-Output "Removed certificate $($_.Thumbprint) from $store"
        }
}
