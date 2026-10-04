# Workflows

## `ci.yml` (push to `main`, every PR)

| Job | Runner | Does |
|---|---|---|
| `rust-linux` | ubuntu-24.04 | `cargo fmt --check`, `cargo clippy -D warnings`, `cargo test` for the whole workspace (Win32 code is stubbed on Linux) |
| `frontend` | ubuntu-24.04 | `npm ci`, lint, typecheck, tests (if present), `npm run build` in `app/` |
| `driver-host-test` | ubuntu-24.04 | `make -C driver/test`: the C pipeline against `core/testdata/vectors.json` |
| `windows` | windows-2022 | msbuild the UMDF driver (Release x64), `InfVerif /v /w` on the INF, clippy + tests for the Windows cfg, `tauri build --no-bundle`; uploads the unsigned `openx3d.dll` + INF as artifact `driver-unsigned-<sha>` (7 days) |

`windows-2022` is pinned because it ships Visual Studio 2022 plus the full WDK (10.1.26100.x: InfVerif, Inf2Cat, signtool). The `windows-2025` images only have the WDK VSIX.

## `release.yml` (tag `v*`)

On windows-2022: checks that the tag version matches `tauri.conf.json`, `app/package.json`, root `Cargo.toml` and the INF `DriverVer`; builds the driver; signs it with `driver/scripts/package.ps1` (Inf2Cat + signtool); stages it into `app/src-tauri/resources/driver/`; builds the NSIS installer with the Tauri CLI; signs the installer; publishes a GitHub Release with:

- `Open-X3D-Pro_<ver>_x64-setup.exe`
- `openx3d-driver-<ver>.zip` (inf, dll, cat, cer, install.ps1, uninstall.ps1)
- `openx3d.cer`
- `SHA256SUMS.txt`

`v0.*` tags and tags with a `-suffix` are published as pre-releases.

### Secrets (optional)

| Secret | Value |
|---|---|
| `SIGN_PFX_BASE64` | code-signing certificate + private key as a base64 PFX: `[Convert]::ToBase64String([IO.File]::ReadAllBytes('cert.pfx'))` |
| `SIGN_PFX_PASSWORD` | the PFX password |

Without both, each release is signed with a fresh self-signed certificate `CN=Open X3D Pro (unsigned beta)`; the installer imports it as a trusted root and the release notes say so.

## Cutting a release

1. Bump the version in all four places above, merge to `main`, wait for CI.
2. `git tag v0.1.0 && git push --tags`

## Re-running

- Failed run: Actions tab, open the run, **Re-run failed jobs**.
- Release for an existing tag: re-run the run, or delete the GitHub Release and push the tag again (`git push --delete origin v0.1.0 && git push origin v0.1.0`). Re-running onto an existing release overwrites assets with the same name.
