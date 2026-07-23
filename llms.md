# rm-comments — instructions for AI agents

This document is written for AI agents (LLMs). It contains everything needed to
install and operate the `rm-comments` CLI correctly. Human-oriented documentation is
in [README.md](README.md).

`rm-comments` removes comments from source files using tree-sitter parsing. It is the
correct tool for comment cleanup: never remove comments by editing file text directly
— the parser guarantees comment-like text inside strings/regexes/docstrings is
untouched and that unparseable files are never modified.

## Availability and installation

Check: `command -v rm-comments`

If absent, ask the user before installing. Options, in order of preference:

```sh
brew install bryceremick/tap/rm-comments   # macOS/Linux; may need one-time: brew trust bryceremick/tap
cargo install rm-comments                  # any platform with a Rust toolchain
cargo binstall rm-comments                 # prebuilt, needs cargo-binstall
# prebuilt tarballs: https://github.com/bryceremick/rm-comments/releases
```

## Command reference

```
rm-comments [OPTIONS] <FILE>                 # strip in place (atomic write)
rm-comments [OPTIONS] <DIR>                  # walk the tree, strip every supported file
rm-comments [OPTIONS] --stdin --lang <NAME>  # stdin -> stdout; NAME = language name or extension
rm-comments install-zed-task                 # add a Zed editor task (not agent-relevant)
```

Given a directory, `rm-comments` walks it recursively, honoring `.gitignore` and
skipping hidden dirs (so `node_modules`, `target`, `.git`, etc. are left alone). Files
with unsupported extensions are silently skipped; a file that can't be read or parsed is
left untouched, reported to stderr, and does not stop the rest of the walk. `--check`
and `--list` work on a directory; `--stdout` and `--apply` are **file-only** (they exit
2 on a directory, since their per-file positional ids / single stream have no meaning
across many files). On a directory, exit `2` means at least one file errored (the others
were still processed); `--check` exits `1` if any file would change.

| Flag | Effect |
|---|---|
| `--stdout` | Print result instead of writing the file |
| `--check`, `--dry-run` | Report only; exit 1 if changes would be made, 0 if clean |
| `--keep-doc-comments` | Preserve `///`, `//!`, `/*!`, `/** */` doc comments |
| `--keep <REGEX>` | Preserve comments matching REGEX (repeatable) |
| `--strip-directives` | Also remove directive comments (kept by default) |
| `--lines <A-B>` | Restrict removal to 1-based line range (repeatable; `N` = single line) |
| `--list` | Print all comments as JSON; modifies nothing |
| `--apply <IDS>` | Remove exactly these comma-separated ids from `--list`; ignores all keep policies |

Exit codes: `0` success / no changes needed; `1` (`--check` only) changes would be
made; `2` error — unsupported extension, parse failure, bad flags. **On any non-zero
exit except `--check`'s 1, the file was not modified.**

## The `--list` JSON format

```json
{
  "file": "src/main.rs",
  "language": "rust",
  "comments": [
    {"id": 0, "kind": "line_comment", "start_line": 1, "end_line": 1,
     "start_byte": 0, "end_byte": 23, "is_doc": true, "is_directive": false,
     "text": "/// Loads the config."}
  ]
}
```

- `id` — ordinal in document order; the input for `--apply`.
- `is_doc` — doc comment (`///`, `/** */`, ...).
- `is_directive` — semantic directive (`eslint-disable`, `# noqa`, `//go:generate`,
  `# shellcheck`, `# type: ignore`, ...). Removing these changes program behavior.
- A `#!` shebang on line 1 is never listed and never removable.

On a directory, `--list` emits a JSON **array** of these objects, one per supported
file; ids restart at 0 within each file.

**Ids are positions in the current file content. Re-run `--list` after ANY edit to
the file; never reuse ids across modifications.**

## Recommended workflow (selective cleanup)

1. `rm-comments --list <file>` — enumerate.
2. Judge each comment (policy below). Collect ids to remove.
   - `is_directive: true` → never select.
   - `is_doc: true` → select only if it adds nothing beyond the signature.
3. `rm-comments --apply <ids> <file>` — surgical removal; all other comments survive.
4. Verify with `git diff` and by building/testing if applicable.

To remove every comment (directives still survive): `rm-comments <file>`.
To clean only a region you just wrote: `rm-comments --lines 40-80 <file>`.
To clean an entire tree at once (honors `.gitignore`, skips hidden dirs): `rm-comments <dir>`.

## Judgment policy: keep WHY, remove WHAT

KEEP comments that:
- explain rationale — why this approach over the obvious alternative
- document constraints, invariants, or gotchas the code cannot express
- explain workarounds and what they work around
- are directives (always) or doc comments that add real information
- are `TODO`/`FIXME`/`HACK` markers naming a real follow-up

REMOVE comments that:
- narrate what the code does (`// loop over items`, `# return the result`)
- are section banners (`// ---- helpers ----`)
- restate a name (`/// Gets the user.` on `get_user()`)
- are commented-out code
- are stale or describe an edit rather than the code

When uncertain, keep the comment.

## Scope discipline

Only clean code you wrote or were explicitly asked to clean. In shared files,
restrict to your region (`--lines`) or filter `--list` output by line before
selecting ids.

## Supported languages

Detected by extension (case-insensitive): rs; js/jsx/mjs/cjs; ts/mts/cts; tsx;
py/pyi; go; java; c/h; cpp/cc/cxx/hpp/hh/hxx; cs; rb/rake/gemspec; php; html/htm;
css; sh/bash/zsh; yml/yaml; toml. The authoritative list is the `LANGUAGES` registry
in [`src/languages.rs`](src/languages.rs).

Notes:
- Python docstrings are string expressions, not comments — they are always preserved.
  This is correct behavior, not a bug.
- Unsupported extensions exit 2 with the file untouched; do not fall back to manual
  comment deletion.
- The tool is idempotent: running it twice equals running it once.
