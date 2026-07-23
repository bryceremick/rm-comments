---
name: rm-comments
description: Remove unnecessary comments from source code using the rm-comments CLI. Use after writing or editing code, when asked to "clean up comments", "remove comments", "strip comments", or "de-noise" a file, or when reviewing code that is over-commented. Keeps comments that explain WHY; removes comments that narrate WHAT.
---

# rm-comments: remove unnecessary comments

Clean a file's comments with the `rm-comments` CLI.
Never delete comments by hand-editing the file — the tool parses with tree-sitter, so it
cannot touch comment-like text inside strings and cannot corrupt a file that doesn't parse.

If `rm-comments` is not on PATH, offer to install it first (any of):
`brew install bryceremick/tap/rm-comments`, `cargo install rm-comments`,
`cargo binstall rm-comments`, or a prebuilt binary from
https://github.com/bryceremick/rm-comments/releases.

## The policy: WHY stays, WHAT goes

Code must be expressive enough to answer *"what is this doing?"* on its own. A comment
earns its place only by answering *"why is it doing that?"* — and only when the answer
is not obvious from the code.

**KEEP** a comment if it:
- Explains **rationale**: why this approach, why not the obvious alternative
  (`// binary search is slower here: lists are ~3 elements`)
- Documents a **constraint, gotcha, or invariant** the code can't express
  (`# API silently truncates batches over 100`, `// SAFETY: caller guarantees non-null`)
- Explains a **workaround** and what it works around (`// retry: S3 eventual consistency`)
- Is a **directive** (`is_directive: true` — `eslint-disable`, `# noqa`, `//go:`,
  `# type: ignore`...). **Never remove these; they change program behavior.**
- Is a **doc comment on a public API** (`is_doc: true`) that adds information beyond the
  signature. Remove doc comments that merely restate the name
  (`/// Gets the user.` on `get_user()` → remove).
- Is a **task marker** (`is_marker: true` — `TODO`/`FIXME`/`HACK`/`XXX`/`BUG`) naming a
  real follow-up. **Kept by default; never remove these.**

**REMOVE** a comment if it:
- **Narrates what the code does**: `// loop over the items`, `// increment counter`,
  `# return the result`, `// call the API` — the code already says this
- Is a **section banner**: `// ---- helpers ----`, `// Imports`
- **Restates the name**: `// Constructor` above a constructor, `/// Gets X` on `get_x`
- Is **commented-out code** (dead code lives in git history, not in files)
- Is stale, describes an edit rather than the code ("// updated to use v2"), or is
  filler an LLM added out of habit

When unsure, lean toward **keeping** — a wrongly kept comment costs a line; a wrongly
removed WHY costs the next reader an investigation.

## Workflow

1. **Enumerate** — never guess what's in the file:

   ```sh
   rm-comments --list path/to/file.rs
   ```

   Returns JSON: every comment with `id`, `start_line`/`end_line`, `text`, `is_doc`,
   `is_directive`, `is_marker`.

2. **Judge** each comment against the policy above. Collect the ids to remove.
   - `is_directive: true` → never select it.
   - `is_marker: true` → never select it (real follow-up work).
   - `is_doc: true` → select only if it adds nothing beyond the signature.

3. **Apply** exactly those ids (removal is surgical; all other comments survive):

   ```sh
   rm-comments --apply 2,5,7 path/to/file.rs
   ```

   For a bulk pass, `rm-comments path/to/file.rs` removes plain narration comments in one
   step — doc comments, directives, and task markers are **kept by default** (add
   `--strip-doc-comments` / `--strip-directives` / `--strip-markers` to remove those too;
   all three removes every comment). To clean a whole tree, pass a directory:
   `rm-comments src/` walks it recursively (honoring `.gitignore`, skipping hidden dirs),
   cleaning every supported file. `--stdout` and `--apply` are file-only; `--list`/`--check`
   accept a directory (`--list` then emits a JSON array, one object per file).

4. **Verify** — check the diff (`git diff path/to/file.rs`) and confirm the build/tests
   still pass. If `rm-comments` exits with an error, the file was not modified; fix the
   parse problem first, do not fall back to hand-deleting comments.

## Scoping rules

- **Only clean code you wrote or were asked to clean.** After editing lines 40–80 of a
  shared file, restrict to your region instead of judging the whole file:
  `rm-comments --lines 40-80 file.rs` (policy mode) or filter the `--list` output by
  `start_line`/`end_line` before choosing ids.
- Ids are positions in the current file content — **re-run `--list` after any edit**;
  never reuse ids across file modifications.
- Unsupported file type or parse failure → exit code 2, file untouched. Report it and
  move on; do not strip such files manually.
