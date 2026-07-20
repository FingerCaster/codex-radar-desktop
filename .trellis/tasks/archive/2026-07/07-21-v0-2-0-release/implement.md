# v0.2.0 release implementation plan

## Ordered Checklist

- [x] Confirm clean `main`, authenticated GitHub access, expected remote, and
  absence of local/remote `v0.2.0` tag or release.
- [x] Update the four authoritative release version locations to `0.2.0` and
  verify no current-build version location was missed.
- [x] Run frontend lint/typecheck/tests/build and Rust fmt/check/tests/clippy.
- [x] Commit the version and Trellis release artifacts with the repository's
  conventional commit style.
- [x] Build fresh MSI and NSIS installers from the committed tree.
- [x] Compute installer hashes, create `SHA256SUMS.txt`, and verify the file
  against both local assets.
- [x] Push `main`, create and push annotated tag `v0.2.0`, and verify the remote
  branch and peeled tag commit.
- [x] Create a draft GitHub Release with all three assets, verify its metadata
  and digests, then publish it as latest.
- [x] Record final URLs, sizes, hashes, and remote verification. Archive and
  journal bookkeeping follow through the standard finish-work flow.

## Validation Commands

```powershell
pnpm lint
pnpm typecheck
pnpm test
pnpm build
cargo fmt --manifest-path src-tauri/Cargo.toml --all -- --check
cargo check --manifest-path src-tauri/Cargo.toml --all-targets --all-features
cargo test --manifest-path src-tauri/Cargo.toml
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --all-features -- -D warnings
pnpm tauri build
git diff --check
```

## Review Gates

- Do not tag before every quality command and both bundle targets pass.
- Do not push a tag that does not peel to the version commit.
- Do not publish the draft until all three asset names, sizes, and digests have
  been compared with local values.
- Do not claim or upload a macOS `0.2.0` binary from this Windows environment.

## Rollback Points

- Before push: fix locally and rebuild.
- After branch/tag push but before publication: leave any failed release draft
  unpublished and report it; never rewrite `main` or the tag silently.
- After publication: use a patch release for corrections unless the user gives
  explicit instructions to alter the existing release.

## Verification Results

- Release commit: `3120480481a12a9d5021050c539e0801692c6706`.
- Local and remote annotated tag `v0.2.0` both peel to the release commit.
- Frontend: lint, typecheck, and production build passed; 11 test files / 82
  tests passed.
- Rust: fmt, all-target/all-feature check, 74 tests, and clippy with denied
  warnings passed.
- `pnpm tauri build` produced the fresh Windows bundles below:
  - MSI: `5,730,304` bytes, SHA-256
    `CD088D5F9B40135D63BB6488C7E74B02B6E40B4F93179CB9BF5E743861C96954`
  - NSIS: `4,223,406` bytes, SHA-256
    `F1078F1905043DC1CDA907194AA2D96BA129C7C164E6CB5D559B6E1678631E0A`
- Published checksum asset: `196` bytes, SHA-256
  `F121058C419E112827ACD9A81968A0DF3CB570FA149A123CAE60B99A75F70A11`.
- GitHub independently reported matching digests and `uploaded` state for all
  three assets. Each public download returned HTTP 200 with the expected
  content length.
- Public latest release:
  `https://github.com/FingerCaster/codex-radar-desktop/releases/tag/v0.2.0`.
