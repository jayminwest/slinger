---
name: release
description: Cut a release of slinger. Use when the user says "release", "cut a release", "ship X.Y.Z", or "bump the version".
---

# Release

CI does the publishing. A release is a version-bump commit on main. When
`.github/workflows/release.yml` sees a version with no tag, it runs the gates,
tags `vX.Y.Z`, and publishes a GitHub release. The release notes are that
version's CHANGELOG section.

1. **Check that it is worth releasing.** `## [Unreleased]` must hold at least
   one change a user can observe. Internal-only changes wait for the next real
   release.
2. **Pick the version.** Default to a patch bump. Use minor for new
   capabilities and major for breaking changes. Before 1.0, use minor for
   breaking changes.
3. **Bump it.**
   - Set `version` under `[workspace.package]` in the root `Cargo.toml`, and the
     same `version` on each path dependency under `[workspace.dependencies]`.
   - Run `cargo update --workspace --offline` to update the lockfile.
4. **Edit the changelog.** Rename `## [Unreleased]` to `## [X.Y.Z] - YYYY-MM-DD`
   and add a fresh, empty `## [Unreleased]` above it. Write the entries for
   users.
5. **Verify.** Run `cargo xtask check-all`. It must be green.
6. **Commit and push.** Commit with `chore(release): X.Y.Z`, then push to
   main.
7. **Confirm.** Watch the run with `gh run watch`, then confirm the release
   with `gh release view vX.Y.Z`.

Never tag by hand. Never let generated notes stand in for the CHANGELOG.
