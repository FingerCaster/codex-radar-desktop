# Publish v0.2.0 release

## Goal

Publish the current tested `main` branch as the public Windows `v0.2.0`
release, with synchronized application metadata, reproducible installer
checksums, and a verified GitHub Release.

## Background

- `main` is clean and seven commits ahead of `origin/main`.
- The existing public release is `v0.1.0`; neither a local nor remote
  `v0.2.0` tag exists at planning time.
- GitHub CLI authentication is active for `FingerCaster` with repository and
  workflow scopes.
- This Windows environment can produce MSI and NSIS installers. It cannot
  truthfully produce a new universal macOS DMG.
- The user explicitly requested packaging, pushing, and publishing `0.2.0`,
  and previously authorized autonomous task execution without another gate.

## Requirements

- Synchronize the release version to `0.2.0` in `package.json`,
  `src-tauri/Cargo.toml`, the root `model_radar` entry in
  `src-tauri/Cargo.lock`, and `src-tauri/tauri.conf.json`.
- Preserve all application behavior. The release commit may contain version
  metadata and Trellis release records only; no opportunistic feature changes.
- Run frontend lint, typecheck, tests, and production build plus Rust fmt,
  check, tests, and clippy with warnings denied.
- Build fresh Windows MSI and NSIS installers from the committed `0.2.0`
  source and generate a `SHA256SUMS.txt` asset for both files.
- Push the release commit and all preceding local commits to `origin/main`.
  Create and push one annotated `v0.2.0` tag pointing at that exact commit.
- Create `Codex Radar Desktop v0.2.0` as a draft GitHub Release, upload the
  MSI, NSIS, and checksum file, verify names/sizes/digests and tag target, then
  publish it as the latest non-prerelease release.
- Release notes must summarize the settings/context-menu/autostart workflow,
  persistent and preset window positioning, visual redesign, local model
  artwork, and main/distributed radar selection. State that Windows installers
  are unsigned and that no new macOS `0.2.0` binary is included.
- Never replace or mutate an existing `v0.2.0` tag or release. Any unexpected
  pre-existing remote object is a hard stop.

## Acceptance Criteria

- [x] All four authoritative version locations report exactly `0.2.0` and no
  tracked release metadata still identifies the current build as `0.1.0`.
- [x] The complete frontend and Rust quality gate passes from the release
  commit without weakening checks.
- [x] Fresh `Model Radar_0.2.0_x64_en-US.msi` and
  `Model Radar_0.2.0_x64-setup.exe` bundles exist and match the published
  SHA-256 checksum asset.
- [x] `origin/main` contains the release commit and local `main` is neither
  ahead nor behind after push.
- [x] Annotated tag `v0.2.0` exists locally and remotely and peels to the exact
  release commit.
- [x] GitHub Release `v0.2.0` is public, latest, non-prerelease, has exactly the
  three intended uploaded assets, and exposes working download URLs.
- [x] The final worktree is clean and the release task records the commit,
  artifact sizes, hashes, and GitHub URL.

## Out Of Scope

- macOS compilation, signing, notarization, code signing for Windows,
  auto-update manifests, package-manager publication, or rewriting the
  existing `v0.1.0` release.
