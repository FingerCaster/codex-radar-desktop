# v0.2.0 release implementation plan

## Ordered Checklist

- [x] Confirm clean `main`, authenticated GitHub access, expected remote, and
  absence of local/remote `v0.2.0` tag or release.
- [x] Update the four authoritative release version locations to `0.2.0` and
  verify no current-build version location was missed.
- [x] Run frontend lint/typecheck/tests/build and Rust fmt/check/tests/clippy.
- [ ] Commit the version and Trellis release artifacts with the repository's
  conventional commit style.
- [ ] Build fresh MSI and NSIS installers from the committed tree.
- [ ] Compute installer hashes, create `SHA256SUMS.txt`, and verify the file
  against both local assets.
- [ ] Push `main`, create and push annotated tag `v0.2.0`, and verify the remote
  branch and peeled tag commit.
- [ ] Create a draft GitHub Release with all three assets, verify its metadata
  and digests, then publish it as latest.
- [ ] Record final URLs, sizes, hashes, and remote verification; archive the
  completed task and add the session journal entry.

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
