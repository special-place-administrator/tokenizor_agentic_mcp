# NPM Release Workflow Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Make the GitHub release workflow build the actual npm package artifact, validate release version consistency, and publish the exact tarball that was built.

**Architecture:** Keep release versioning explicit in source control instead of mutating `package.json` inside CI. Validate the tag against `Cargo.toml` and `npm/package.json`, build binary assets and the npm tarball separately, attach both to the GitHub release, and publish the built tarball to npm.

**Tech Stack:** GitHub Actions, npm, Node.js, Cargo

---

### Task 1: Align npm manifest version with the current release version

**Files:**
- Modify: `npm/package.json`

**Step 1: Update the npm package version**

Set `npm/package.json` `version` to match the current Rust package version and current git release tag.

**Step 2: Verify the manifest value**

Run: `node -p "require('./npm/package.json').version"`

Expected: `0.3.5`

### Task 2: Validate release tag/manifests in CI

**Files:**
- Modify: `.github/workflows/release.yml`

**Step 1: Add a release validation job**

Fail the workflow unless:
- `${GITHUB_REF_NAME#v}` matches `Cargo.toml` package version
- `${GITHUB_REF_NAME#v}` matches `npm/package.json` version

**Step 2: Wire downstream jobs to the validation job**

Make binary build, npm package build, release creation, and npm publish depend on the validation result.

### Task 3: Build the npm tarball in CI

**Files:**
- Modify: `.github/workflows/release.yml`

**Step 1: Add an npm package build job**

Use Node setup plus `npm pack` in `npm/` to build the tarball.

**Step 2: Upload the tarball as an artifact**

Make the workflow preserve the `.tgz` file for later jobs.

### Task 4: Publish the built tarball

**Files:**
- Modify: `.github/workflows/release.yml`

**Step 1: Include the npm tarball in the GitHub release assets**

Download the tarball artifact in the release job and attach it alongside the platform binaries.

**Step 2: Publish the downloaded tarball to npm**

Replace directory-based `npm publish` with publishing the built `.tgz` artifact.

### Task 5: Verify locally

**Files:**
- None

**Step 1: Run manifest/version checks**

Run:
- `node -p "require('./npm/package.json').version"`
- `cargo pkgid`

**Step 2: Build the npm package locally**

Run: `cd npm && npm pack`

Expected: a `tokenizor-mcp-0.3.5.tgz` tarball is created successfully.

**Step 3: Run the Rust test suite**

Run: `cargo test`

Expected: all tests pass.
