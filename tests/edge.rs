//! Edge cases for the stripping logic (library level).

use rm_comments::{languages, strip_comments, Options};

fn keep_doc() -> Options {
    Options { keep_doc_comments: true, ..Default::default() }
}

fn lang(name: &str) -> &'static languages::Lang {
    languages::by_name(name).unwrap()
}

fn strip(name: &str, src: &str) -> String {
    strip_comments(src, lang(name), &Options::default()).unwrap()
}

#[test]
fn js_regex_with_slashes_preserved() {
    let src = "const re = /https:\\/\\/x/; // go\n";
    assert_eq!(strip("javascript", src), "const re = /https:\\/\\/x/;\n");
}

#[test]
fn python_fstring_hash_preserved() {
    let src = "n = 1\nx = f\"count {n} #tag\"  # trailing\n";
    assert_eq!(strip("python", src), "n = 1\nx = f\"count {n} #tag\"\n");
}

#[test]
fn yaml_block_scalar_hash_preserved() {
    let src = "text: |\n  # not a comment\nkey: 1\n";
    assert_eq!(strip("yaml", src), src);
}

#[test]
fn multibyte_chars_before_comment() {
    let src = "fn main() {\n    let s = \"héllo wörld 🦀\"; // comment\n}\n";
    assert_eq!(
        strip("rust", src),
        "fn main() {\n    let s = \"héllo wörld 🦀\";\n}\n"
    );
}

#[test]
fn multibyte_chars_inside_comment() {
    let src = "fn main() {} // héllo 🦀 comment\n";
    assert_eq!(strip("rust", src), "fn main() {}\n");
}

#[test]
fn inline_block_comment_keeps_surrounding_spaces() {
    // only trailing whitespace is trimmed; interior gaps are not comment bytes
    let src = "fn main() { let a = 1 /* c */ ; }\n";
    assert_eq!(strip("rust", src), "fn main() { let a = 1  ; }\n");
}

#[test]
fn comment_between_code_lines_leaves_no_blank() {
    let src = "fn a() {}\n// c\nfn b() {}\n";
    assert_eq!(strip("rust", src), "fn a() {}\nfn b() {}\n");
}

#[test]
fn comment_between_blanks_collapses_to_one() {
    let src = "fn a() {}\n\n// c\n\nfn b() {}\n";
    assert_eq!(strip("rust", src), "fn a() {}\n\nfn b() {}\n");
}

#[test]
fn blanks_far_from_comments_untouched() {
    let src = "fn a() {}\n\n\nfn b() {}\n// c\n";
    assert_eq!(strip("rust", src), "fn a() {}\n\n\nfn b() {}\n");
}

#[test]
fn trailing_whitespace_on_untouched_lines_preserved() {
    let src = "fn main() {   \n}\n// c\n";
    assert_eq!(strip("rust", src), "fn main() {   \n}\n");
}

#[test]
fn leading_comment_block_leaves_no_blank_at_top() {
    let src = "// a\n// b\n\nfn main() {}\n";
    assert_eq!(strip("rust", src), "fn main() {}\n");
}

#[test]
fn multiline_block_swallows_lines() {
    let src = "fn a() {}\n/* one\n   two\n   three */\nfn b() {}\n";
    assert_eq!(strip("rust", src), "fn a() {}\nfn b() {}\n");
}

#[test]
fn block_comment_with_code_on_both_ends() {
    // both lines keep their code; the comment span (incl. its newline) is
    // masked but lines with surviving code are never merged
    let src = "fn main() { /* one\n   two */ let x = 1; }\n";
    assert_eq!(strip("rust", src), "fn main() {\n let x = 1; }\n");
}

#[test]
fn crlf_multiline_block() {
    let src = "/* a\r\n   b */\r\nfn main() {\r\n}\r\n";
    assert_eq!(strip("rust", src), "fn main() {\r\n}\r\n");
}

#[test]
fn crlf_no_comments_is_byte_identical() {
    let src = "fn main() {\r\n    let x = 1;\r\n}\r\n";
    assert_eq!(strip("rust", src), src);
}

#[test]
fn comment_only_no_trailing_newline() {
    assert_eq!(strip("rust", "// only comment"), "");
}

#[test]
fn whitespace_only_file_untouched() {
    let src = "\n\n   \n";
    assert_eq!(strip("rust", src), src);
}

#[test]
fn shebang_only_file_untouched() {
    let src = "#!/bin/sh\n";
    assert_eq!(strip("bash", src), src);
}

#[test]
fn shebang_then_comments_only() {
    let src = "#!/bin/sh\n# a\n# b\n";
    assert_eq!(strip("bash", src), "#!/bin/sh\n");
}

#[test]
fn keep_doc_jsdoc() {
    let src = "/** jsdoc */\nfunction f() {}\n// plain\nfunction g() {}\n";
    let out = strip_comments(src, lang("javascript"), &keep_doc()).unwrap();
    assert_eq!(out, "/** jsdoc */\nfunction f() {}\nfunction g() {}\n");
}

#[test]
fn keep_doc_csharp_triple_slash() {
    let src = "/// <summary>doc</summary>\nclass A {\n    // plain\n}\n";
    let out = strip_comments(src, lang("csharp"), &keep_doc()).unwrap();
    assert_eq!(out, "/// <summary>doc</summary>\nclass A {\n}\n");
}

#[test]
fn empty_block_comment_is_not_doc() {
    // `/**/` must not be mistaken for a `/**` doc comment
    let src = "/**/\nfn main() {}\n";
    let out = strip_comments(src, lang("rust"), &keep_doc()).unwrap();
    assert_eq!(out, "fn main() {}\n");
}

#[test]
fn extension_lookup_is_case_insensitive() {
    assert!(languages::by_extension("RS").is_some());
    assert!(languages::by_extension("Py").is_some());
    assert!(languages::by_extension("nope").is_none());
}

#[test]
fn by_name_accepts_name_or_extension() {
    assert!(languages::by_name("rust").is_some());
    assert!(languages::by_name("rs").is_some());
    assert!(languages::by_name("TypeScript").is_some());
    assert!(languages::by_name("klingon").is_none());
}

#[test]
fn extensions_are_unique_across_registry() {
    let mut seen = std::collections::HashSet::new();
    for l in languages::LANGUAGES {
        for e in l.extensions {
            assert!(seen.insert(*e), "extension '{e}' registered twice");
        }
    }
}
