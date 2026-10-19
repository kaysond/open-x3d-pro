# Workflows

| Workflow | Trigger | Output |
|---|---|---|
| `ci.yml` | push to `main`, every PR | static checks + Windows compile, no packaging |
| `nightly.yml` | push to `main`, manual (`workflow_dispatch`) | unsigned installer (app + driver) with a build-provenance attestation, as a 14-day workflow artifact |
| `release.yml` | tag `v*` | the same build, published as a GitHub Release |
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

Input `artifact-prefix`; no secrets. A `build` job on windows-2022 (`timeout-minutes: 60`, `contents: read` only) and an `attest` job on ubuntu-24.04, the only one with `id-token: write` + `attestations: write`, so the build's third-party actions, npm scripts and `build.rs` never hold the OIDC token. Nothing is code-signed:

1. msbuild the driver; `driver/scripts/package.ps1` runs Inf2Cat and leaves `openx3d.dll` and `openx3d.cat` unsigned. The installer signs the catalog on the user's PC (`sign-catalog.ps1`).
2. Stage `openx3d.inf/.dll/.cat` into `app/src-tauri/resources/driver/`, which `hooks.nsh` embeds together with `driver/scripts/sign-catalog.ps1`, then `npm run tauri -- build --bundles nsis`.
3. Smoke test under Windows PowerShell 5.1: run `sign-catalog.ps1` on a copy of the staged files, `signtool verify /pa` the catalog and `/c` that it covers the DLL, check the certificate is in `Root` + `TrustedPublisher` with EKU code signing only and a critical `CA:FALSE`, that no private key is left, and that `-Remove` deletes it.
4. Upload artifact `<prefix>-<sha7>` (14 days); the `attest` job downloads it and runs `actions/attest-build-provenance` on every file:
   - `Open-X3D-Pro_<ver>_x64-setup.exe`
   - `BUILD_INFO.txt` (commit, ref, build time, run URL)
   - `SHA256SUMS.txt`

Verify a download with `gh attestation verify <file> --repo kaysond/open-x3d-pro`. The installer creates a per-PC driver certificate only if the user ticks the box on its first page (or passes `/TRUSTCERT` to a silent/passive install). See [README: Driver signing](../../README.md#driver-signing).

The app version is never changed for nightlies (NSIS needs plain semver); the sha in the artifact name and `BUILD_INFO.txt` identify the build.

## `nightly.yml`

1. `build`: `build.yml`.

Every push to `main` builds `open-x3d-pro-nightly-<sha7>`. A newer push cancels a running nightly. No tag or GitHub Release is created.

Manual runs (Actions, Nightly, **Run workflow**) can pick any branch, e.g. to get an installer for a PR branch.

### Downloading a nightly

Artifacts need a signed-in GitHub account.

- Browser: Actions, Nightly, open the run, **Artifacts** at the bottom.
- CLI: `gh run download --repo kaysond/open-x3d-pro -n open-x3d-pro-nightly-<sha7>` (searches all runs; add a run id to pick one).

## `release.yml` (tag `v*`)

1. `check` (ubuntu-24.04): checks that the tag version matches `tauri.conf.json`, `app/package.json`, root `Cargo.toml`, the INF `DriverVer` and the `openx3d.rc` `ProductVersion` (`a.b.c.0`).
2. `build`: `build.yml`.
3. `publish` (the only job with `contents: write`): GitHub Release with the artifact's files. `v0.*` tags and tags with a `-suffix` are published as pre-releases.

No repository secrets are needed.

## Cutting a release

1. Bump the version in all five places above, merge to `main`, wait for CI and the nightly.
2. `git tag v0.1.0 && git push --tags`.

## Re-running

- Failed run: Actions tab, open the run, **Re-run all jobs**.
- Release for an existing tag: re-run the run, or delete the GitHub Release and push the tag again (`git push --delete origin v0.1.0 && git push origin v0.1.0`). Re-running onto an existing release overwrites assets with the same name.
