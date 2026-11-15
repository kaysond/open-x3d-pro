<p align="center"><img src="logo.svg" alt="Open X3D Pro logo" width="128"></p>

<h1 align="center">Open X3D Pro</h1>

<p align="center">
  <a href="https://github.com/kaysond/open-x3d-pro/actions/workflows/ci.yml"><img src="https://github.com/kaysond/open-x3d-pro/actions/workflows/ci.yml/badge.svg" alt="CI"></a>
  <a href="https://github.com/kaysond/open-x3d-pro/actions/workflows/nightly.yml"><img src="https://github.com/kaysond/open-x3d-pro/actions/workflows/nightly.yml/badge.svg" alt="Nightly"></a>
  <a href="https://github.com/kaysond/open-x3d-pro/releases/latest"><img src="https://img.shields.io/github/v/release/kaysond/open-x3d-pro" alt="Latest release"></a>
</p>

Open-source replacement for Logitech Gaming Software 5.10 for the Logitech Extreme 3D Pro joystick on Windows 10 22H2 and Windows 11 24H2/25H2 x64. It provides per-axis calibration, deadzones, response curves, inversion, button remapping, hat-as-buttons, a shift layer and per-application profiles applied automatically by a tray agent. It contains no kernel-mode code, so it installs and runs with Secure Boot and Memory Integrity (HVCI) on, unlike LGS 5.10's 2010-era kernel filter.

## Installation

1. Download `Open-X3D-Pro_<version>_x64-setup.exe` from the [releases page](https://github.com/kaysond/open-x3d-pro/releases). Optionally [verify it](#verifying-a-download) first.
2. Run it. If SmartScreen shows "Windows protected your PC", choose "More info", then "Run anyway".
3. Keep the driver certificate checkbox on the first page ticked so the driver is installed (see [Driver signing](#driver-signing)).

## Driver signing

### Why the installer asks to create a certificate

Windows installs a driver package only if its catalog (`openx3d.cat`, the list of hashes of the driver's files) is signed by a certificate the PC trusts. The driver runs in user mode (UMDF 2), so it does not need the Microsoft (WHQL or attestation) signature that kernel drivers require; any trusted code-signing certificate works.

Open X3D Pro does what [libwdi/Zadig](https://github.com/pbatard/libwdi/wiki/Certification-Practice-Statement) does: the catalog ships **unsigned** and is signed on your PC, by a certificate made on your PC, whose private key is then destroyed. The installer's first page has a checkbox, ticked by default. While it is ticked, [`sign-catalog.ps1`](driver/scripts/sign-catalog.ps1) runs elevated and:

1. Removes any certificate from an earlier Open X3D Pro install.
2. Creates a new self-signed code-signing certificate, `CN=Open X3D Pro local driver signing (key destroyed)`, with a non-exportable private key in the machine store (usable by administrators only).
3. Signs `openx3d.cat` with it.
4. Deletes the private key.
5. Only then adds the certificate (public part only) to the local machine's _Trusted Root Certification Authorities_ store, which makes the signature valid, and its _Trusted Publishers_ store, which stops Windows asking "Would you like to install this device software?". The key never exists while the certificate is trusted.

Then the driver is installed. Other cases:

- **Unticked:** only the app is installed and the stick keeps the stock Windows driver, so there are no curves or remapping in games. Run the installer again to change your mind.
- **Uninstall** removes the certificate from both stores.
- **Upgrades:** passive (`/P`) upgrades and silent `/UPDATE` installs keep the driver if it was installed before.
- **Other silent (`/S`) and passive (`/P`) installs** do this only when given `/TRUSTCERT`.

### What the certificate can and cannot do

Because the private key is deleted seconds after it is created, before the certificate is trusted, **nobody can sign anything with this certificate**: not an attacker, not malware, not the Open X3D Pro developers. Its only effect is that Windows accepts the one catalog it already signed. There is no project-wide driver key to steal, and each PC's certificate is different.

Its extensions limit it further, all marked critical so Windows enforces them:

| Extension          | Value                                   | Effect                                                                                                                               |
| ------------------ | --------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------ |
| Extended Key Usage | Code Signing (`1.3.6.1.5.5.7.3.3`) only | Windows rejects it for websites (TLS server or client), e-mail (S/MIME), document signing, smart-card logon and every other purpose. |
| Key Usage          | Digital Signature only                  | It cannot be used to encrypt data or exchange keys.                                                                                  |
| Basic Constraints  | `CA:FALSE` (end entity)                 | It cannot issue other certificates. Windows rejects any certificate chain that names it as the issuer.                               |

It also cannot sign kernel-mode drivers. With Secure Boot on, Windows 10 1607+ loads only kernel drivers signed by Microsoft, whatever is in the local stores.

### How to check

```powershell
# The certificate this PC trusts (expect one entry in Root and one in TrustedPublisher)
Get-ChildItem Cert:\LocalMachine\Root, Cert:\LocalMachine\TrustedPublisher |
  Where-Object Subject -like '*Open X3D Pro*' |
  Format-List PSParentPath, Subject, Thumbprint, NotAfter, HasPrivateKey, EnhancedKeyUsageList

# Its extensions: "End Entity", "Digital Signature", "Code Signing" and nothing else
certutil -store Root "Open X3D Pro local driver signing (key destroyed)"

# The private key is gone: this must print nothing
Get-ChildItem Cert:\CurrentUser\My, Cert:\LocalMachine\My | Where-Object Subject -like '*Open X3D Pro*'

# The catalog it signed, as installed in the driver store
Get-WindowsDriver -Online | Where-Object OriginalFileName -like '*\openx3d.inf' |
  ForEach-Object { Get-AuthenticodeSignature (Join-Path (Split-Path $_.OriginalFileName) openx3d.cat) } |
  Format-List Path, Status, SignerCertificate

# Remove it by hand (elevated); the driver stays installed until you uninstall it
Get-ChildItem Cert:\LocalMachine\Root, Cert:\LocalMachine\TrustedPublisher |
  Where-Object Subject -eq 'CN=Open X3D Pro local driver signing (key destroyed)' | Remove-Item
```

`HasPrivateKey` must be `False`.

### Limits

- **Smart App Control** (Windows 11), when **on**, blocks the installer and the app, with no per-app override. It only accepts certificates from CAs in Microsoft's [Trusted Root Program](https://learn.microsoft.com/en-us/windows/apps/develop/smart-app-control/code-signing-for-smart-app-control) and ignores roots added locally, and it checks every executable, not just downloads. It would very likely also block the locally signed driver DLL. Its _evaluation_ mode blocks nothing, and it is off on most PCs. To use Open X3D Pro, turn it off in Windows Security, App & browser control; since the April 2026 update (KB5083769) it can be turned back on later.

## Verifying a download

Nightlies and releases are built only by GitHub Actions from this repository: [`build.yml`](.github/workflows/build.yml), called by [`nightly.yml`](.github/workflows/nightly.yml) and [`release.yml`](.github/workflows/release.yml). Each build carries `BUILD_INFO.txt` (commit, ref, build time, run URL, signing certificate) and `SHA256SUMS.txt`. Check a download before running it.

### Release signature

The setup exe of every release is signed by the release workflow with the project's self-signed certificate. Like the driver certificate it is limited to code signing (critical `CA:FALSE`, Extended Key Usage Code Signing only). Its private key is a GitHub Actions secret that only the release workflow receives. The certificate (public part only) is [`release-signing-cert.pem`](release-signing-cert.pem); its SHA-256 fingerprint is:

```
8DC5DB41F8E0B876A86188DC8DF5ADDD7E95AA33C8EF719836EA86CABA19DE73
```

```powershell
$sig = Get-AuthenticodeSignature .\Open-X3D-Pro_0.1.0_x64-setup.exe
# Must print the fingerprint above
[BitConverter]::ToString([Security.Cryptography.SHA256]::Create().ComputeHash($sig.SignerCertificate.RawData)) -replace '-'
# 'UnknownError' is expected (Windows does not trust the certificate); 'HashMismatch' means the file was changed after signing
$sig.Status
```

- Windows does not know this certificate, so **SmartScreen** still shows "Windows protected your PC" (choose "More info", then "Run anyway") and **UAC** shows "Unknown publisher". Do not install the certificate: comparing the fingerprint is the whole check, and the installer never adds it to any store.
- Nightlies, the app exe and the uninstaller are not signed. Check nightlies by attestation.

### Provenance and attestation

Every file of every nightly and release gets a [GitHub build-provenance attestation](https://docs.github.com/actions/security-for-github-actions/using-artifact-attestations) (SLSA provenance, signed through Sigstore with the workflow's OIDC identity). It proves the file was built from a specific commit of `kaysond/open-x3d-pro` by a specific workflow run:

```sh
# A release: built from that tag by the release workflow
gh attestation verify Open-X3D-Pro_0.1.0_x64-setup.exe --repo kaysond/open-x3d-pro \
  --source-ref refs/tags/v0.1.0 --signer-workflow kaysond/open-x3d-pro/.github/workflows/build.yml
# A nightly: built from main (any branch can be built by hand, so pin the ref)
gh attestation verify Open-X3D-Pro_0.1.0_x64-setup.exe --repo kaysond/open-x3d-pro \
  --source-ref refs/heads/main --signer-workflow kaysond/open-x3d-pro/.github/workflows/build.yml
```

`--repo` alone accepts any attested build of the repository, including a manual run of a modified workflow on a branch.

Without the GitHub CLI, compare the file's hash (`Get-FileHash` in PowerShell) with `SHA256SUMS.txt` and the release notes, and look the attestation up under the repository's **Attestations** page.

## AI Usage Disclosure and Policy

Claude Code was used to assist with the development of this code base, including code / documentation generation and code review. Architectural and implementation decisions are human-only, and every line of code has been reviewed by a human.

Contributions must disclose if AI tools are used and to what extent. Drive-by pull requests are not accepted; open an issue first.

## License

MIT. See [LICENSE](LICENSE).
