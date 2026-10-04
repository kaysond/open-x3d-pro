<#
.SYNOPSIS
  Creates the self-signed code-signing certificate used for beta builds.
.DESCRIPTION
  Generates "CN=Open X3D Pro (unsigned beta)" (RSA 3072, SHA-256, 5 years) in
  Cert:\CurrentUser\My and exports openx3d.cer (public) and openx3d.pfx
  (password protected) into -OutDir. Feed the PFX to package.ps1.
.EXAMPLE
  .\make-cert.ps1 -OutDir C:\keys -PfxPassword 'correct horse'
#>
param(
    [string]$OutDir = '.',
    [Parameter(Mandatory)][string]$PfxPassword
)
$ErrorActionPreference = 'Stop'

New-Item -ItemType Directory -Force -Path $OutDir | Out-Null
$OutDir = (Resolve-Path $OutDir).Path

$cert = New-SelfSignedCertificate -Type CodeSigningCert `
    -Subject 'CN=Open X3D Pro (unsigned beta)' `
    -CertStoreLocation Cert:\CurrentUser\My `
    -KeyAlgorithm RSA -KeyLength 3072 -HashAlgorithm SHA256 `
    -KeyExportPolicy Exportable `
    -NotAfter (Get-Date).AddYears(5)

$cer = Join-Path $OutDir 'openx3d.cer'
$pfx = Join-Path $OutDir 'openx3d.pfx'
Export-Certificate -Cert $cert -FilePath $cer -Type CERT | Out-Null
Export-PfxCertificate -Cert $cert -FilePath $pfx `
    -Password (ConvertTo-SecureString $PfxPassword -AsPlainText -Force) | Out-Null

Write-Output "Thumbprint: $($cert.Thumbprint)"
Write-Output "Public cert: $cer"
Write-Output "PFX:         $pfx  (keep private; also left in Cert:\CurrentUser\My)"
