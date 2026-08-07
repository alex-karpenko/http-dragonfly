---
name: release
description: Cut a new http-dragonfly release — bump version, regenerate changelog, tag, and push. Use when the user asks to release, cut a version, or publish a new version of http-dragonfly.
disable-model-invocation: true
---

Follow these steps in order. Confirm the target version with the user before making any changes if it wasn't given explicitly.

1. Make sure the working tree is clean and up to date on `main` (`git status`, `git pull`).
2. Retrieve current version from the `Cargo.toml` file.
3. Tag current commit with the retrieved version: `git tag vX.Y.Z`.
4. Regenerate the changelog: `git cliff --tag vX.Y.Z -o CHANGELOG.md` (config is `cliff.toml` at repo root; this only affects the `[Unreleased]`/new section — git-cliff is idempotent over prior entries).
5. Review the generated CHANGELOG.md diff with the user before committing.
6. Commit the changelog: `Update changelog [skip ci]` (matches existing history — `[skip ci]` avoids retriggering CI for a docs-only commit).
7. Push the commits to `main`: `git push`.
8. Push the tag: `git push --tags`. This is the step that triggers CI:
   - `.github/workflows/publish-release.yaml` runs git-cliff again to build the GitHub release body and publishes the release.
   - `.github/workflows/publish-image.yaml` builds and pushes the Docker image to `ghcr.io/alex-karpenko/http-dragonfly:vX.Y.Z` (and `:latest`), cosign-signed.
9. Confirm both workflows succeeded (`gh run list --workflow=publish-release.yaml --limit 1`, `gh run list --workflow=publish-image.yaml --limit 1`) and the GitHub release + image tag exist.

Pushing a tag is not reversible in any clean way (it kicks off a public release + image publish) — always get explicit confirmation from the user before step 8.
