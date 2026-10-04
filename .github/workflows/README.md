# Workflows

| Workflow | Trigger | Output |
|---|---|---|
| `ci.yml` | push to `main`, every PR | static checks + Windows compile, no packaging |
| `nightly.yml` | push to `main`, manual (`workflow_dispatch`) | installer + driver signed with a throwaway self-signed certificate, as a 14-day workflow artifact |
| `release.yml` | tag `v*` | the same build signed by SignPath Foundation (manually approved), published as a GitHub Release |
| `build.yml` | `workflow_call` only | the shared Windows build used by `nightly.yml` and `release.yml` |

## `ci.yml`

| Job | Runner | Does |
|---|---|---|
| `rust-linux` | ubuntu-24.04 | `cargo fmt --check`, `cargo clippy -D warnings`, `cargo test` for the whole workspace (Win32 code is stubbed on Linux) |
| `frontend` | ubuntu-24.04 | `npm ci`, lint, typecheck, tests (if present), `npm run build` in `app/` |
| `driver-host-test` | ubuntu-24.04 | `make -C driver/test`: the C pipeline against `core/testdata/vectors.json` |
| `windows` | windows-2022 | msbuild the UMDF driver (Release x64), `InfVerif /v /h` on the INF, clippy + tests for the Windows cfg, `tauri build --no-bundle`; uploads the unsigned `openx3d.dll` + INF as artifact `driver-unsigned-<sha>` (7 days) |

`windows-2022` is pinned because it ships Visual Studio 2022 plus the full WDK (10.1.26100.x: InfVerif, Inf2Cat, signtool). The `windows-2025` images only have the WDK VSIX.

## `build.yml` (shared by nightly and release)

Input `signing`, one job on windows-2022 (`timeout-minutes: 300`):

- `self-signed` (nightly): `driver/scripts/make-cert.ps1` makes a fresh `CN=Open X3D Pro (unsigned beta)` certificate, trusted in the runner's `LocalMachine\Root` so `signtool verify /pa` passes.
  1. msbuild the driver; `driver/scripts/package.ps1` signs `openx3d.dll`, runs Inf2Cat, signs `openx3d.cat` and exports `openx3d.cer` from the PFX.
  2. `npm run tauri -- build --bundles nsis --config <file>` with `bundle.windows.certificateThumbprint` set, so Tauri signs the app exe, the uninstaller and the setup exe.
- `signpath` (release): two round trips through [SignPath](../signpath/README.md), each waiting up to 2 h for manual approval in the SignPath portal.
  1. msbuild the driver; `package.ps1` without a PFX runs Inf2Cat and leaves the DLL and catalog unsigned. Both are uploaded as artifact `signpath-driver-unsigned` and submitted (configuration `driver`); the signed files come back, `signtool verify /pa /v` checks both and `/c` checks the catalog still covers the DLL. `openx3d.cer` and the CN come from the catalog's signer certificate (SignPath Foundation).
  2. `npm run tauri -- build --bundles nsis` without signing config. The setup exe is uploaded as `signpath-installer-unsigned` and submitted (configuration `installer`), then verified with `signtool verify /pa /v`. SignPath cannot deep-sign NSIS installers, so the app exe and uninstaller inside stay unsigned in v0.x.

Both modes then:

1. Stage `openx3d.inf/.dll/.cat/.cer` into `app/src-tauri/resources/driver/` (before the NSIS build), plus `cert-cn.txt` (the certificate CN) and, for self-signed builds only, `self-signed.flag`. `windows/hooks.nsh` imports the certificate into `Root` + `TrustedPublisher` only when `self-signed.flag` exists (nightlies); CA-signed releases change no certificate store and Windows prompts once for the publisher.
2. `signtool verify /pa` on the setup exe, DLL and catalog, then upload artifact `<prefix>-<sha7>` (14 days):
   - `Open-X3D-Pro_<ver>_x64-setup.exe`
   - `openx3d-driver-<ver>.zip` (inf, dll, cat, cer, install.ps1, uninstall.ps1)
   - `openx3d.cer`
   - `BUILD_INFO.txt` (commit, ref, build time, run URL, signer)
   - `SHA256SUMS.txt`

The app version is never changed for nightlies (NSIS needs plain semver); the sha in the artifact name and `BUILD_INFO.txt` identify the build.

## `nightly.yml`

Every push to `main` builds `open-x3d-pro-nightly-<sha7>`. A newer push cancels a running nightly. No tag or GitHub Release is created. The self-signed certificate is trusted as a root by the installer, so only install nightlies on test machines.

Manual runs (Actions, Nightly, **Run workflow**) can pick any branch, e.g. to get an installer for a PR branch.

### Downloading a nightly

Artifacts need a signed-in GitHub account.

- Browser: Actions, Nightly, open the run, **Artifacts** at the bottom.
- CLI: `gh run download --repo kaysond/open-x3d-pro -n open-x3d-pro-nightly-<sha7>` (searches all runs; add a run id to pick one).

## `release.yml` (tag `v*`)

1. `check` (ubuntu-24.04): fails immediately unless both SignPath secrets are set, then checks that the tag version matches `tauri.conf.json`, `app/package.json`, root `Cargo.toml`, the INF `DriverVer` and the `openx3d.rc` `ProductVersion` (`a.b.c.0`, enforced by SignPath's metadata restriction).
2. `build`: `build.yml` with `signing: signpath`; approve both signing requests in SignPath while it runs.
3. `publish` (the only job with `contents: write`): GitHub Release with the artifact's files. `v0.*` tags and tags with a `-suffix` are published as pre-releases.

### Secrets (required for releases)

| Secret | Value |
|---|---|
| `SIGNPATH_API_TOKEN` | API token of the SignPath CI user that may submit to `release-signing` |
| `SIGNPATH_ORGANIZATION_ID` | SignPath organization ID (shown on the signing policy page) |

Portal setup (project `open-x3d-pro`, policy `release-signing`, artifact configurations `driver` and `installer`): [`.github/signpath/README.md`](../signpath/README.md). There is no self-signed fallback for releases.

## Cutting a release

1. Bump the version in all five places above, merge to `main`, wait for CI and the nightly.
2. `git tag v0.1.0 && git push --tags`, then approve the two signing requests in SignPath (2 h limit each).

## Re-running

- Failed run: Actions tab, open the run, **Re-run failed jobs** (releases: cancel pending SignPath requests and use **Re-run all jobs**, see [`.github/signpath/README.md`](../signpath/README.md)).
- Release for an existing tag: re-run the run, or delete the GitHub Release and push the tag again (`git push --delete origin v0.1.0 && git push origin v0.1.0`). Re-running onto an existing release overwrites assets with the same name.
