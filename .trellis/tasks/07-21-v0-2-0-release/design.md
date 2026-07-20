# v0.2.0 release design

## Release Boundary

The tagged source commit is the release identity. The four version locations
must be synchronized in one commit before any installer is built. Installers
and checksums are generated from that committed tree; no tracked files may
change between build completion and tag creation.

```text
version metadata -> quality gate -> Windows bundle -> hashes
       -> release commit push -> annotated tag push
       -> draft GitHub Release + assets -> remote verification -> publish latest
```

## Version Contract

`package.json`, `src-tauri/Cargo.toml`, `src-tauri/tauri.conf.json`, and the
`model_radar` package entry in `src-tauri/Cargo.lock` all carry `0.2.0`.
Dependency versions in `Cargo.lock` are not release metadata and remain
unchanged.

## Publication Transaction

Preflight rejects an existing local tag, remote tag, or GitHub Release named
`v0.2.0`. The repository is pushed before the tag. The annotated tag is then
pushed and verified to peel to the release commit.

The GitHub Release is created as a draft with all three assets in one command.
Remote metadata and asset digests are inspected while it is private. Only a
complete verified draft is changed to public/latest. If upload or verification
fails, leave the draft unpublished and report the exact state instead of
deleting or rewriting remote history.

## Assets

- `Model Radar_0.2.0_x64_en-US.msi`
- `Model Radar_0.2.0_x64-setup.exe`
- `SHA256SUMS.txt`

The checksum file uses uppercase SHA-256 followed by two spaces and the exact
asset filename on each line. GitHub's server-computed asset digest must match
the locally recorded value for both installers and the checksum file itself.

## Compatibility And Claims

The release is a Windows x64 release built on Windows. GitHub automatically
provides source archives. Release notes explicitly avoid claiming a macOS
`0.2.0` binary, signing, notarization, or auto-update support.

## Rollback

Before publication, a failure leaves either no GitHub Release or an unpublished
draft. After publication, do not silently delete or move the tag. Corrective
work requires a new patch release unless the user explicitly directs otherwise.
