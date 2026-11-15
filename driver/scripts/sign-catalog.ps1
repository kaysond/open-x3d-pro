#Requires -RunAsAdministrator
<#
.SYNOPSIS
  Signs openx3d.cat with a certificate created on this PC, then destroys its private key.
.DESCRIPTION
  The libwdi/Zadig approach: create a self-signed code-signing certificate, sign the catalog with it,
  delete the private key, and only then trust the certificate in LocalMachine\Root and
  LocalMachine\TrustedPublisher, so nothing can ever be signed with a trusted key. Certificates from
  earlier installs are removed first. With -Remove it only removes them. See README.md#driver-signing.
  Cmdlets only (no .NET calls), so it also runs under Constrained Language Mode.
.PARAMETER Dir
  Folder holding openx3d.cat (default: this script's folder).
#>
param([string]$Dir = $PSScriptRoot, [switch]$Remove)
$ErrorActionPreference = 'Stop'

# Uninstall and upgrades find earlier certificates by this subject; keep it stable.
$subject = 'CN=Open X3D Pro local driver signing (key destroyed)'

# -DeleteKey is a Cert: provider parameter, so it only binds when the path is given as an argument, not piped.
Get-ChildItem Cert:\LocalMachine\My | Where-Object Subject -eq $subject | ForEach-Object { Remove-Item $_.PSPath -DeleteKey }
Get-ChildItem Cert:\LocalMachine\Root, Cert:\LocalMachine\TrustedPublisher |
    Where-Object Subject -eq $subject | Remove-Item
if ($Remove) { return }

$cat = Join-Path $Dir 'openx3d.cat'
if (-not (Test-Path $cat)) { throw "missing $cat" }

# LocalMachine\My: the key container is readable by Administrators and SYSTEM only.
$cert = New-SelfSignedCertificate -Type CodeSigningCert -Subject $subject `
    -CertStoreLocation Cert:\LocalMachine\My -KeyAlgorithm RSA -KeyLength 3072 -HashAlgorithm SHA256 `
    -KeyExportPolicy NonExportable -NotAfter (Get-Date).AddYears(30) `
    -TextExtension @('2.5.29.19={critical}{text}ca=false')
# Users can create files in %SystemRoot%\Temp but not list, read or change ones Administrators create.
$public = Join-Path $env:SystemRoot "Temp\openx3d-$($cert.Thumbprint).cer"
try {
    # No timestamp: offline installs must work, and the certificate outlives any install.
    # Status is not Valid yet because the certificate is not trusted; checked below.
    $sig = Set-AuthenticodeSignature -FilePath $cat -Certificate $cert -HashAlgorithm SHA256
    if ($sig.SignerCertificate.Thumbprint -ne $cert.Thumbprint) { throw "signing $cat failed: $($sig.StatusMessage)" }
    Export-Certificate -Cert $cert -FilePath $public | Out-Null
} finally {
    Remove-Item "Cert:\LocalMachine\My\$($cert.Thumbprint)" -DeleteKey
}
try {
    foreach ($store in 'Root', 'TrustedPublisher') {
        Import-Certificate -FilePath $public -CertStoreLocation "Cert:\LocalMachine\$store" | Out-Null
    }
} finally {
    Remove-Item $public -ErrorAction SilentlyContinue
}
$sig = Get-AuthenticodeSignature $cat
if ($sig.Status -ne 'Valid') { throw "signed $cat does not verify: $($sig.Status) $($sig.StatusMessage)" }
Write-Output "Signed $cat as '$subject' ($($cert.Thumbprint)); private key deleted"
