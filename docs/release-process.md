# Release And Update Process

This repository now uses a GitHub-native release flow built around `release-please`.

The intended product behavior is simple:

- users install `tokenizor-mcp`
- launches self-heal local binary drift where possible
- launches do not silently attach to an incompatible old daemon
- release publishing stays version-aligned across Rust, npm, tags, and GitHub releases

## Why This Exists

The old tag-push release flow depended on humans keeping these in sync:

- `Cargo.toml`
- `npm/package.json`
- the git tag
- the GitHub release assets
- the npm publish step

That is error-prone. The new flow makes GitHub Actions own the release state and refuse mismatched versions.

## Source Of Truth

Canonical release version state lives in:

- `.github/.release-please-manifest.json`

Publishable manifests must match it:

- `Cargo.toml`
- `npm/package.json`

Local verification command:

```bash
python execution/version_sync.py check
```

Manual repair command:

```bash
python execution/version_sync.py set 0.3.13
```

Canonical fresh-terminal entrypoint:

```bash
python execution/release_ops.py guide
```

Most useful operator commands:

```bash
python execution/release_ops.py status
python execution/release_ops.py preflight
python execution/release_ops.py push-main
python execution/release_ops.py rebuild --tag v0.3.12
```

GitHub repository prerequisites:

- Settings -> Actions -> General -> Workflow permissions = `Read and write`
- Settings -> Actions -> General -> Allow GitHub Actions to create and approve pull requests = enabled
- canonical release tags must use plain `vX.Y.Z`

## Standard Release Flow

1. Merge normal feature and fix PRs into `main`.
2. GitHub Actions runs `.github/workflows/release.yml` on every push to `main`.
3. `release-please` updates or creates the release PR.
4. Merge the release PR when the proposed version and changelog are acceptable.
5. The same workflow run creates the GitHub release tag, builds binaries, uploads release assets, and publishes the npm tarball.

`python execution/release_ops.py status` now prints the detected GitHub repository slug and, when `gh` is authenticated, the current workflow-permission state so a fresh terminal can verify the prerequisite without guessing.

## Hotfix Validation Loop

For important runtime or installer fixes, do not stop at a green repo test run.

Use this loop:

1. ship the next patch release immediately
2. install that published package version
3. restart the MCP client session
4. retest through the installed MCP, not only the source checkout

That is the fastest way to catch wrapper, install-path, daemon-reuse, and stdio-surface regressions that repo-local tests can miss.

## Installed Runtime Self-Healing

The release pipeline is only half of the problem. Installed machines also need safe runtime convergence.

Current runtime behavior:

1. The npm wrapper launches through `npm/bin/launcher.js`.
2. On every launch it first tries to apply any staged `tokenizor-mcp.pending` binary.
3. It checks the installed binary version against the wrapper package version.
4. If the binary is missing or the versions differ, it reruns the installer automatically before spawning the binary.
5. The Rust stdio client checks the recorded daemon `/health` response before reusing it.
6. Daemon reuse is allowed only when both the daemon version and executable path match the current client executable.
7. If the recorded daemon is incompatible, the client best-effort stops it, clears stale rendezvous files, and starts a fresh daemon automatically.

That gives the user the commercial-style behavior we want:

- `npm install -g tokenizor-mcp`
- run the MCP
- wrapper and runtime repair ordinary local drift without manual cleanup

This still does not guarantee recovery from every hostile local condition, but version skew and ordinary stale-daemon reuse are now handled automatically in the normal path.

## Why Build And Publish Happen In The Same Workflow

GitHub does not reliably start a second workflow from workflow events created with the default `GITHUB_TOKEN`.

That matters because a release tool can create:

- release PRs
- tags
- GitHub releases

If a second workflow were waiting for those events, it could be skipped entirely.

So the repo now does this in one workflow:

1. `release-please` decides whether a release was created.
2. If yes, the workflow builds the exact tagged revision.
3. The workflow uploads assets to the GitHub release.
4. The workflow publishes the exact built npm tarball.

## Recovery And Rebuild Procedure

If the GitHub release exists but asset upload or npm publish failed:

1. Open the `Release` workflow in GitHub Actions.
2. Run `workflow_dispatch`.
3. Supply the existing tag, for example `v0.3.12`.
4. The workflow rebuilds from that tag and re-uploads assets with `--clobber`.
5. The npm publish step reuses the exact tarball built from that tag.

This avoids ad hoc local rebuilds and keeps recovery inside GitHub.

If GitHub reports `release-please failed: GitHub Actions is not permitted to create or approve pull requests`, fix the two repository settings above first. That failure is a repository Actions-permission problem, not a version-sync problem.

## Secrets And Environments

Required secret:

- `NPM_TOKEN`

Recommended secret:

- `RELEASE_PLEASE_TOKEN`

Why the optional token matters:

- if `release-please` uses only `GITHUB_TOKEN`, the single-workflow release path still works
- if you want other workflows to trigger from the release PR itself, use a PAT or GitHub App token in `RELEASE_PLEASE_TOKEN`

Recommended GitHub environment:

- `npm-publish`

Recommended protection settings for `npm-publish`:

- required reviewers
- prevent self-review

That turns npm publication into an explicit deployment gate instead of an unrestricted secret use.

## CI Guarantees

The normal CI workflow now checks:

- unit tests for `execution/version_sync.py`
- release-version alignment across the release manifest, Cargo, and npm manifests

The release workflow also re-checks the version alignment against the resolved tag before building publish artifacts.
It also refuses noncanonical tags so component-prefixed legacy tags cannot silently slip back into the normal path.

## Files Involved

- `.github/workflows/ci.yml`
- `.github/workflows/release.yml`
- `.github/.release-please-manifest.json`
- `.github/release-please-config.json`
- `execution/version_sync.py`
- `execution/test_version_sync.py`
- `npm/bin/launcher.js`
- `npm/bin/tokenizor-mcp.js`
- `src/daemon.rs`

## Operational Standard

The standard for this project is not "document the recovery steps and ask the user to do them."

The standard is:

- publish one coherent version
- install one coherent version
- on launch, converge local runtime state automatically
- only surface an error when automatic repair cannot safely finish the job
