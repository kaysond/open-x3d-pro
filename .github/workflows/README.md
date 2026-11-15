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
| `driver-host-test` | ubuntu-24.04 | `make -C driver/test`: the C pipeline against `core/testdata/vectors.txt` |
| `windows` | windows-2022 | msbuild the UMDF driver (Release x64), `InfVerif /v /h` on the INF, clippy + tests for the Windows cfg, `tauri build --no-bundle`; uploads the unsigned `openx3d.dll` + INF as artifact `driver-unsigned-<sha>` (7 days) |

`windows-2022` is pinned because it ships Visual Studio 2022 plus the full WDK (10.1.26100.x: InfVerif, Inf2Cat, signtool). The `windows-2025` images only have the WDK VSIX.

## `build.yml` (shared by nightly and release)

Inputs `artifact-prefix` and `sign` (default false); optional secrets `SIGNING_PFX` and `SIGNING_PFX_PASSWORD`, read by one step only. A `build` job on windows-2022 (`timeout-minutes: 60`, `contents: read` only) and an `attest` job on ubuntu-24.04, the only one with `id-token: write` + `attestations: write`, so the build's third-party actions, npm scripts and `build.rs` never hold the OIDC token:

1. msbuild the driver; `driver/scripts/package.ps1` runs Inf2Cat and leaves `openx3d.dll` and `openx3d.cat` unsigned. The installer signs the catalog on the user's PC (`sign-catalog.ps1`).
2. Stage `openx3d.inf/.dll/.cat` into `app/src-tauri/resources/driver/`, which `hooks.nsh` embeds together with `driver/scripts/sign-catalog.ps1`, then `npm run tauri -- build --bundles nsis`.
3. Smoke test under Windows PowerShell 5.1: run `sign-catalog.ps1` on a copy of the staged files, `signtool verify /pa` the catalog and `/c` that it covers the DLL, check the certificate is in `Root` + `TrustedPublisher` with EKU code signing only and a critical `CA:FALSE`, that no private key is left, and that `-Remove` deletes it.
4. Collect the setup exe. With `sign`, sign it (`Set-AuthenticodeSignature`, SHA-256, no timestamp) with the certificate in `SIGNING_PFX`; the build fails if the secret is missing or `README.md` does not contain that certificate's SHA-256 fingerprint. `npm ci`, the Tauri build and every third-party action have finished before this step, the only one that sees the secrets.
5. Upload artifact `<prefix>-<sha7>` (14 days); the `attest` job downloads it and runs `actions/attest-build-provenance` on every file:
   - `Open-X3D-Pro_<ver>_x64-setup.exe`
   - `BUILD_INFO.txt` (commit, ref, build time, run URL, signing certificate fingerprint or `unsigned`)
   - `SHA256SUMS.txt`

Verify a download with `gh attestation verify <file> --repo kaysond/open-x3d-pro --source-ref <refs/tags/vX.Y.Z or refs/heads/main> --signer-workflow kaysond/open-x3d-pro/.github/workflows/build.yml`; `--repo` alone accepts builds of any branch. See [README: Verifying a download](../../README.md#verifying-a-download). The installer creates a per-PC driver certificate only if the user ticks the box on its first page (or passes `/TRUSTCERT` to a silent/passive install). See [README: Driver signing](../../README.md#driver-signing).

The app version is never changed for nightlies (NSIS needs plain semver); the sha in the artifact name and `BUILD_INFO.txt` identify the build.

## `nightly.yml`

1. `build`: `build.yml`, without `sign`.

Every push to `main` builds `open-x3d-pro-nightly-<sha7>`, unsigned. A newer push cancels a running nightly. No tag or GitHub Release is created.

Manual runs (Actions, Nightly, **Run workflow**) can pick any branch, e.g. to get an installer for a PR branch.

### Downloading a nightly

Artifacts need a signed-in GitHub account.

- Browser: Actions, Nightly, open the run, **Artifacts** at the bottom.
- CLI: `gh run download --repo kaysond/open-x3d-pro -n open-x3d-pro-nightly-<sha7>` (searches all runs; add a run id to pick one).

## `release.yml` (Run workflow on `main`, or tag `v*`)

0. `bump` (only when started with **Run workflow** on `main`): writes the version to `Cargo.toml` and `Cargo.lock`, `app/package.json` and its lock file, the INF `DriverVer` (with today's date), every version field of `openx3d.rc` and `X3D_DRIVER_VERSION_BCD` in `device.h`; commits `chore(release): vX.Y.Z` to `main`, tags it and starts this workflow again on the tag. The jobs below then run on the tag, so a release's attestation still names `refs/tags/vX.Y.Z`.

1. `check` (ubuntu-24.04): checks that the tagged commit is on `main` and that the tag version matches `app/package.json`, root `Cargo.toml` (which Tauri also uses), the INF `DriverVer`, the `openx3d.rc` `ProductVersion` (`a.b.c.0`) and `X3D_DRIVER_VERSION_BCD`.
2. `build`: `build.yml` with `sign: true` and the two signing secrets.
3. `publish` (the only job with `contents: write`, in environment `release`): GitHub Release with the artifact's files. `v0.*` tags and tags with a `-suffix` are published as pre-releases.

Repository settings to set once: required reviewers on the `release` environment, a tag ruleset that limits who can create `v*` tags, and the release signing certificate.

### Release signing certificate

A self-signed certificate limited to code signing, valid 10 years. Windows does not trust it; users compare its fingerprint with the README.

```sh
openssl req -x509 -newkey rsa:3072 -sha256 -days 3650 -noenc -keyout key.pem -out cert.pem \
  -subj "/CN=Open X3D Pro release signing" \
  -addext "basicConstraints=critical,CA:FALSE" \
  -addext "keyUsage=critical,digitalSignature" \
  -addext "extendedKeyUsage=critical,codeSigning"
openssl pkcs12 -export -inkey key.pem -in cert.pem -out signing.pfx   # asks for a password
base64 -w0 signing.pfx | gh secret set SIGNING_PFX
gh secret set SIGNING_PFX_PASSWORD                                    # asks for the same password
# The fingerprint for README "Release signature"
openssl x509 -in cert.pem -noout -fingerprint -sha256 | cut -d= -f2 | tr -d :
shred -u key.pem signing.pfx
```

Put the fingerprint in the README and commit `cert.pem` as `release-signing-cert.pem` before tagging. To replace the certificate, repeat this and update both in the same commit; older releases then no longer match the README.

## Cutting a release

1. Wait for CI and the nightly on `main`.
2. Actions tab, **Release**, **Run workflow** on `main`, enter the version (`0.2.0`, or `0.2.0-rc.1` for a pre-release). Or from a terminal: `gh workflow run release.yml --ref main -f version=0.2.0`.
3. Approve the `publish` job if the `release` environment has required reviewers.

The bump commit is pushed by the workflow, so it gets no CI or nightly run of its own; the release run builds it. If branch protection or a tag ruleset is added later, `github-actions[bot]` needs to be allowed to push to `main` and create `v*` tags.

Tagging by hand still works: bump the version files yourself, merge to `main`, then `git tag v0.2.0 && git push --tags`.

## Re-running

- Failed run: Actions tab, open the run, **Re-run all jobs**.
- Release for an existing tag: re-run the run, or delete the GitHub Release and push the tag again (`git push --delete origin v0.1.0 && git push origin v0.1.0`). Re-running onto an existing release overwrites assets with the same name.
