---
name: release
description: Prepare a Rin release - bump the version everywhere it is pinned, verify, and hand the manifest change and tag to the user.
argument-hint: "<new version, e.g. 0.2.0>"
disable-model-invocation: true
---

# Release $ARGUMENTS

Pushing a `vX.Y.Z` tag runs `.github/workflows/publish-and-release.yml`. It checks the tag
against `Cargo.toml`, publishes `tohsaka` to crates.io, builds the binaries and
updates the Homebrew tap. Everything here only prepares that tag.

Current version:

!`grep -m1 '^version' Cargo.toml`

## Steps

1. `git fetch`, then confirm the branch is up to date with its upstream and the tree is clean.
   Stop and ask if not.
2. The new version must be `MAJOR.MINOR.PATCH` and greater than the current one. Ask if
   `$ARGUMENTS` is missing or invalid.
3. Bump every pinned copy of the version:
   - `Cargo.toml` `version` (a manifest: edit it, never commit it).
   - `Cargo.lock`: run `cargo check` so the `tohsaka` entry updates (also a manifest).
   - `examples/team/rin.yml` `version:` (teams must pin the rin version they run).
   - `README.md`: the install command's `--tag vX.Y.Z`.
   - Then `git grep -n '<old version>'` and decide for each hit whether it is a pin. Test
     fixtures such as the `"v0.1.0"` git revision in `src/host_cli/generate.rs` are not.
4. Run `scripts/verify.sh matrix`, and `cargo publish --dry-run --locked --package tohsaka --allow-dirty`
   (dirty because the manifest is uncommitted).
5. Commit the non-manifest files as `release: X.Y.Z`.
6. Hand over to the user, and do not do these yourself:
   - commit `Cargo.toml` and `Cargo.lock`;
   - push, then `git tag vX.Y.Z && git push origin vX.Y.Z` once the bump is on `main`.

Linux and Docker paths are only exercised by the workflow and by `rin up`; say so
rather than claiming them verified.
