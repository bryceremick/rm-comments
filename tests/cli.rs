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
    child
        .stdin
        .take()
        .unwrap()
        .write_all(input.as_bytes())
        .unwrap();
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
fn keep_doc_comments_flag() {
    let f = tmpfile("a.rs", "/// doc\n// plain\nfn main() {}\n");
    let out = run(&["--keep-doc-comments", f.to_str().unwrap()]);
    assert!(out.status.success());
    assert_eq!(fs::read_to_string(&f).unwrap(), "/// doc\nfn main() {}\n");
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
    let f = tmpfile("a.rs", "// TODO: later\n// noise\nfn main() {}\n");
    let out = run(&["--keep", "TODO", f.to_str().unwrap()]);
    assert!(out.status.success(), "stderr: {:?}", out.stderr);
    assert_eq!(fs::read_to_string(&f).unwrap(), "// TODO: later\nfn main() {}\n");
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
