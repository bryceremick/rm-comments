use std::fs;
use std::path::Path;
use rm_comments::{languages, strip_comments, Options};

fn strip_doc() -> Options {
    Options { keep_doc_comments: false, ..Default::default() }
}

/// Every language in the registry has a fixture pair; stripping input
/// produces expected, and stripping twice equals stripping once.
#[test]
fn golden_fixtures() {
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    for lang in languages::LANGUAGES {
        let dir = fixtures.join(lang.name);
        let ext = lang.extensions[0];
        let input = fs::read_to_string(dir.join(format!("input.{ext}")))
            .unwrap_or_else(|_| panic!("missing fixture tests/fixtures/{}/input.{ext}", lang.name));
        let expected = fs::read_to_string(dir.join(format!("expected.{ext}")))
            .unwrap_or_else(|_| panic!("missing fixture tests/fixtures/{}/expected.{ext}", lang.name));

        let got = strip_comments(&input, lang, &Options::default()).unwrap();
        assert_eq!(got, expected, "golden mismatch for {}", lang.name);

        let again = strip_comments(&got, lang, &Options::default()).unwrap();
        assert_eq!(again, got, "not idempotent for {}", lang.name);
    }
}

fn rust() -> &'static languages::Lang {
    languages::by_name("rust").unwrap()
}

#[test]
fn doc_comments_kept_by_default() {
    let src = "//! inner doc\n/// outer doc\n// plain\nfn f() {}\n/** block doc */\nfn g() {}\n";
    // default keeps doc comments, removes only the plain one
    let out = strip_comments(src, rust(), &Options::default()).unwrap();
    assert_eq!(
        out,
        "//! inner doc\n/// outer doc\nfn f() {}\n/** block doc */\nfn g() {}\n"
    );
    // --strip-doc-comments removes them all
    let out = strip_comments(src, rust(), &strip_doc()).unwrap();
    assert_eq!(out, "fn f() {}\nfn g() {}\n");
}

#[test]
fn crlf_preserved() {
    let src = "// c\r\nfn main() {\r\n    let x = 1; // t\r\n}\r\n";
    let out = strip_comments(src, rust(), &Options::default()).unwrap();
    assert_eq!(out, "fn main() {\r\n    let x = 1;\r\n}\r\n");
}

#[test]
fn no_trailing_newline_preserved() {
    let src = "fn main() {} // c";
    let out = strip_comments(src, rust(), &Options::default()).unwrap();
    assert_eq!(out, "fn main() {}");
}

#[test]
fn empty_file() {
    assert_eq!(strip_comments("", rust(), &Options::default()).unwrap(), "");
}

#[test]
fn no_comments_is_byte_identical() {
    let src = "fn main() {\n\n\n    let x = 1;\n}\n"; // extra blanks stay
    assert_eq!(strip_comments(src, rust(), &Options::default()).unwrap(), src);
}

#[test]
fn comment_only_file_becomes_empty() {
    let src = "// a\n// b\n";
    assert_eq!(strip_comments(src, rust(), &Options::default()).unwrap(), "");
}

#[test]
fn shebang_survives() {
    let bash = languages::by_name("bash").unwrap();
    let src = "#!/bin/sh\n# comment\necho hi\n";
    assert_eq!(
        strip_comments(src, bash, &Options::default()).unwrap(),
        "#!/bin/sh\necho hi\n"
    );
}

#[test]
fn parse_error_refuses() {
    let src = "fn main( { this is not rust ][\n";
    assert!(strip_comments(src, rust(), &Options::default()).is_err());
}

#[test]
fn nested_block_comments() {
    let src = "/* outer /* inner */ still outer */\nfn main() {}\n";
    assert_eq!(strip_comments(src, rust(), &Options::default()).unwrap(), "fn main() {}\n");
}
