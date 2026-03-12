#!/usr/bin/env python3
from __future__ import annotations

import argparse
import datetime as dt
import re
import sys
from dataclasses import dataclass
from pathlib import Path


TASK_PATTERN = re.compile(r"^\d+-T-.*\.md$")
FRONT_MATTER_PATTERN = re.compile(r"\A---\n(.*?)\n---\n", re.DOTALL)


@dataclass
class Task:
    path: Path
    fields: dict[str, str]
    body: str

    @property
    def task_id(self) -> int:
        raw = self.fields.get("task_id", "").strip()
        if raw.isdigit():
            return int(raw)
        prefix = self.path.name.split("-", 1)[0]
        return int(prefix)

    @property
    def status(self) -> str:
        return self.fields.get("status", "").strip()

    @property
    def title(self) -> str:
        return self.fields.get("title", self.path.stem)


def parse_front_matter(text: str) -> tuple[dict[str, str], str]:
    match = FRONT_MATTER_PATTERN.match(text)
    if not match:
        raise ValueError("missing front matter")
    front_matter = match.group(1)
    fields: dict[str, str] = {}
    for line in front_matter.splitlines():
        if not line.strip():
            continue
        if ":" not in line:
            raise ValueError(f"invalid front matter line: {line!r}")
        key, value = line.split(":", 1)
        fields[key.strip()] = value.strip()
    body = text[match.end() :]
    return fields, body


def serialize_task(task: Task) -> str:
    ordered_keys = [
        "doc_type",
        "task_id",
        "title",
        "status",
        "sprint",
        "parent_plan",
        "prev_task",
        "next_task",
        "created",
        "updated",
    ]
    seen = set()
    lines = ["---"]
    for key in ordered_keys:
        if key in task.fields:
            lines.append(f"{key}: {task.fields[key]}")
            seen.add(key)
    for key in sorted(task.fields):
        if key not in seen:
            lines.append(f"{key}: {task.fields[key]}")
    lines.append("---")
    return "\n".join(lines) + "\n" + task.body


def load_tasks(root: Path) -> list[Task]:
    tasks: list[Task] = []
    for path in sorted(root.rglob("*.md")):
        if not TASK_PATTERN.match(path.name):
            continue
        text = path.read_text(encoding="utf-8")
        fields, body = parse_front_matter(text)
        tasks.append(Task(path=path, fields=fields, body=body))
    tasks.sort(key=lambda task: (task.task_id, task.path.name))
    return tasks


def save_task(task: Task) -> None:
    task.path.write_text(serialize_task(task), encoding="utf-8")


def now_date() -> str:
    return dt.date.today().isoformat()


def ensure_single_in_progress(tasks: list[Task]) -> Task | None:
    in_progress = [task for task in tasks if task.status == "in_progress"]
    if len(in_progress) > 1:
        names = ", ".join(task.path.name for task in in_progress)
        raise SystemExit(f"multiple in_progress tasks found: {names}")
    return in_progress[0] if in_progress else None


def resolve_task(tasks: list[Task], ref: str) -> Task:
    for task in tasks:
        if ref == str(task.task_id) or ref == task.path.name or ref == task.path.stem or ref == str(task.path):
            return task
    raise SystemExit(f"task not found: {ref}")


def next_pending(tasks: list[Task], current: Task | None = None) -> Task | None:
    if current is not None:
        next_name = current.fields.get("next_task", "").strip()
        if next_name:
            for task in tasks:
                if task.path.name == next_name and task.status == "pending":
                    return task
    for task in tasks:
        if task.status == "pending":
            return task
    return None


def print_task(task: Task) -> None:
    print(f"path={task.path.as_posix()}")
    print(f"task_id={task.task_id}")
    print(f"title={task.title}")
    print(f"status={task.status}")
    print(f"parent_plan={task.fields.get('parent_plan', '')}")
    print(f"prev_task={task.fields.get('prev_task', '')}")
    print(f"next_task={task.fields.get('next_task', '')}")


def cmd_list(args: argparse.Namespace) -> int:
    tasks = load_tasks(Path(args.root))
    for task in tasks:
        print(f"{task.task_id:03d}  {task.status:11}  {task.path.name}  {task.title}")
    return 0


def cmd_resume(args: argparse.Namespace) -> int:
    tasks = load_tasks(Path(args.root))
    current = ensure_single_in_progress(tasks)
    if current is None:
        current = next_pending(tasks)
        if current is None:
            print("no pending tasks")
            return 0
        current.fields["status"] = "in_progress"
        current.fields["updated"] = now_date()
        save_task(current)
    print_task(current)
    return 0


def cmd_complete(args: argparse.Namespace) -> int:
    tasks = load_tasks(Path(args.root))
    ensure_single_in_progress(tasks)
    task = resolve_task(tasks, args.task_ref)
    if task.status != "in_progress":
        raise SystemExit(f"task is not in_progress: {task.path.name}")
    task.fields["status"] = "done"
    task.fields["updated"] = now_date()
    save_task(task)
    print(f"completed={task.path.as_posix()}")
    if args.advance:
        tasks = load_tasks(Path(args.root))
        current = next_pending(tasks, current=task)
        if current is None:
            print("no pending tasks")
            return 0
        current.fields["status"] = "in_progress"
        current.fields["updated"] = now_date()
        save_task(current)
        print("advanced_to:")
        print_task(current)
    return 0


def cmd_set_status(args: argparse.Namespace) -> int:
    tasks = load_tasks(Path(args.root))
    if args.status == "in_progress":
        ensure_single_in_progress([task for task in tasks if task.path.name != args.task_ref and str(task.task_id) != args.task_ref])
    task = resolve_task(tasks, args.task_ref)
    if args.status == "in_progress":
        current = ensure_single_in_progress(tasks)
        if current is not None and current.path != task.path:
            raise SystemExit(f"another task is already in_progress: {current.path.name}")
    task.fields["status"] = args.status
    task.fields["updated"] = now_date()
    save_task(task)
    print_task(task)
    return 0


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description="Manage execution-plan task files.")
    subparsers = parser.add_subparsers(dest="command", required=True)

    list_parser = subparsers.add_parser("list", help="List task files.")
    list_parser.add_argument("root")
    list_parser.set_defaults(func=cmd_list)

    resume_parser = subparsers.add_parser("resume", help="Return current in-progress task or promote the next pending task.")
    resume_parser.add_argument("root")
    resume_parser.set_defaults(func=cmd_resume)

    complete_parser = subparsers.add_parser("complete", help="Mark an in-progress task done.")
    complete_parser.add_argument("root")
    complete_parser.add_argument("task_ref")
    complete_parser.add_argument("--advance", action="store_true")
    complete_parser.set_defaults(func=cmd_complete)

    status_parser = subparsers.add_parser("set-status", help="Set task status explicitly.")
    status_parser.add_argument("root")
    status_parser.add_argument("task_ref")
    status_parser.add_argument("status", choices=["pending", "in_progress", "done"])
    status_parser.set_defaults(func=cmd_set_status)

    return parser


def main() -> int:
    parser = build_parser()
    args = parser.parse_args()
    return args.func(args)


if __name__ == "__main__":
    sys.exit(main())
