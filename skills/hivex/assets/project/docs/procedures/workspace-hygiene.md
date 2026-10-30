---
title: "Workspace hygiene"
status: draft
created_at: 2026-10-30
tags: [workflow, cleanup, storage]
---

# Workspace hygiene

Work leaves residue outside its worktree: build caches keyed by checkout, simulators or devices, containers and images, copied evidence and builds, temporary folders, other tools' worktrees and CI caches. Removing branches and worktrees does not remove it, so it accumulates until the disk fills. This procedure declares where the project's residue lives and when it is no longer needed, so each piece of work removes what it created when it closes and a sweep removes what no live work still needs.

## Declare the residue classes

[`workspace-hygiene.json`](workspace-hygiene.json) declares the project's classes; it starts empty. Each class has:

- `name`: what the residue is.
- `paths`: where it lives, as glob patterns; `~` and environment variables expand, and relative patterns start at the project root.
- `unneeded`: a shell command, run from the project root with `RESIDUE` set to an item's path, that exits 0 when no live work needs the item, 1 when something still does and with any other code when it cannot tell. Base it on a fact a command can check, such as the checkout a cache records no longer existing, or the item's issue being closed and its branch integrated.
- `removal`: `deterministic` when an unneeded item can go without human judgment, or `review` when it is only listed for the responsible person to decide. Use `deterministic` only when the check is reliable, the item's owner can be established and the residue can be regenerated; evidence that may still need preserving is `review`.

For example, a build cache whose folders record the checkout they belong to:

```json
{
  "classes": [
    {
      "name": "Build cache per checkout",
      "paths": ["~/Library/Caches/example-builds/*"],
      "unneeded": "test ! -e \"$(cat \"$RESIDUE/checkout-path\")\"",
      "removal": "deterministic"
    }
  ]
}
```

Prefer locations whose name identifies the work, such as the issue or branch, so that the check can tell whether the work is still live. Declare each runner cache and its cap from the [runner procedure](self-hosted-runners.md) when the project uses one.

## While working

Record in the work's checkpoint each residue it creates outside its worktree, with its class and location. Keep evidence that must outlive the work in its durable home, such as the pull request, the issue or the project's archive, not only in a local folder.

## When work closes

Once the work is integrated, or has another established end state under the [tracker procedure](issue-tracker.md), preserve the notes and evidence it still needs, then remove the residue it recorded along with its branches and worktrees.

## At each handoff or pause

Sweep the declared classes with the script installed with the `hivex-git` skill, from the project root:

```sh
python3 .agents/skills/hivex-git/scripts/residue.py
python3 .agents/skills/hivex-git/scripts/residue.py --trash
```

The script needs Python 3, run as `python` where `python3` is not available.

Without options it changes nothing: it reports each item with its size and status (`unneeded`, `needed`, `uncertain`, or `protected` for the filesystem root, the home folder, the project and any folder containing them). With `--trash` it moves only the `unneeded` items of `deterministic` classes to the Trash and reports how many items and bytes it moved. Give the responsible person the `unneeded` items of `review` classes to decide, and leave `needed`, `uncertain` and `protected` items alone. Never remove unfinished work, another session's work or anything uncertain. Report what was moved, what was listed and their sizes.

## Remove recoverably

Move files and folders to the Trash, never delete them permanently. The script uses the command in `HIVEX_TRASH` when set, otherwise `trash` (built into recent macOS) or `gio trash` on Linux, and refuses to move anything without one. Residue a tool manages, such as containers, images or simulators, is removed with that tool's own command for the declared items only; that removal is not recoverable, so list such classes as `review`.

The Trash frees no space until it is emptied. The owner empties it by hand: nothing in this procedure empties it or changes its settings.
