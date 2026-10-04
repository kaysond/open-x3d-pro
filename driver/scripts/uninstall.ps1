#Requires -RunAsAdministrator
<#
.SYNOPSIS
  Removes openx3d from the driver store (the stick falls back to inbox hidusb)
  and deletes the beta certificate from Root and TrustedPublisher.
#>
$ErrorActionPreference = 'Stop'

# pnputil /delete-driver only takes published oemNN.inf names; Get-WindowsDriver maps
# them without parsing pnputil's localized text.
$published = Get-WindowsDriver -Online | Where-Object { $_.OriginalFileName -like '*\openx3d.inf' } | ForEach-Object Driver

if (-not $published) { Write-Output 'openx3d.inf is not in the driver store.' }
foreach ($oem in $published) {
    Write-Output "Removing $oem"
    pnputil /delete-driver $oem /uninstall
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
