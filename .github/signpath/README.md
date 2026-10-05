# SignPath

All builds are signed by [SignPath Foundation](https://signpath.org) on SignPath's HSM: releases (`release.yml`, tags `v*`) through policy `release-signing`, nightlies (`nightly.yml`, pushes to `main`) through policy `nightly-signing`. `build.yml` submits two signing requests per build, each approved by hand in the SignPath portal:

| Artifact configuration | File here | Signs |
|---|---|---|
| `driver` | `driver.xml` | `openx3d.dll` and `openx3d.cat` (Authenticode) |
| `installer` | `installer.xml` | `Open-X3D-Pro_<ver>_x64-setup.exe` (Authenticode; the app exe and uninstaller inside stay unsigned in v0.x) |

SignPath uses the copies in the portal; these files are the reference. Change both together.

## One-time setup

1. Apply at <https://signpath.org/apply> (conditions: <https://signpath.org/terms>; the root README's "Code signing policy" section covers the homepage requirements). After acceptance SignPath creates the organization; make it match the values below.
2. Turn on MFA for SignPath and GitHub (required by the terms).
3. Install the [SignPath GitHub App](https://github.com/apps/signpath) on `kaysond/open-x3d-pro`.
4. Organization: add the predefined trusted build system **GitHub.com**.
5. Project: slug `open-x3d-pro`, repository URL `https://github.com/kaysond/open-x3d-pro`, link the GitHub.com trusted build system.
6. CI user (e.g. `github-actions`): create it and generate its API token. A personal user's token is rejected once trusted build system verification is on.
7. Signing policy `release-signing`: SignPath Foundation certificate; submitter = the CI user; **approval process** on, approver = kaysond, 1 approval; trusted build system verification and origin verification on. Leave allowed branches empty until a first request shows which ref SignPath records for tag builds.
8. Signing policy `nightly-signing`: same SignPath Foundation certificate, same submitter, trusted build system and origin verification as `release-signing`; allowed branches empty (manual nightlies may build any branch) or `main`. Approval: the Foundation terms require every signing request to be approved, so keep the approval process on (approver = kaysond). Approve the nightlies you want; an unapproved request times out after 2 h and that nightly run fails.
9. Optional signing policy `test-signing` (test certificate, no approval): not used by the workflows; for a dry run, call `build.yml` with `signing-policy-slug: test-signing`.
10. Artifact configurations: **Add**, **Custom**, slug `driver`, paste `driver.xml`; again with slug `installer` and `installer.xml`. Both policies use them.
11. Repository secrets (GitHub, Settings, Secrets and variables, Actions); `nightly.yml` and `release.yml` both fail fast without them:
    - `SIGNPATH_API_TOKEN`: the CI user's API token.
    - `SIGNPATH_ORGANIZATION_ID`: the organization ID (shown on the signing policy page).

## Each release

- Bump the version in `tauri.conf.json`, `app/package.json`, `Cargo.toml`, the INF `DriverVer` and the `openx3d.rc` `ProductVersion` (`a.b.c.0`; SignPath rejects the DLL otherwise), then push the tag.
- A few minutes after the push the `driver` request waits for approval (SignPath e-mails the approver); the `installer` request follows once the NSIS build is done. Approve each within 2 hours (`wait-for-completion-timeout-in-seconds: 7200`) or the job fails.
- After a timeout or failure: cancel any request still pending in SignPath, then use **Re-run all jobs** (new requests, new approvals). With pipeline policies active SignPath accepts at most 3 re-runs of one run; after that push the tag again.

## Nightlies

- Every push to `main` (and every manual Nightly run) submits a `driver` and then an `installer` request to `nightly-signing`. Approve both within 2 hours to get a signed nightly; ignored requests time out and the run fails, which is expected.
- A newer push cancels the running nightly in GitHub, but not its SignPath request: cancel or ignore it in the portal.
- `tauri.conf.json` version and `openx3d.rc` `ProductVersion` (`a.b.c.0`) must match between releases too, or the `driver` request is rejected by the metadata restriction.
