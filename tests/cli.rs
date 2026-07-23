//! Integration tests for the actual binary: flags, exit codes, and the
//! never-corrupt-the-file guarantees.

use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};

const BIN: &str = env!("CARGO_BIN_EXE_rm-comments");

static COUNTER: AtomicUsize = AtomicUsize::new(0);

/// Fresh temp dir per test file usage (unique per pid + counter).
fn tmpfile(name: &str, contents: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "sc-cli-{}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::SeqCst)
    ));
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join(name);
    fs::write(&path, contents).unwrap();
    path
}

fn run(args: &[&str]) -> Output {
    Command::new(BIN).args(args).output().unwrap()
}

fn run_stdin(args: &[&str], input: &str) -> Output {
    let mut child = Command::new(BIN)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    // The child may exit before consuming stdin (e.g. an invalid --lang exits 2
    // before reading), which races the write into a BrokenPipe. That's expected
    // here — the test asserts on exit code/output — so only a real write error fails.
    let mut stdin = child.stdin.take().unwrap();
    match stdin.write_all(input.as_bytes()) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::BrokenPipe => {}
        Err(e) => panic!("writing to child stdin: {e}"),
    }
    drop(stdin);
    child.wait_with_output().unwrap()
}

const DIRTY: &str = "// comment\nfn main() {} // trailing\n";
const CLEAN: &str = "fn main() {}\n";

#[test]
fn in_place_strips() {
    let f = tmpfile("a.rs", DIRTY);
    let out = run(&[f.to_str().unwrap()]);
    assert!(out.status.success(), "stderr: {:?}", out.stderr);
    assert_eq!(fs::read_to_string(&f).unwrap(), CLEAN);
}

#[test]
fn stdout_flag_prints_without_modifying() {
    let f = tmpfile("a.rs", DIRTY);
    let out = run(&["--stdout", f.to_str().unwrap()]);
    assert!(out.status.success());
    assert_eq!(String::from_utf8(out.stdout).unwrap(), CLEAN);
    assert_eq!(fs::read_to_string(&f).unwrap(), DIRTY, "file was modified");
}

#[test]
fn check_dirty_exits_1_without_modifying() {
    let f = tmpfile("a.rs", DIRTY);
    let out = run(&["--check", f.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(1));
    assert_eq!(fs::read_to_string(&f).unwrap(), DIRTY, "file was modified");
}

#[test]
fn check_clean_exits_0() {
    let f = tmpfile("a.rs", CLEAN);
    let out = run(&["--check", f.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(0));
}

#[test]
fn version_flag_prints_version() {
    let out = run(&["--version"]);
    assert!(out.status.success());
    let expected = format!("rm-comments {}\n", env!("CARGO_PKG_VERSION"));
    assert_eq!(String::from_utf8(out.stdout).unwrap(), expected);
}

#[test]
fn dry_run_is_alias_for_check() {
    let f = tmpfile("a.rs", DIRTY);
    let out = run(&["--dry-run", f.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(1));
    assert_eq!(fs::read_to_string(&f).unwrap(), DIRTY);
}

#[test]
fn stdin_roundtrip() {
    let out = run_stdin(&["--stdin", "--lang", "rust"], DIRTY);
    assert!(out.status.success());
    assert_eq!(String::from_utf8(out.stdout).unwrap(), CLEAN);
}

#[test]
fn stdin_accepts_extension_as_lang() {
    let out = run_stdin(&["--stdin", "--lang", "py"], "# c\nx = 1\n");
    assert!(out.status.success());
    assert_eq!(String::from_utf8(out.stdout).unwrap(), "x = 1\n");
}

#[test]
fn stdin_check_exit_codes() {
    let out = run_stdin(&["--stdin", "--lang", "rust", "--check"], DIRTY);
    assert_eq!(out.status.code(), Some(1));
    let out = run_stdin(&["--stdin", "--lang", "rust", "--check"], CLEAN);
    assert_eq!(out.status.code(), Some(0));
}

#[test]
fn doc_comments_kept_by_default() {
    let f = tmpfile("a.rs", "/// doc\n// plain\nfn main() {}\n");
    let out = run(&[f.to_str().unwrap()]);
    assert!(out.status.success(), "stderr: {:?}", out.stderr);
    assert_eq!(fs::read_to_string(&f).unwrap(), "/// doc\nfn main() {}\n");
}

#[test]
fn strip_doc_comments_flag_removes_them() {
    let f = tmpfile("a.rs", "/// doc\n// plain\nfn main() {}\n");
    let out = run(&["--strip-doc-comments", f.to_str().unwrap()]);
    assert!(out.status.success(), "stderr: {:?}", out.stderr);
    assert_eq!(fs::read_to_string(&f).unwrap(), "fn main() {}\n");
}

#[test]
fn removed_keep_doc_comments_flag_exits_2_untouched() {
    // the old --keep-doc-comments flag is gone; it's now just an unknown flag
    let src = "// plain\nfn main() {}\n";
    let f = tmpfile("a.rs", src);
    let out = run(&["--keep-doc-comments", f.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(2));
    assert_eq!(fs::read_to_string(&f).unwrap(), src, "file touched on unknown flag");
}

#[test]
fn markers_kept_by_default() {
    let src = "// TODO: later\n// FIXME: bug\n// HACK: x\n// XXX\n// BUG: y\n// narration\nfn main() {}\n";
    let f = tmpfile("a.rs", src);
    let out = run(&[f.to_str().unwrap()]);
    assert!(out.status.success(), "stderr: {:?}", out.stderr);
    assert_eq!(
        fs::read_to_string(&f).unwrap(),
        "// TODO: later\n// FIXME: bug\n// HACK: x\n// XXX\n// BUG: y\nfn main() {}\n"
    );
}

#[test]
fn strip_markers_flag_removes_them() {
    let f = tmpfile("a.rs", "// TODO: later\n// narration\nfn main() {}\n");
    let out = run(&["--strip-markers", f.to_str().unwrap()]);
    assert!(out.status.success(), "stderr: {:?}", out.stderr);
    assert_eq!(fs::read_to_string(&f).unwrap(), "fn main() {}\n");
}

#[test]
fn marker_word_boundary_not_matched_in_prose() {
    // starts with a marker substring but not as a whole token -> removed as narration
    let f = tmpfile("a.rs", "// todos are hard\n// buggy path\nfn main() {}\n");
    let out = run(&[f.to_str().unwrap()]);
    assert!(out.status.success(), "stderr: {:?}", out.stderr);
    assert_eq!(fs::read_to_string(&f).unwrap(), "fn main() {}\n");
}

#[test]
fn list_reports_is_marker() {
    let f = tmpfile("a.rs", "// TODO: x\n// plain\nfn main() {}\n");
    let out = run(&["--list", f.to_str().unwrap()]);
    assert!(out.status.success(), "stderr: {:?}", out.stderr);
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).expect("invalid JSON");
    let c = v["comments"].as_array().unwrap();
    assert_eq!(c[0]["is_marker"], true);
    assert_eq!(c[1]["is_marker"], false);
}

// --- refusal paths: the file must NEVER be touched ---

#[test]
fn unknown_extension_exits_2_untouched() {
    let f = tmpfile("a.xyz", "// whatever\n");
    let out = run(&[f.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(2));
    assert!(!out.stderr.is_empty());
    assert_eq!(fs::read_to_string(&f).unwrap(), "// whatever\n");
}

#[test]
fn no_extension_exits_2_untouched() {
    let f = tmpfile("Makefile", "# c\nall:\n");
    let out = run(&[f.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(2));
    assert_eq!(fs::read_to_string(&f).unwrap(), "# c\nall:\n");
}

#[test]
fn parse_error_exits_2_untouched() {
    let src = "// comment\nfn main( { ][ not rust\n";
    let f = tmpfile("bad.rs", src);
    let out = run(&[f.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("refusing"));
    assert_eq!(fs::read_to_string(&f).unwrap(), src, "corrupted on parse error");
}

#[test]
fn non_utf8_exits_2_untouched() {
    let dir = std::env::temp_dir().join(format!("sc-cli-{}-utf8", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let f = dir.join("bin.rs");
    fs::write(&f, [0xff, 0xfe, b'/', b'/', b'x']).unwrap();
    let out = run(&[f.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(2));
    assert_eq!(fs::read(&f).unwrap(), [0xff, 0xfe, b'/', b'/', b'x']);
}

#[test]
fn missing_file_exits_2() {
    let out = run(&["/nonexistent/nope.rs"]);
    assert_eq!(out.status.code(), Some(2));
}

#[test]
fn unknown_lang_exits_2() {
    let out = run_stdin(&["--stdin", "--lang", "klingon"], "x\n");
    assert_eq!(out.status.code(), Some(2));
}

#[test]
fn unknown_flag_exits_2() {
    let out = run(&["--frobnicate", "a.rs"]);
    assert_eq!(out.status.code(), Some(2));
}

#[test]
fn help_exits_0() {
    for arg in ["--help", "-h", "help"] {
        let out = run(&[arg]);
        assert_eq!(out.status.code(), Some(0), "{arg} failed");
        assert!(String::from_utf8_lossy(&out.stdout).contains("Usage"));
    }
}

#[test]
fn no_temp_file_left_behind() {
    let f = tmpfile("a.rs", DIRTY);
    run(&[f.to_str().unwrap()]);
    let leftovers: Vec<_> = fs::read_dir(f.parent().unwrap())
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .filter(|n| n.to_string_lossy().contains("sc-tmp"))
        .collect();
    assert!(leftovers.is_empty(), "temp files left: {leftovers:?}");
}

#[cfg(unix)]
#[test]
fn permissions_preserved() {
    use std::os::unix::fs::PermissionsExt;
    let f = tmpfile("a.rs", DIRTY);
    fs::set_permissions(&f, fs::Permissions::from_mode(0o754)).unwrap();
    let out = run(&[f.to_str().unwrap()]);
    assert!(out.status.success());
    let mode = fs::metadata(&f).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o754);
}

#[test]
fn keep_flag_preserves_matches() {
    // PERF isn't a built-in marker, so this isolates --keep behavior
    let f = tmpfile("a.rs", "// PERF: hot\n// noise\nfn main() {}\n");
    let out = run(&["--keep", "PERF", f.to_str().unwrap()]);
    assert!(out.status.success(), "stderr: {:?}", out.stderr);
    assert_eq!(fs::read_to_string(&f).unwrap(), "// PERF: hot\nfn main() {}\n");
}

#[test]
fn invalid_keep_regex_exits_2() {
    let out = run(&["--keep", "(unclosed", "a.rs"]);
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("invalid regex"));
}

#[test]
fn directives_kept_by_default_stripped_on_request() {
    let src = "// eslint-disable-next-line\nconsole.log(1);\n";
    let f = tmpfile("a.js", src);
    assert!(run(&[f.to_str().unwrap()]).status.success());
    assert_eq!(fs::read_to_string(&f).unwrap(), src, "directive was stripped by default");
    assert!(run(&["--strip-directives", f.to_str().unwrap()]).status.success());
    assert_eq!(fs::read_to_string(&f).unwrap(), "console.log(1);\n");
}

#[test]
fn lines_flag_limits_scope() {
    let f = tmpfile("a.rs", "// one\nfn a() {}\n// two\nfn b() {}\n");
    let out = run(&["--lines", "3-4", f.to_str().unwrap()]);
    assert!(out.status.success(), "stderr: {:?}", out.stderr);
    assert_eq!(fs::read_to_string(&f).unwrap(), "// one\nfn a() {}\nfn b() {}\n");
}

#[test]
fn lines_flag_single_number_and_repeat() {
    let f = tmpfile("a.rs", "// one\nfn a() {}\n// two\nfn b() {}\n// three\n");
    let out = run(&["--lines", "1", "--lines", "5", f.to_str().unwrap()]);
    assert!(out.status.success());
    assert_eq!(fs::read_to_string(&f).unwrap(), "fn a() {}\n// two\nfn b() {}\n");
}

#[test]
fn lines_flag_rejects_garbage() {
    for bad in ["abc", "5-2", "0", "1-x"] {
        let out = run(&["--lines", bad, "a.rs"]);
        assert_eq!(out.status.code(), Some(2), "--lines {bad} should be rejected");
    }
}

#[test]
fn list_outputs_valid_json_and_modifies_nothing() {
    let src = "/// doc\nfn main() {\n    let x = 1; // eslint-disable-line\n}\n// gone\n";
    let f = tmpfile("a.rs", src);
    let out = run(&["--list", f.to_str().unwrap()]);
    assert!(out.status.success(), "stderr: {:?}", out.stderr);
    assert_eq!(fs::read_to_string(&f).unwrap(), src, "--list modified the file");

    let v: serde_json::Value = serde_json::from_slice(&out.stdout).expect("invalid JSON");
    assert_eq!(v["language"], "rust");
    let comments = v["comments"].as_array().unwrap();
    assert_eq!(comments.len(), 3);
    assert_eq!(comments[0]["id"], 0);
    assert_eq!(comments[0]["is_doc"], true);
    assert_eq!(comments[0]["text"], "/// doc");
    assert_eq!(comments[1]["is_directive"], true);
    assert_eq!(comments[2]["text"], "// gone");
    assert_eq!(comments[2]["start_line"], 5);
}

#[test]
fn list_json_escapes_special_chars() {
    let f = tmpfile("a.rs", "fn main() {} // \"quoted\" \\ back\tslash\n");
    let out = run(&["--list", f.to_str().unwrap()]);
    assert!(out.status.success());
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).expect("invalid JSON");
    assert_eq!(
        v["comments"][0]["text"],
        "// \"quoted\" \\ back\tslash"
    );
}

#[test]
fn list_empty_file_is_valid_json() {
    let f = tmpfile("a.rs", "fn main() {}\n");
    let out = run(&["--list", f.to_str().unwrap()]);
    assert!(out.status.success());
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).expect("invalid JSON");
    assert_eq!(v["comments"].as_array().unwrap().len(), 0);
}

#[test]
fn list_works_on_stdin() {
    let out = run_stdin(&["--list", "--stdin", "--lang", "py"], "# c\nx = 1\n");
    assert!(out.status.success());
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).expect("invalid JSON");
    assert_eq!(v["file"], "<stdin>");
    assert_eq!(v["comments"][0]["text"], "# c");
}

#[test]
fn apply_removes_exactly_those_ids() {
    let f = tmpfile("a.rs", "// zero\n// one\n// two\nfn main() {}\n");
    let out = run(&["--apply", "0,2", f.to_str().unwrap()]);
    assert!(out.status.success(), "stderr: {:?}", out.stderr);
    assert_eq!(fs::read_to_string(&f).unwrap(), "// one\nfn main() {}\n");
}

#[test]
fn apply_unknown_id_exits_2_untouched() {
    let src = "// a\nfn main() {}\n";
    let f = tmpfile("a.rs", src);
    let out = run(&["--apply", "9", f.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("unknown comment id"));
    assert_eq!(fs::read_to_string(&f).unwrap(), src);
}

#[test]
fn apply_invalid_id_syntax_exits_2() {
    let out = run(&["--apply", "1,x", "a.rs"]);
    assert_eq!(out.status.code(), Some(2));
}

#[test]
fn list_apply_roundtrip_through_binary() {
    // the full agent workflow against the real binary
    let src = "/// doc\nfn f() {}\n// narration\nfn g() {} // type: ignore\n";
    let f = tmpfile("a.rs", src);
    let out = run(&["--list", f.to_str().unwrap()]);
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let ids: Vec<String> = v["comments"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["is_doc"] == false && c["is_directive"] == false)
        .map(|c| c["id"].to_string())
        .collect();
    let out = run(&["--apply", &ids.join(","), f.to_str().unwrap()]);
    assert!(out.status.success(), "stderr: {:?}", out.stderr);
    let after = fs::read_to_string(&f).unwrap();
    assert!(after.contains("/// doc"));
    assert!(after.contains("// type: ignore"));
    assert!(!after.contains("narration"));
}

// --- install-zed-task (HOME is overridden so the real config is never touched) ---

fn run_install(home: &std::path::Path) -> Output {
    Command::new(BIN)
        .arg("install-zed-task")
        .env("HOME", home)
        .output()
        .unwrap()
}

fn fake_home() -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "sc-home-{}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::SeqCst)
    ));
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn tasks_path(home: &std::path::Path) -> PathBuf {
    home.join(".config/zed/tasks.json")
}

#[test]
fn install_creates_fresh_tasks_json() {
    let home = fake_home();
    let out = run_install(&home);
    assert!(out.status.success(), "stderr: {:?}", out.stderr);
    let written = fs::read_to_string(tasks_path(&home)).unwrap();
    assert!(written.contains("\"label\": \"rm-comments\""));
    assert!(written.contains(BIN), "task should use the binary's own path");
    assert!(written.contains("\"save\": \"current\""));
    // fresh file is strict JSON — must parse
    assert!(written.trim_start().starts_with('[') && written.trim_end().ends_with(']'));
}

#[test]
fn install_is_idempotent() {
    let home = fake_home();
    assert!(run_install(&home).status.success());
    let first = fs::read_to_string(tasks_path(&home)).unwrap();
    let out = run_install(&home);
    assert!(out.status.success());
    assert!(String::from_utf8_lossy(&out.stdout).contains("already present"));
    assert_eq!(fs::read_to_string(tasks_path(&home)).unwrap(), first);
}

#[test]
fn install_merges_into_existing_array() {
    let home = fake_home();
    let path = tasks_path(&home);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, "[\n  {\n    \"label\": \"other\",\n    \"command\": \"true\"\n  }\n]\n").unwrap();
    let out = run_install(&home);
    assert!(out.status.success(), "stderr: {:?}", out.stderr);
    let written = fs::read_to_string(&path).unwrap();
    assert!(written.contains("\"label\": \"other\""), "existing task lost");
    assert!(written.contains("\"label\": \"rm-comments\""));
    // result must still be valid JSON (no comments were present)
    let commas = written.matches("},").count();
    assert_eq!(commas, 1, "exactly one separating comma expected:\n{written}");
    // backup of the original was taken
    assert!(path.with_extension("json.bak").exists());
}

#[test]
fn install_merges_into_jsonc_with_comments_and_trailing_comma() {
    let home = fake_home();
    let path = tasks_path(&home);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(
        &path,
        "// my tasks\n[\n  {\n    \"label\": \"other\",\n    \"command\": \"true\"\n  }, // keep\n]\n",
    )
    .unwrap();
    let out = run_install(&home);
    assert!(out.status.success(), "stderr: {:?}", out.stderr);
    let written = fs::read_to_string(&path).unwrap();
    assert!(written.contains("// my tasks"), "user comment lost");
    assert!(written.contains("// keep"), "user comment lost");
    assert!(written.contains("\"label\": \"rm-comments\""));
    // trailing comma already separated the entries — none may be added inside the comment line
    assert!(!written.contains("keep,"), "comma landed in a comment:\n{written}");
}

#[test]
fn install_merges_into_empty_array() {
    let home = fake_home();
    let path = tasks_path(&home);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, "[]\n").unwrap();
    let out = run_install(&home);
    assert!(out.status.success(), "stderr: {:?}", out.stderr);
    let written = fs::read_to_string(&path).unwrap();
    assert!(written.contains("\"label\": \"rm-comments\""));
    assert!(!written.contains(",\n["), "stray comma in empty array:\n{written}");
}

#[test]
fn install_bails_on_garbage_without_modifying() {
    let home = fake_home();
    let path = tasks_path(&home);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, "not json at all").unwrap();
    let out = run_install(&home);
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("paste"));
    assert_eq!(fs::read_to_string(&path).unwrap(), "not json at all", "modified garbage file");
}

#[test]
fn crlf_file_in_place() {
    let f = tmpfile("a.rs", "// c\r\nfn main() {}\r\n");
    assert!(run(&[f.to_str().unwrap()]).status.success());
    assert_eq!(fs::read_to_string(&f).unwrap(), "fn main() {}\r\n");
}

#[test]
fn running_twice_is_idempotent() {
    let f = tmpfile("a.rs", DIRTY);
    assert!(run(&[f.to_str().unwrap()]).status.success());
    let once = fs::read_to_string(&f).unwrap();
    assert!(run(&[f.to_str().unwrap()]).status.success());
    assert_eq!(fs::read_to_string(&f).unwrap(), once);
}

// ===================================================================
// Directory mode: `rm-comments <DIR>` walks the tree (honoring
// .gitignore, skipping hidden dirs) and strips every supported file.
// ===================================================================

use std::path::Path;

/// Fresh unique empty temp dir; caller populates it.
fn tmpdir() -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "sc-cli-dir-{}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::SeqCst)
    ));
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// Write `dir/rel` (creating parent dirs); returns the path.
fn write_in(dir: &Path, rel: &str, contents: &str) -> PathBuf {
    let path = dir.join(rel);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, contents).unwrap();
    path
}

/// `git init` the dir so .gitignore is honored (the ignore crate needs a
/// real repo). Returns false if git isn't available so callers can skip.
fn git_init(dir: &Path) -> bool {
    Command::new("git")
        .args(["init", "-q"])
        .arg(dir)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Every file under `dir`, recursively (test-side walk, no gitignore logic).
fn all_files(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for e in fs::read_dir(dir).unwrap() {
        let p = e.unwrap().path();
        if p.is_dir() {
            out.extend(all_files(&p));
        } else {
            out.push(p);
        }
    }
    out
}

// --- traversal & stripping ---

#[test]
fn dir_strips_all_supported_files() {
    let d = tmpdir();
    let rs = write_in(&d, "a.rs", DIRTY);
    let py = write_in(&d, "b.py", "# c\nx = 1\n");
    let ts = write_in(&d, "c.ts", "// c\nconst x = 1;\n");
    let out = run(&[d.to_str().unwrap()]);
    assert!(out.status.success(), "stderr: {:?}", out.stderr);
    assert_eq!(fs::read_to_string(&rs).unwrap(), CLEAN);
    assert_eq!(fs::read_to_string(&py).unwrap(), "x = 1\n");
    assert_eq!(fs::read_to_string(&ts).unwrap(), "const x = 1;\n");
}

#[test]
fn dir_recurses_into_subdirs() {
    let d = tmpdir();
    let root = write_in(&d, "a.rs", DIRTY);
    let one = write_in(&d, "sub/b.rs", DIRTY);
    let two = write_in(&d, "sub/deep/c.rs", DIRTY);
    assert!(run(&[d.to_str().unwrap()]).status.success());
    for f in [&root, &one, &two] {
        assert_eq!(fs::read_to_string(f).unwrap(), CLEAN, "{}", f.display());
    }
}

#[test]
fn dir_skips_unknown_extensions() {
    let d = tmpdir();
    let rs = write_in(&d, "a.rs", DIRTY);
    let txt = write_in(&d, "keep.txt", "// not stripped\nhi\n");
    let json = write_in(&d, "data.json", "{\n  \"a\": 1\n}\n");
    let mk = write_in(&d, "Makefile", "# c\nall:\n");
    assert!(run(&[d.to_str().unwrap()]).status.success());
    assert_eq!(fs::read_to_string(&rs).unwrap(), CLEAN);
    assert_eq!(fs::read_to_string(&txt).unwrap(), "// not stripped\nhi\n");
    assert_eq!(fs::read_to_string(&json).unwrap(), "{\n  \"a\": 1\n}\n");
    assert_eq!(fs::read_to_string(&mk).unwrap(), "# c\nall:\n");
}

#[test]
fn dir_mixed_languages_each_uses_own_grammar() {
    let d = tmpdir();
    let rs = write_in(&d, "r.rs", "// c\nfn a() {}\n");
    let py = write_in(&d, "p.py", "# c\nx = 1\n");
    let go = write_in(&d, "g.go", "package main\n\n// c\nfunc a() {}\n");
    let css = write_in(&d, "s.css", "/* c */\na { color: red; }\n");
    assert!(run(&[d.to_str().unwrap()]).status.success());
    let (rs, py, go, css) = (
        fs::read_to_string(&rs).unwrap(),
        fs::read_to_string(&py).unwrap(),
        fs::read_to_string(&go).unwrap(),
        fs::read_to_string(&css).unwrap(),
    );
    assert!(!rs.contains("// c") && rs.contains("fn a"), "rust: {rs:?}");
    assert!(!py.contains("# c") && py.contains("x = 1"), "py: {py:?}");
    assert!(!go.contains("// c") && go.contains("func a"), "go: {go:?}");
    assert!(!css.contains("/* c */") && css.contains("color"), "css: {css:?}");
}

#[test]
fn dir_empty_dir_is_noop_exit_0() {
    let d = tmpdir();
    let out = run(&[d.to_str().unwrap()]);
    assert!(out.status.success());
    assert!(all_files(&d).is_empty());
}

#[test]
fn dir_already_clean_is_noop_exit_0() {
    let d = tmpdir();
    let a = write_in(&d, "a.rs", CLEAN);
    let b = write_in(&d, "sub/b.rs", CLEAN);
    let out = run(&[d.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(fs::read_to_string(&a).unwrap(), CLEAN);
    assert_eq!(fs::read_to_string(&b).unwrap(), CLEAN);
}

// --- .gitignore / hidden ---

#[test]
fn dir_respects_gitignore() {
    let d = tmpdir();
    if !git_init(&d) {
        eprintln!("git unavailable; skipping dir_respects_gitignore");
        return;
    }
    write_in(&d, ".gitignore", "skip/\n*.gen.rs\n");
    let plain = write_in(&d, "a.rs", DIRTY);
    let ignored_dir = write_in(&d, "skip/z.rs", DIRTY);
    let ignored_glob = write_in(&d, "root.gen.rs", DIRTY);
    assert!(run(&[d.to_str().unwrap()]).status.success());
    assert_eq!(fs::read_to_string(&plain).unwrap(), CLEAN);
    assert_eq!(fs::read_to_string(&ignored_dir).unwrap(), DIRTY, "gitignored dir touched");
    assert_eq!(fs::read_to_string(&ignored_glob).unwrap(), DIRTY, "gitignored glob touched");
}

#[test]
fn dir_skips_hidden_dirs() {
    let d = tmpdir();
    let plain = write_in(&d, "a.rs", DIRTY);
    let in_git = write_in(&d, ".git/x.rs", DIRTY);
    let in_hidden = write_in(&d, ".hidden/y.rs", DIRTY);
    assert!(run(&[d.to_str().unwrap()]).status.success());
    assert_eq!(fs::read_to_string(&plain).unwrap(), CLEAN);
    assert_eq!(fs::read_to_string(&in_git).unwrap(), DIRTY, "hidden .git touched");
    assert_eq!(fs::read_to_string(&in_hidden).unwrap(), DIRTY, "hidden dir touched");
}

#[test]
fn dir_nested_gitignore() {
    let d = tmpdir();
    if !git_init(&d) {
        eprintln!("git unavailable; skipping dir_nested_gitignore");
        return;
    }
    write_in(&d, "sub/.gitignore", "local.rs\n");
    let local = write_in(&d, "sub/local.rs", DIRTY);
    let other = write_in(&d, "sub/other.rs", DIRTY);
    assert!(run(&[d.to_str().unwrap()]).status.success());
    assert_eq!(fs::read_to_string(&local).unwrap(), DIRTY, "nested-gitignored file touched");
    assert_eq!(fs::read_to_string(&other).unwrap(), CLEAN);
}

// --- --check on a directory ---

#[test]
fn dir_check_any_dirty_exits_1_no_writes() {
    let d = tmpdir();
    let dirty = write_in(&d, "a.rs", DIRTY);
    let clean = write_in(&d, "b.rs", CLEAN);
    let out = run(&["--check", d.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(1));
    assert_eq!(fs::read_to_string(&dirty).unwrap(), DIRTY, "check wrote to file");
    assert_eq!(fs::read_to_string(&clean).unwrap(), CLEAN);
    assert!(String::from_utf8_lossy(&out.stdout).contains("a.rs"), "dirty file not named");
}

#[test]
fn dir_check_all_clean_exits_0() {
    let d = tmpdir();
    write_in(&d, "a.rs", CLEAN);
    write_in(&d, "sub/b.py", "x = 1\n");
    let out = run(&["--check", d.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(0));
}

#[test]
fn dir_dry_run_alias() {
    let d = tmpdir();
    write_in(&d, "a.rs", DIRTY);
    let out = run(&["--dry-run", d.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(1));
}

// --- --list on a directory ---

#[test]
fn dir_list_emits_json_array() {
    let d = tmpdir();
    let a = write_in(&d, "a.rs", "// c\nfn f() {}\n");
    write_in(&d, "b.py", "# c\nx = 1\n");
    let out = run(&["--list", d.to_str().unwrap()]);
    assert!(out.status.success(), "stderr: {:?}", out.stderr);
    assert_eq!(fs::read_to_string(&a).unwrap(), "// c\nfn f() {}\n", "--list modified a file");
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).expect("invalid JSON");
    let arr = v.as_array().expect("top level must be an array");
    assert_eq!(arr.len(), 2);
    for obj in arr {
        assert!(obj["language"].is_string());
        assert!(obj["comments"].is_array());
    }
}

#[test]
fn dir_list_ids_are_per_file() {
    let d = tmpdir();
    write_in(&d, "a.rs", "// zero\n// one\nfn f() {}\n");
    write_in(&d, "b.rs", "// zero\n// one\nfn g() {}\n");
    let out = run(&["--list", d.to_str().unwrap()]);
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    for obj in v.as_array().unwrap() {
        let comments = obj["comments"].as_array().unwrap();
        assert_eq!(comments.len(), 2);
        assert_eq!(comments[0]["id"], 0, "ids must restart per file");
        assert_eq!(comments[1]["id"], 1);
    }
}

#[test]
fn dir_list_deterministic_order() {
    let d = tmpdir();
    write_in(&d, "a.rs", "// c\nfn a() {}\n");
    write_in(&d, "m.rs", "// c\nfn m() {}\n");
    write_in(&d, "z.rs", "// c\nfn z() {}\n");
    let first = run(&["--list", d.to_str().unwrap()]).stdout;
    let second = run(&["--list", d.to_str().unwrap()]).stdout;
    assert_eq!(first, second, "--list output not deterministic");
}

// --- flag rejections on a directory ---

#[test]
fn dir_stdout_exits_2_untouched() {
    let d = tmpdir();
    let a = write_in(&d, "a.rs", DIRTY);
    let out = run(&["--stdout", d.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(2));
    assert_eq!(fs::read_to_string(&a).unwrap(), DIRTY);
}

#[test]
fn dir_apply_exits_2_untouched() {
    let d = tmpdir();
    let a = write_in(&d, "a.rs", DIRTY);
    let out = run(&["--apply", "0", d.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(2));
    assert_eq!(fs::read_to_string(&a).unwrap(), DIRTY);
}

// --- options propagate per file ---

#[test]
fn dir_doc_comments_kept_by_default() {
    let d = tmpdir();
    let a = write_in(&d, "a.rs", "/// doc\n// plain\nfn f() {}\n");
    let b = write_in(&d, "sub/b.rs", "/// doc\n// plain\nfn g() {}\n");
    assert!(run(&[d.to_str().unwrap()]).status.success());
    assert_eq!(fs::read_to_string(&a).unwrap(), "/// doc\nfn f() {}\n");
    assert_eq!(fs::read_to_string(&b).unwrap(), "/// doc\nfn g() {}\n");
}

#[test]
fn dir_strip_doc_comments() {
    let d = tmpdir();
    let a = write_in(&d, "a.rs", "/// doc\n// plain\nfn f() {}\n");
    let b = write_in(&d, "sub/b.rs", "/// doc\n// plain\nfn g() {}\n");
    assert!(run(&["--strip-doc-comments", d.to_str().unwrap()]).status.success());
    assert_eq!(fs::read_to_string(&a).unwrap(), "fn f() {}\n");
    assert_eq!(fs::read_to_string(&b).unwrap(), "fn g() {}\n");
}

#[test]
fn dir_markers_kept_by_default_stripped_on_request() {
    let d = tmpdir();
    let a = write_in(&d, "a.rs", "// TODO: later\n// noise\nfn f() {}\n");
    assert!(run(&[d.to_str().unwrap()]).status.success());
    assert_eq!(fs::read_to_string(&a).unwrap(), "// TODO: later\nfn f() {}\n", "marker stripped by default");
    assert!(run(&["--strip-markers", d.to_str().unwrap()]).status.success());
    assert_eq!(fs::read_to_string(&a).unwrap(), "fn f() {}\n");
}

#[test]
fn dir_keep_pattern() {
    let d = tmpdir();
    let a = write_in(&d, "a.rs", "// PERF: hot\n// noise\nfn f() {}\n");
    assert!(run(&["--keep", "PERF", d.to_str().unwrap()]).status.success());
    assert_eq!(fs::read_to_string(&a).unwrap(), "// PERF: hot\nfn f() {}\n");
}

#[test]
fn dir_strip_directives() {
    let d = tmpdir();
    let src = "// eslint-disable-next-line\nconsole.log(1);\n";
    let a = write_in(&d, "a.js", src);
    assert!(run(&[d.to_str().unwrap()]).status.success());
    assert_eq!(fs::read_to_string(&a).unwrap(), src, "directive stripped by default");
    assert!(run(&["--strip-directives", d.to_str().unwrap()]).status.success());
    assert_eq!(fs::read_to_string(&a).unwrap(), "console.log(1);\n");
}

// --- error resilience & never-corrupt ---

#[test]
fn dir_bad_file_skipped_others_processed() {
    let d = tmpdir();
    let bad_src = "// c\nfn main( { ][ not rust\n";
    let bad = write_in(&d, "bad.rs", bad_src);
    let good = write_in(&d, "good.rs", DIRTY);
    let ok = write_in(&d, "sub/ok.py", "# c\nx = 1\n");
    let out = run(&[d.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(2), "any error -> exit 2");
    assert_eq!(fs::read_to_string(&bad).unwrap(), bad_src, "bad file corrupted");
    assert_eq!(fs::read_to_string(&good).unwrap(), CLEAN, "good file not processed");
    assert_eq!(fs::read_to_string(&ok).unwrap(), "x = 1\n");
    assert!(String::from_utf8_lossy(&out.stderr).contains("bad.rs"), "bad file not reported");
}

#[test]
fn dir_non_utf8_file_skipped() {
    let d = tmpdir();
    let bin = d.join("bin.rs");
    fs::write(&bin, [0xff, 0xfe, b'/', b'/', b'x']).unwrap();
    let good = write_in(&d, "a.rs", DIRTY);
    let out = run(&[d.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(2));
    assert_eq!(fs::read(&bin).unwrap(), [0xff, 0xfe, b'/', b'/', b'x'], "binary corrupted");
    assert_eq!(fs::read_to_string(&good).unwrap(), CLEAN);
}

#[test]
fn dir_no_temp_files_left_behind() {
    let d = tmpdir();
    write_in(&d, "a.rs", DIRTY);
    write_in(&d, "sub/b.rs", DIRTY);
    write_in(&d, "sub/deep/c.py", "# c\nx = 1\n");
    run(&[d.to_str().unwrap()]);
    let leftovers: Vec<_> = all_files(&d)
        .into_iter()
        .filter(|p| p.to_string_lossy().contains("sc-tmp"))
        .collect();
    assert!(leftovers.is_empty(), "temp files left: {leftovers:?}");
}

#[test]
fn dir_idempotent() {
    let d = tmpdir();
    write_in(&d, "a.rs", DIRTY);
    write_in(&d, "sub/b.py", "# c\nx = 1\n");
    assert!(run(&[d.to_str().unwrap()]).status.success());
    let snapshot: Vec<(PathBuf, String)> = all_files(&d)
        .into_iter()
        .map(|p| {
            let s = fs::read_to_string(&p).unwrap();
            (p, s)
        })
        .collect();
    assert!(run(&[d.to_str().unwrap()]).status.success());
    for (p, before) in snapshot {
        assert_eq!(fs::read_to_string(&p).unwrap(), before, "{} changed on 2nd run", p.display());
    }
}

#[test]
fn dir_crlf_preserved() {
    let d = tmpdir();
    let a = write_in(&d, "a.rs", "// c\r\nfn main() {}\r\n");
    assert!(run(&[d.to_str().unwrap()]).status.success());
    assert_eq!(fs::read_to_string(&a).unwrap(), "fn main() {}\r\n");
}

#[cfg(unix)]
#[test]
fn dir_permissions_preserved() {
    use std::os::unix::fs::PermissionsExt;
    let d = tmpdir();
    let a = write_in(&d, "a.rs", DIRTY);
    fs::set_permissions(&a, fs::Permissions::from_mode(0o754)).unwrap();
    assert!(run(&[d.to_str().unwrap()]).status.success());
    let mode = fs::metadata(&a).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o754);
}
