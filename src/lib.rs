//! Strip comments from source code using tree-sitter.

pub mod languages;

use languages::Lang;
use tree_sitter::Parser;

/// What to remove. `Default` = remove every comment except semantic
/// directives (see [`DIRECTIVE_PREFIXES`]) and the line-1 shebang.
pub struct Options {
    /// Preserve doc comments (`///`, `//!`, `/*!`, `/** */`).
    pub keep_doc_comments: bool,
    /// Preserve directive comments (`eslint-disable`, `# noqa`, `//go:`, ...).
    /// On by default: removing these changes program behavior.
    pub keep_directives: bool,
    /// Preserve comments whose text matches any of these patterns.
    pub keep_patterns: Vec<regex::Regex>,
    /// 1-based inclusive line ranges; when non-empty, only comments
    /// intersecting a range are removed.
    pub lines: Vec<(usize, usize)>,
    /// When set, remove exactly these comment ids (as reported by
    /// [`list_comments`]) and ignore every policy above.
    pub only_ids: Option<Vec<usize>>,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            keep_doc_comments: false,
            keep_directives: true,
            keep_patterns: Vec::new(),
            lines: Vec::new(),
            only_ids: None,
        }
    }
}

/// One comment in a file, as enumerated by [`list_comments`]. `id` is the
/// comment's document-order ordinal — stable between `list_comments` and
/// `Options::only_ids` for the same file content. The line-1 shebang is
/// never enumerated (it is never removable).
pub struct Comment {
    pub id: usize,
    pub kind: String,
    /// Byte range in the source exactly as given (no line-ending normalization).
    pub start_byte: usize,
    pub end_byte: usize,
    /// 1-based lines.
    pub start_line: usize,
    pub end_line: usize,
    pub text: String,
    pub is_doc: bool,
    pub is_directive: bool,
}

/// Comment prefixes that carry semantics for other tools — removing them
/// changes behavior, so they are kept unless explicitly stripped. Matched
/// case-insensitively against the comment text after its `//`/`#`/`/*`/`<!--`
/// marker. Extend per-run with `Options::keep_patterns` (`--keep`).
pub const DIRECTIVE_PREFIXES: &[&str] = &[
    // JS/TS ecosystem
    "eslint-", "@ts-ignore", "@ts-expect-error", "@ts-nocheck", "@jsx",
    "prettier-ignore", "biome-ignore", "istanbul ignore", "webpack",
    "stylelint-",
    // Python
    "noqa", "type:", "mypy:", "ruff:", "pylint:", "pyright:", "flake8:",
    "fmt:", "isort:", "pragma", "-*-",
    // Go
    "go:", "nolint",
    // Shell
    "shellcheck",
    // Ruby
    "rubocop:", "frozen_string_literal:", "encoding:",
    // C/C++/C#/Java tooling
    "nolintnextline", "clang-format",
    // editors/misc
    "noinspection", "@formatter:", "spell-checker:", "cspell:",
];

/// The comment's text after its leading marker (`//`, `#`, `/*`, `<!--`, `;`).
fn comment_inner(s: &str) -> &str {
    let s = s.trim_start();
    for m in ["<!--", "/*", "//", "#", ";"] {
        if let Some(rest) = s.strip_prefix(m) {
            return rest;
        }
    }
    s
}

fn is_directive(text: &str) -> bool {
    let inner = comment_inner(text).trim_start().to_ascii_lowercase();
    DIRECTIVE_PREFIXES.iter().any(|p| inner.starts_with(p))
}

/// Doc-comment heuristic: Rust `///` `//!` `/*!`, and `/** ... */`
/// (JSDoc, Javadoc, Doxygen). Python docstrings are strings, not comments,
/// so they're never removed regardless.
fn is_doc_comment(s: &str) -> bool {
    s.starts_with("///")
        || s.starts_with("//!")
        || s.starts_with("/*!")
        || (s.starts_with("/**") && !s.starts_with("/**/"))
}

/// Parse `src` and collect comment node ranges in document order,
/// skipping the line-1 shebang. Errors if the file doesn't parse cleanly.
fn collect_comment_ranges(src: &str, lang: &Lang) -> Result<Vec<(usize, usize)>, String> {
    let mut parser = Parser::new();
    parser
        .set_language(&(lang.language)())
        .map_err(|e| format!("failed to load {} grammar: {e}", lang.name))?;
    let tree = parser
        .parse(src, None)
        .ok_or_else(|| format!("failed to parse as {}", lang.name))?;
    if tree.root_node().has_error() {
        return Err(format!(
            "file does not parse cleanly as {}; refusing to modify it",
            lang.name
        ));
    }

    let mut ranges = Vec::new();
    let mut cursor = tree.walk();
    let mut done = false;
    while !done {
        let node = cursor.node();
        if lang.comment_kinds.contains(&node.kind()) {
            let (start, end) = (node.start_byte(), node.end_byte());
            // The shebang is never a removal candidate, even if the grammar
            // tokenizes it as a comment.
            if !(start == 0 && src[start..end].starts_with("#!")) {
                ranges.push((start, end));
            }
        }
        if !cursor.goto_first_child() {
            while !cursor.goto_next_sibling() {
                if !cursor.goto_parent() {
                    done = true;
                    break;
                }
            }
        }
    }
    Ok(ranges)
}

/// Enumerate every comment in `src` (except the shebang), in document order.
/// Byte offsets refer to `src` exactly as passed — no normalization.
pub fn list_comments(src: &str, lang: &Lang) -> Result<Vec<Comment>, String> {
    let ranges = collect_comment_ranges(src, lang)?;
    let line_starts = line_starts(src);
    Ok(ranges
        .iter()
        .enumerate()
        .map(|(id, &(start, mut end))| {
            // some grammars (e.g. rust line_comment) include the trailing
            // newline in the node; report the comment without it
            while end > start && matches!(src.as_bytes()[end - 1], b'\n' | b'\r') {
                end -= 1;
            }
            let text = &src[start..end];
            Comment {
                id,
                kind: kind_of(src, lang, start, end),
                start_byte: start,
                end_byte: end,
                start_line: line_of(&line_starts, start),
                end_line: line_of(&line_starts, end.saturating_sub(1).max(start)),
                text: text.to_string(),
                is_doc: is_doc_comment(text),
                is_directive: is_directive(text),
            }
        })
        .collect())
}

// The node kind was already matched during collection; re-derive it cheaply
// from the registry rather than threading it through: single-kind languages
// have exactly one answer, multi-kind ones are distinguished by the text.
fn kind_of(src: &str, lang: &Lang, start: usize, end: usize) -> String {
    if lang.comment_kinds.len() == 1 {
        return lang.comment_kinds[0].to_string();
    }
    let text = &src[start..end];
    if lang.comment_kinds.contains(&"line_comment") {
        // rust/java style: line_comment vs block_comment
        if text.starts_with("//") { "line_comment" } else { "block_comment" }.to_string()
    } else if lang.comment_kinds.contains(&"js_comment") {
        // css style: comment vs js_comment (`//`)
        if text.starts_with("//") { "js_comment" } else { "comment" }.to_string()
    } else {
        // js/ts style: comment vs html_comment
        if text.starts_with("<!--") { "html_comment" } else { "comment" }.to_string()
    }
}

fn line_starts(src: &str) -> Vec<usize> {
    let mut v = vec![0];
    v.extend(src.bytes().enumerate().filter(|(_, b)| *b == b'\n').map(|(i, _)| i + 1));
    v
}

/// 1-based line containing byte `offset`.
fn line_of(line_starts: &[usize], offset: usize) -> usize {
    line_starts.partition_point(|&s| s <= offset)
}

/// Strip comments from `src` per `opts`. Errors (never panics) if the source
/// fails to parse or `only_ids` references an unknown id — callers must not
/// write anything to disk in that case.
pub fn strip_comments(src: &str, lang: &Lang, opts: &Options) -> Result<String, String> {
    // Normalize CRLF -> LF for processing; restored at the end.
    let crlf = src.contains("\r\n");
    let text = if crlf { src.replace("\r\n", "\n") } else { src.to_string() };
    let had_trailing_newline = text.ends_with('\n');

    let all = collect_comment_ranges(&text, lang)?;
    let line_starts = line_starts(&text);

    let ranges: Vec<(usize, usize)> = if let Some(ids) = &opts.only_ids {
        for &id in ids {
            if id >= all.len() {
                return Err(format!(
                    "unknown comment id {id} (file has {} comments, ids 0-{})",
                    all.len(),
                    all.len().saturating_sub(1)
                ));
            }
        }
        all.iter()
            .enumerate()
            .filter(|(id, _)| ids.contains(id))
            .map(|(_, &r)| r)
            .collect()
    } else {
        all.into_iter()
            .filter(|&(start, end)| {
                let ctext = &text[start..end];
                if opts.keep_doc_comments && is_doc_comment(ctext) {
                    return false;
                }
                if opts.keep_directives && is_directive(ctext) {
                    return false;
                }
                if opts.keep_patterns.iter().any(|re| re.is_match(ctext)) {
                    return false;
                }
                if !opts.lines.is_empty() {
                    let (first, last) = (
                        line_of(&line_starts, start),
                        line_of(&line_starts, end.saturating_sub(1).max(start)),
                    );
                    if !opts.lines.iter().any(|&(a, b)| first <= b && last >= a) {
                        return false;
                    }
                }
                true
            })
            .collect()
    };

    if ranges.is_empty() {
        return Ok(src.to_string()); // no-op: byte-for-byte identical
    }

    let mut mask = vec![false; text.len()];
    for (start, end) in &ranges {
        mask[*start..*end].fill(true);
    }

    let mut out = rebuild(&text, &mask);
    if had_trailing_newline && !out.is_empty() && !out.ends_with('\n') {
        out.push('\n');
    }
    if crlf {
        out = out.replace('\n', "\r\n");
    }
    Ok(out)
}

/// Whitespace policy — the single place removal cleanup lives:
///
/// - A line whose non-comment bytes are only whitespace is removed entirely
///   (covers full-line comments and lines swallowed by multi-line blocks).
/// - A line that keeps code has trailing whitespace trimmed (covers the gap
///   before a trailing comment).
/// - Around removed lines, runs of pre-existing blank lines collapse to at
///   most one. Blank lines nowhere near a removal are untouched.
fn rebuild(text: &str, mask: &[bool]) -> String {
    enum Line {
        Kept(String), // survives verbatim or trimmed
        Removed,      // dropped; participates in blank collapsing
    }

    let bytes = text.as_bytes();
    let mut lines: Vec<Line> = Vec::new();
    let mut start = 0;
    while start <= bytes.len() {
        let end = bytes[start..]
            .iter()
            .position(|&b| b == b'\n')
            .map(|p| start + p)
            .unwrap_or(bytes.len());
        if start == bytes.len() && lines.last().is_some() {
            break; // trailing newline: no phantom final line
        }
        let had_comment = mask[start..end].iter().any(|&m| m);
        if !had_comment {
            lines.push(Line::Kept(text[start..end].to_string()));
        } else {
            let kept: String = text[start..end]
                .char_indices()
                .filter(|(i, _)| !mask[start + i])
                .map(|(_, c)| c)
                .collect();
            if kept.trim().is_empty() {
                lines.push(Line::Removed);
            } else {
                lines.push(Line::Kept(kept.trim_end().to_string()));
            }
        }
        start = end + 1;
    }

    // Collapse: in any maximal run of [Removed | blank] lines containing at
    // least one Removed, emit at most one blank line (and only if the run
    // had blanks to begin with).
    let mut out: Vec<&str> = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        match &lines[i] {
            Line::Kept(s) if !s.trim().is_empty() => {
                out.push(s);
                i += 1;
            }
            _ => {
                // run of blanks and/or removed lines
                let mut blanks = 0;
                let mut removed = 0;
                let mut j = i;
                while j < lines.len() {
                    match &lines[j] {
                        Line::Removed => removed += 1,
                        Line::Kept(s) if s.trim().is_empty() => blanks += 1,
                        _ => break,
                    }
                    j += 1;
                }
                if removed > 0 {
                    // keep at most one blank, and none at the very top of the file
                    if blanks > 0 && !out.is_empty() {
                        out.push("");
                    }
                } else {
                    for _ in 0..blanks {
                        out.push("");
                    }
                }
                i = j;
            }
        }
    }

    out.join("\n")
}
