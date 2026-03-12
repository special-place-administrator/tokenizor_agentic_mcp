#!/usr/bin/env python
"""Operator entrypoints for Tokenizor release and publish workflow."""

from __future__ import annotations

import argparse
import json
import re
import shutil
import subprocess
import sys
from pathlib import Path

CANONICAL_TAG_RE = re.compile(r"^v?\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?(?:\+[0-9A-Za-z.-]+)?$")


class ReleaseOpsError(RuntimeError):
    """Raised when a release operation cannot be completed safely."""


def repo_root(path: str | None = None) -> Path:
    if path is not None:
        return Path(path).resolve()
    return Path(__file__).resolve().parent.parent


def normalize_release_tag(tag: str) -> str:
    cleaned = tag.strip()
    if not cleaned:
        raise ReleaseOpsError("release tag must not be empty")
    if not CANONICAL_TAG_RE.fullmatch(cleaned):
        raise ReleaseOpsError(
            f"canonical release tags must use plain vX.Y.Z format, got '{cleaned}'"
        )
    return cleaned if cleaned.startswith("v") else f"v{cleaned}"


def guide_text() -> str:
    return """Tokenizor release operator guide

Fresh terminal commands:
  python execution/release_ops.py status
  python execution/release_ops.py preflight
  python execution/release_ops.py push-main

Repository prerequisites:
  - GitHub Actions workflow permissions must be `Read and write`.
  - GitHub Actions must be allowed to create and approve pull requests.
  - Canonical release tags are plain `vX.Y.Z`.

Normal publish flow:
  1. Make sure your branch is `main` and the working tree is clean.
  2. Make sure at least one unreleased commit uses conventional commit syntax such as `fix:`, `fix(scope):`, or `feat:`.
  3. Run `python execution/release_ops.py preflight`.
  4. Run `python execution/release_ops.py push-main`.
  5. Wait for the release PR opened by `release-please`.
  6. Merge that release PR on GitHub.
  7. GitHub Actions builds binaries, uploads release assets, and publishes npm.

Recovery flow for an existing tag:
  python execution/release_ops.py rebuild --tag v0.3.12

Source of truth:
  - docs/release-process.md
  - .github/workflows/release.yml
  - execution/version_sync.py
"""


def recommended_next_steps(branch: str, clean: bool) -> list[str]:
    if branch != "main":
        return [
            f"Current branch is '{branch}'. Switch to 'main' before running push-main.",
            "If you only need a reminder of the release flow, run `python execution/release_ops.py guide`.",
        ]
    if not clean:
        return [
            "Working tree is dirty. Commit or stash changes before running push-main.",
            "When the tree is clean, run `python execution/release_ops.py preflight`.",
        ]
    return [
        "Branch and working tree are ready for release preflight.",
        "Next commands: `python execution/release_ops.py preflight` then `python execution/release_ops.py push-main`.",
    ]


def run_checked(
    args: list[str],
    *,
    cwd: Path,
    capture_output: bool = False,
) -> str:
    resolved_args = [resolve_executable(args[0]), *args[1:]]
    completed = subprocess.run(
        resolved_args,
        cwd=cwd,
        text=True,
        capture_output=capture_output,
        check=False,
    )
    if completed.returncode != 0:
        rendered = " ".join(resolved_args)
        message = f"command failed: {rendered}"
        if capture_output:
            stderr = completed.stderr.strip()
            stdout = completed.stdout.strip()
            detail = stderr or stdout
            if detail:
                message = f"{message}\n{detail}"
        raise ReleaseOpsError(message)
    return completed.stdout.strip() if capture_output else ""


def try_capture(args: list[str], *, cwd: Path) -> str | None:
    resolved_args = [resolve_executable(args[0]), *args[1:]]
    completed = subprocess.run(
        resolved_args,
        cwd=cwd,
        text=True,
        capture_output=True,
        check=False,
    )
    if completed.returncode != 0:
        return None
    return completed.stdout.strip()


def resolve_executable(executable: str) -> str:
    resolved = shutil.which(executable)
    return resolved or executable


def parse_github_repo_slug(remote_url: str) -> str | None:
    cleaned = remote_url.strip()
    if not cleaned:
        return None
    if cleaned.endswith(".git"):
        cleaned = cleaned[:-4]
    for prefix in ("https://github.com/", "git@github.com:"):
        if cleaned.startswith(prefix):
            slug = cleaned[len(prefix) :]
            return slug if slug.count("/") == 1 else None
    return None


def origin_repo_slug(root: Path) -> str | None:
    remote_url = try_capture(["git", "remote", "get-url", "origin"], cwd=root)
    if remote_url is None:
        return None
    return parse_github_repo_slug(remote_url)


def github_workflow_permissions(root: Path) -> tuple[str, bool] | None:
    gh = shutil.which("gh")
    slug = origin_repo_slug(root)
    if gh is None or slug is None:
        return None

    completed = subprocess.run(
        [gh, "api", f"repos/{slug}/actions/permissions/workflow"],
        cwd=root,
        text=True,
        capture_output=True,
        check=False,
    )
    if completed.returncode != 0:
        return None

    try:
        payload = json.loads(completed.stdout)
    except json.JSONDecodeError:
        return None

    permissions = payload.get("default_workflow_permissions")
    can_approve = payload.get("can_approve_pull_request_reviews")
    if not isinstance(permissions, str) or not isinstance(can_approve, bool):
        return None
    return permissions, can_approve


def latest_canonical_tag(root: Path) -> str | None:
    output = try_capture(["git", "tag", "--list", "v*"], cwd=root)
    if output is None:
        return None
    tags = [line.strip() for line in output.splitlines() if line.strip()]
    if not tags:
        return None

    def version_key(tag: str) -> tuple[tuple[int, ...], str]:
        version = tag[1:]
        core = version.split("+", 1)[0].split("-", 1)[0]
        suffix = version[len(core) :]
        numbers = tuple(int(part) for part in core.split("."))
        return numbers, suffix

    return max(tags, key=version_key)


def current_branch(root: Path) -> str:
    return run_checked(["git", "rev-parse", "--abbrev-ref", "HEAD"], cwd=root, capture_output=True)


def git_is_clean(root: Path) -> bool:
    return run_checked(["git", "status", "--short"], cwd=root, capture_output=True) == ""


def current_version(root: Path) -> str:
    return run_checked(
        [sys.executable, str(root / "execution" / "version_sync.py"), "current"],
        cwd=root,
        capture_output=True,
    )


def release_metadata_is_aligned(root: Path) -> bool:
    completed = subprocess.run(
        [sys.executable, str(root / "execution" / "version_sync.py"), "check"],
        cwd=root,
        text=True,
        capture_output=True,
        check=False,
    )
    return completed.returncode == 0


def preflight_steps(root: Path) -> list[tuple[str, list[str], Path]]:
    return [
        (
            "Verify release metadata alignment",
            [sys.executable, str(root / "execution" / "version_sync.py"), "check"],
            root,
        ),
        (
            "Run execution unit tests",
            [sys.executable, "-m", "unittest", "discover", "-s", "execution", "-p", "test_*.py"],
            root,
        ),
        ("Run npm tests", ["npm", "test"], root / "npm"),
        ("Check Rust formatting", ["cargo", "fmt", "--all", "--check"], root),
        ("Run Rust tests", ["cargo", "test", "--all-targets", "--", "--test-threads=1"], root),
    ]


def run_preflight(root: Path) -> None:
    for label, args, cwd in preflight_steps(root):
        print(f"==> {label}")
        run_checked(args, cwd=cwd)


def cmd_guide(args: argparse.Namespace) -> int:
    _ = args
    print(guide_text())
    return 0


def cmd_status(args: argparse.Namespace) -> int:
    root = repo_root(args.root)
    branch = current_branch(root)
    clean = git_is_clean(root)
    version = current_version(root)
    aligned = release_metadata_is_aligned(root)
    repo_slug = origin_repo_slug(root)
    workflow_permissions = github_workflow_permissions(root)
    upstream = try_capture(["git", "rev-parse", "--abbrev-ref", "--symbolic-full-name", "@{upstream}"], cwd=root)
    latest_tag = latest_canonical_tag(root)

    print(f"Repo root: {root}")
    print(f"Branch: {branch}")
    print(f"Working tree: {'clean' if clean else 'dirty'}")
    print(f"Canonical version: {version}")
    print(f"Release metadata: {'aligned' if aligned else 'drifted'}")
    if repo_slug:
        print(f"Origin repo: {repo_slug}")
    if workflow_permissions:
        permissions, can_approve = workflow_permissions
        pr_status = "enabled" if can_approve else "disabled"
        print(f"GitHub workflow permissions: {permissions}; PR create/approve {pr_status}")
    elif repo_slug:
        print("GitHub workflow permissions: unavailable (authenticate `gh` to inspect repository settings)")
    if upstream:
        print(f"Upstream: {upstream}")
    if latest_tag:
        print(f"Latest tag: {latest_tag}")
    print("")
    for line in recommended_next_steps(branch, clean):
        print(line)
    return 0


def cmd_preflight(args: argparse.Namespace) -> int:
    root = repo_root(args.root)
    run_preflight(root)
    print("Release preflight passed.")
    return 0


def cmd_push_main(args: argparse.Namespace) -> int:
    root = repo_root(args.root)
    branch = current_branch(root)
    if branch != "main":
        raise ReleaseOpsError(f"refusing to push: current branch is '{branch}', expected 'main'")
    if not git_is_clean(root):
        raise ReleaseOpsError("refusing to push: working tree is dirty")
    if not args.skip_preflight:
        run_preflight(root)
    print("==> Pushing main")
    run_checked(["git", "push", "origin", "main"], cwd=root)
    print("Push complete. If a release is due, release-please will open or update the release PR.")
    return 0


def cmd_rebuild(args: argparse.Namespace) -> int:
    root = repo_root(args.root)
    tag = normalize_release_tag(args.tag)
    gh = shutil.which("gh")
    if gh is None:
        raise ReleaseOpsError(
            "GitHub CLI 'gh' is required for rebuild dispatch. "
            f"Manual command: gh workflow run Release --ref main -f tag={tag}"
        )
    run_checked([gh, "workflow", "run", "Release", "--ref", "main", "-f", f"tag={tag}"], cwd=root)
    print(f"Triggered Release workflow rebuild for {tag}.")
    return 0


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        description="Canonical operator commands for Tokenizor release and publish workflow."
    )
    parser.add_argument(
        "--root",
        default=None,
        help="Repository root to operate on. Defaults to the current repository.",
    )
    subparsers = parser.add_subparsers(dest="command", required=True)

    guide = subparsers.add_parser("guide", help="Print the release operator runbook.")
    guide.set_defaults(func=cmd_guide)

    status = subparsers.add_parser("status", help="Show current repo release readiness.")
    status.set_defaults(func=cmd_status)

    preflight = subparsers.add_parser("preflight", help="Run the local release preflight checks.")
    preflight.set_defaults(func=cmd_preflight)

    push_main = subparsers.add_parser(
        "push-main",
        help="Run preflight and push the current main branch to origin.",
    )
    push_main.add_argument(
        "--skip-preflight",
        action="store_true",
        help="Push without rerunning preflight checks.",
    )
    push_main.set_defaults(func=cmd_push_main)

    rebuild = subparsers.add_parser(
        "rebuild",
        help="Trigger the GitHub Release workflow for an existing tag.",
    )
    rebuild.add_argument("--tag", required=True, help="Existing release tag, for example v0.3.12.")
    rebuild.set_defaults(func=cmd_rebuild)

    return parser


def main(argv: list[str] | None = None) -> int:
    args = build_parser().parse_args(argv)
    try:
        return args.func(args)
    except ReleaseOpsError as error:
        print(error, file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
