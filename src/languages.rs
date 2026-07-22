//! Language registry: file extension -> tree-sitter grammar + comment node kinds.
//!
//! To add a language:
//!   1. `cargo add tree-sitter-<lang>`
//!   2. Add one entry to LANGUAGES below:
//!
//!      Lang {
//!          name: "kotlin",
//!          extensions: &["kt", "kts"],
//!          language: || tree_sitter_kotlin::LANGUAGE.into(),
//!          comment_kinds: &["comment"], // check the grammar's node-types.json
//!      },
//!
//!   3. Add a fixture pair under tests/fixtures/<name>/ (input.<ext> + expected.<ext>).
//!
//! Comment node kinds differ per grammar (verified against each crate's
//! node-types.json): most use "comment", but Rust and Java use
//! "line_comment"/"block_comment", JS/TS also have legacy "html_comment",
//! and CSS has "js_comment" for `//` comments.

use tree_sitter::Language;

pub struct Lang {
    pub name: &'static str,
    pub extensions: &'static [&'static str],
    pub language: fn() -> Language,
    pub comment_kinds: &'static [&'static str],
}

pub static LANGUAGES: &[Lang] = &[
    Lang {
        name: "rust",
        extensions: &["rs"],
        language: || tree_sitter_rust::LANGUAGE.into(),
        comment_kinds: &["line_comment", "block_comment"],
    },
    Lang {
        name: "javascript",
        extensions: &["js", "jsx", "mjs", "cjs"],
        language: || tree_sitter_javascript::LANGUAGE.into(),
        comment_kinds: &["comment", "html_comment"],
    },
    Lang {
        name: "typescript",
        extensions: &["ts", "mts", "cts"],
        language: || tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
        comment_kinds: &["comment", "html_comment"],
    },
    Lang {
        name: "tsx",
        extensions: &["tsx"],
        language: || tree_sitter_typescript::LANGUAGE_TSX.into(),
        comment_kinds: &["comment", "html_comment"],
    },
    Lang {
        name: "python",
        extensions: &["py", "pyi"],
        language: || tree_sitter_python::LANGUAGE.into(),
        comment_kinds: &["comment"],
    },
    Lang {
        name: "go",
        extensions: &["go"],
        language: || tree_sitter_go::LANGUAGE.into(),
        comment_kinds: &["comment"],
    },
    Lang {
        name: "java",
        extensions: &["java"],
        language: || tree_sitter_java::LANGUAGE.into(),
        comment_kinds: &["line_comment", "block_comment"],
    },
    Lang {
        name: "c",
        extensions: &["c", "h"],
        language: || tree_sitter_c::LANGUAGE.into(),
        comment_kinds: &["comment"],
    },
    Lang {
        name: "cpp",
        extensions: &["cpp", "cc", "cxx", "hpp", "hh", "hxx"],
        language: || tree_sitter_cpp::LANGUAGE.into(),
        comment_kinds: &["comment"],
    },
    Lang {
        name: "csharp",
        extensions: &["cs"],
        language: || tree_sitter_c_sharp::LANGUAGE.into(),
        comment_kinds: &["comment"],
    },
    Lang {
        name: "ruby",
        extensions: &["rb", "rake", "gemspec"],
        language: || tree_sitter_ruby::LANGUAGE.into(),
        comment_kinds: &["comment"],
    },
    Lang {
        name: "php",
        extensions: &["php"],
        language: || tree_sitter_php::LANGUAGE_PHP.into(),
        comment_kinds: &["comment"],
    },
    Lang {
        name: "html",
        extensions: &["html", "htm"],
        language: || tree_sitter_html::LANGUAGE.into(),
        comment_kinds: &["comment"],
    },
    Lang {
        name: "css",
        extensions: &["css"],
        language: || tree_sitter_css::LANGUAGE.into(),
        comment_kinds: &["comment", "js_comment"],
    },
    Lang {
        name: "bash",
        extensions: &["sh", "bash", "zsh"],
        language: || tree_sitter_bash::LANGUAGE.into(),
        comment_kinds: &["comment"],
    },
    Lang {
        name: "yaml",
        extensions: &["yml", "yaml"],
        language: || tree_sitter_yaml::LANGUAGE.into(),
        comment_kinds: &["comment"],
    },
    Lang {
        name: "toml",
        extensions: &["toml"],
        language: || tree_sitter_toml_ng::LANGUAGE.into(),
        comment_kinds: &["comment"],
    },
];

/// Look up a language by file extension (case-insensitive).
pub fn by_extension(ext: &str) -> Option<&'static Lang> {
    let ext = ext.to_ascii_lowercase();
    LANGUAGES
        .iter()
        .find(|l| l.extensions.contains(&ext.as_str()))
}

/// Look up a language by name or extension (for --lang).
pub fn by_name(name: &str) -> Option<&'static Lang> {
    let name = name.to_ascii_lowercase();
    LANGUAGES
        .iter()
        .find(|l| l.name == name || l.extensions.contains(&name.as_str()))
}
