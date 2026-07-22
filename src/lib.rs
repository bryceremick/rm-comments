//! Strip comments from source code using tree-sitter.

pub mod languages;

use languages::Lang;
use tree_sitter::Parser;

/// Strip all comments from `src`, parsed as `lang`.
///
/// Errors (returned, never panics) if the source fails to parse — callers
/// must not write anything to disk in that case.
pub fn strip_comments(src: &str, lang: &Lang, keep_doc_comments: bool) -> Result<String, String> {
    // Normalize CRLF -> LF for processing; restored at the end.
    let crlf = src.contains("\r\n");
    let text = if crlf { src.replace("\r\n", "\n") } else { src.to_string() };
    let had_trailing_newline = text.ends_with('\n');

    let mut parser = Parser::new();
    parser
        .set_language(&(lang.language)())
        .map_err(|e| format!("failed to load {} grammar: {e}", lang.name))?;
    let tree = parser
        .parse(&text, None)
        .ok_or_else(|| format!("failed to parse as {}", lang.name))?;
    if tree.root_node().has_error() {
        return Err(format!(
            "file does not parse cleanly as {}; refusing to modify it",
            lang.name
        ));
    }

    // Collect byte ranges of comment nodes.
    let mut ranges: Vec<(usize, usize)> = Vec::new();
    let mut cursor = tree.walk();
    let mut done = false;
    while !done {
        let node = cursor.node();
        if lang.comment_kinds.contains(&node.kind()) {
            let (start, end) = (node.start_byte(), node.end_byte());
            let comment_text = &text[start..end];
            // Preserve a shebang on line 1 even if the grammar calls it a comment.
            let is_shebang = start == 0 && comment_text.starts_with("#!");
            let keep = is_shebang || (keep_doc_comments && is_doc_comment(comment_text));
            if !keep {
                ranges.push((start, end));
            }
        }
        // Depth-first traversal.
        if !cursor.goto_first_child() {
            while !cursor.goto_next_sibling() {
                if !cursor.goto_parent() {
                    done = true;
                    break;
                }
            }
        }
    }

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

/// Doc-comment heuristic for `--keep-doc-comments`: Rust `///` `//!` `/*!`,
/// and `/** ... */` (JSDoc, Javadoc, Doxygen). Python docstrings are strings,
/// not comments, so they're never removed regardless of this flag.
fn is_doc_comment(s: &str) -> bool {
    s.starts_with("///")
        || s.starts_with("//!")
        || s.starts_with("/*!")
        || (s.starts_with("/**") && !s.starts_with("/**/"))
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
