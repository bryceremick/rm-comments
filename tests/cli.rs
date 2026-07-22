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
    let out = run(&["--help"]);
    assert_eq!(out.status.code(), Some(0));
    assert!(String::from_utf8_lossy(&out.stdout).contains("Usage"));
}

// --- file-system behavior ---

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
    assert!(written.contains("\"label\": \"Strip Comments\""));
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
    assert!(written.contains("\"label\": \"Strip Comments\""));
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
    assert!(written.contains("\"label\": \"Strip Comments\""));
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
    assert!(written.contains("\"label\": \"Strip Comments\""));
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
