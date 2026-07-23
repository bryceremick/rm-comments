//! Tests for agent-oriented controls: directives, keep patterns, line
//! ranges, and the list/apply (enumerate -> decide -> remove) contract.

use rm_comments::{languages, list_comments, strip_comments, Options};

fn lang(name: &str) -> &'static languages::Lang {
    languages::by_name(name).unwrap()
}

fn strip_with(name: &str, src: &str, opts: &Options) -> String {
    strip_comments(src, lang(name), opts).unwrap()
}

#[test]
fn directives_survive_by_default() {
    let cases: &[(&str, &str)] = &[
        ("javascript", "// eslint-disable-next-line no-console\nconsole.log(1); // plain\n"),
        ("typescript", "// @ts-expect-error\nconst x: number = \"s\" as any; // plain\n"),
        ("python", "x = 1  # type: ignore\ny = 2  # plain\n"),
        ("python", "import os  # noqa: F401\n"),
        ("go", "//go:generate stringer\npackage main\n"),
        ("bash", "#!/bin/sh\n# shellcheck disable=SC2086\necho $x # plain\n"),
        ("ruby", "# frozen_string_literal: true\nx = 1 # plain\n"),
        ("javascript", "/* eslint-disable */\nlet a = 1;\n"),
    ];
    for (l, src) in cases {
        let out = strip_with(l, src, &Options::default());
        // the directive text must survive; any '// plain'-style comment must not
        assert!(
            !out.contains("plain"),
            "[{l}] plain comment survived:\n{out}"
        );
        let directive_line = src
            .lines()
            .find(|l| {
                l.contains("eslint") || l.contains("@ts-") || l.contains("type: ignore")
                    || l.contains("noqa") || l.contains("go:generate") || l.contains("shellcheck")
                    || l.contains("frozen_string_literal")
            })
            .unwrap();
        let marker = directive_line.trim_start_matches(['/', '*', '#', ' ']).split_whitespace().next().unwrap();
        assert!(
            out.contains(marker),
            "[{l}] directive '{marker}' was stripped:\n{out}"
        );
    }
}

#[test]
fn strip_directives_removes_them() {
    let src = "// eslint-disable-next-line\nconsole.log(1);\n";
    let opts = Options { keep_directives: false, ..Default::default() };
    assert_eq!(strip_with("javascript", src, &opts), "console.log(1);\n");
}

#[test]
fn directive_check_is_case_insensitive() {
    let src = "int x; // NOLINT(readability)\n";
    assert_eq!(strip_with("cpp", src, &Options::default()), src);
}

#[test]
fn directive_lookalike_in_plain_prose_is_not_kept() {
    // "the eslint config" does not START with a directive prefix -> removed
    let src = "// we updated the eslint config here\nlet a = 1;\n";
    assert_eq!(strip_with("javascript", src, &Options::default()), "let a = 1;\n");
}

#[test]
fn keep_pattern_preserves_matches() {
    let src = "// TODO: fix later\n// obvious narration\nfn main() {}\n";
    let opts = Options {
        keep_patterns: vec![regex::Regex::new("TODO|FIXME").unwrap()],
        ..Default::default()
    };
    assert_eq!(strip_with("rust", src, &opts), "// TODO: fix later\nfn main() {}\n");
}

#[test]
fn multiple_keep_patterns() {
    let src = "// SAFETY: aligned\n// TODO: later\n// noise\nfn main() {}\n";
    let opts = Options {
        keep_patterns: vec![
            regex::Regex::new("^// SAFETY").unwrap(),
            regex::Regex::new("TODO").unwrap(),
        ],
        ..Default::default()
    };
    assert_eq!(
        strip_with("rust", src, &opts),
        "// SAFETY: aligned\n// TODO: later\nfn main() {}\n"
    );
}

#[test]
fn lines_restricts_removal() {
    let src = "// one\nfn a() {}\n// two\nfn b() {}\n// three\n";
    // only lines 3-4: '// two' removed, '// one' and '// three' survive
    let opts = Options { lines: vec![(3, 4)], ..Default::default() };
    assert_eq!(
        strip_with("rust", src, &opts),
        "// one\nfn a() {}\nfn b() {}\n// three\n"
    );
}

#[test]
fn lines_multiple_ranges() {
    let src = "// one\nfn a() {}\n// two\nfn b() {}\n// three\n";
    let opts = Options { lines: vec![(1, 1), (5, 5)], ..Default::default() };
    assert_eq!(
        strip_with("rust", src, &opts),
        "fn a() {}\n// two\nfn b() {}\n"
    );
}

#[test]
fn lines_intersects_multiline_comments() {
    // block spans lines 1-3; a range touching only line 2 still removes it
    let src = "/* a\n   b\n   c */\nfn main() {}\n";
    let opts = Options { lines: vec![(2, 2)], ..Default::default() };
    assert_eq!(strip_with("rust", src, &opts), "fn main() {}\n");
}

#[test]
fn list_reports_ids_spans_and_flags() {
    let src = "/// doc\nfn main() {\n    let x = 1; // eslint-disable-line\n}\n// plain\n";
    let comments = list_comments(src, lang("rust")).unwrap();
    assert_eq!(comments.len(), 3);

    assert_eq!(comments[0].id, 0);
    assert_eq!(comments[0].text, "/// doc");
    assert!(comments[0].is_doc);
    assert!(!comments[0].is_directive);
    assert_eq!((comments[0].start_line, comments[0].end_line), (1, 1));
    assert_eq!(comments[0].kind, "line_comment");

    assert!(comments[1].is_directive);
    assert_eq!(comments[1].start_line, 3);

    assert_eq!(comments[2].text, "// plain");
    assert_eq!(comments[2].start_line, 5);
    // byte span round-trips to the text
    assert_eq!(&src[comments[2].start_byte..comments[2].end_byte], "// plain");
}

#[test]
fn list_excludes_shebang() {
    let src = "#!/usr/bin/env python3\n# real comment\nx = 1\n";
    let comments = list_comments(src, lang("python")).unwrap();
    assert_eq!(comments.len(), 1);
    assert_eq!(comments[0].text, "# real comment");
    assert_eq!(comments[0].id, 0);
}

#[test]
fn list_multiline_block_line_span() {
    let src = "fn a() {}\n/* one\n   two */\nfn b() {}\n";
    let comments = list_comments(src, lang("rust")).unwrap();
    assert_eq!(comments.len(), 1);
    assert_eq!((comments[0].start_line, comments[0].end_line), (2, 3));
    assert_eq!(comments[0].kind, "block_comment");
}

#[test]
fn list_refuses_parse_errors() {
    assert!(list_comments("fn ][ nope", lang("rust")).is_err());
}

#[test]
fn only_ids_removes_exactly_those() {
    let src = "// zero\n// one\n// two\nfn main() {}\n";
    let opts = Options { only_ids: Some(vec![0, 2]), ..Default::default() };
    assert_eq!(strip_with("rust", src, &opts), "// one\nfn main() {}\n");
}

#[test]
fn only_ids_ignores_keep_policies() {
    // the agent's explicit decision wins: even a directive goes if selected
    let src = "// eslint-disable-next-line\nlet a = 1;\n";
    let opts = Options { only_ids: Some(vec![0]), ..Default::default() };
    assert_eq!(strip_with("javascript", src, &opts), "let a = 1;\n");
}

#[test]
fn only_ids_unknown_id_errors_and_writes_nothing() {
    let src = "// a\nfn main() {}\n";
    let opts = Options { only_ids: Some(vec![5]), ..Default::default() };
    let err = strip_comments(src, lang("rust"), &opts).unwrap_err();
    assert!(err.contains("unknown comment id 5"), "got: {err}");
}

#[test]
fn only_ids_empty_is_a_noop() {
    let src = "// a\nfn main() {}\n";
    let opts = Options { only_ids: Some(vec![]), ..Default::default() };
    assert_eq!(strip_with("rust", src, &opts), src);
}

#[test]
fn list_then_apply_roundtrip() {
    // the canonical agent workflow: list, pick non-doc non-directive ids, apply
    let src = "/// keep me (doc)\nfn f() {}\n// drop me\nlet x = 1; // type: ignore\n";
    let src = &format!("fn main() {{\n{src}\n}}\n"); // make it valid rust
    let comments = list_comments(src, lang("rust")).unwrap();
    let ids: Vec<usize> = comments
        .iter()
        .filter(|c| !c.is_doc && !c.is_directive)
        .map(|c| c.id)
        .collect();
    let opts = Options { only_ids: Some(ids), ..Default::default() };
    let out = strip_with("rust", src, &opts);
    assert!(out.contains("/// keep me"));
    assert!(out.contains("// type: ignore"));
    assert!(!out.contains("drop me"));
}
