use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::exit;

use rm_comments::{languages, strip_comments};

const USAGE: &str = "\
Usage: rm-comments [OPTIONS] <FILE>
       rm-comments [OPTIONS] --stdin --lang <NAME>
       rm-comments install-zed-task

Strip all comments from a source file (in place by default).

Commands:
  install-zed-task     Add a 'Strip Comments' task to ~/.config/zed/tasks.json

Options:
  --stdout             Write result to stdout instead of modifying the file
  --check, --dry-run   Report whether changes would be made (exit 1 if so); write nothing
  --stdin              Read source from stdin, write result to stdout (requires --lang)
  --lang <NAME>        Language name or extension (e.g. rust, py) for --stdin
  --keep-doc-comments  Preserve doc comments (///, //!, /** */); default removes all
  -h, --help           Show this help";

fn die(msg: &str) -> ! {
    eprintln!("rm-comments: {msg}");
    exit(2);
}

fn main() {
    let mut file: Option<PathBuf> = None;
    let mut to_stdout = false;
    let mut check = false;
    let mut use_stdin = false;
    let mut lang_name: Option<String> = None;
    let mut keep_doc = false;

    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "install-zed-task" => {
                install_zed_task();
                return;
            }
            "--stdout" => to_stdout = true,
            "--check" | "--dry-run" => check = true,
            "--stdin" => use_stdin = true,
            "--lang" => lang_name = Some(args.next().unwrap_or_else(|| die("--lang needs a value"))),
            "--keep-doc-comments" => keep_doc = true,
            "-h" | "--help" => {
                println!("{USAGE}");
                return;
            }
            _ if arg.starts_with('-') => die(&format!("unknown flag {arg}\n{USAGE}")),
            _ => {
                if file.replace(PathBuf::from(&arg)).is_some() {
                    die("only one FILE argument is allowed");
                }
            }
        }
    }

    if use_stdin {
        let lang_name = lang_name.unwrap_or_else(|| die("--stdin requires --lang <NAME>"));
        let lang = languages::by_name(&lang_name)
            .unwrap_or_else(|| die(&format!("unknown language '{lang_name}'")));
        let mut src = String::new();
        std::io::stdin()
            .read_to_string(&mut src)
            .unwrap_or_else(|e| die(&format!("reading stdin: {e}")));
        let out = strip_comments(&src, lang, keep_doc).unwrap_or_else(|e| die(&e));
        if check {
            exit(if out != src { 1 } else { 0 });
        }
        print!("{out}");
        return;
    }

    let path = file.unwrap_or_else(|| die(&format!("no file given\n{USAGE}")));
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or_else(|| die(&format!("{}: no file extension", path.display())));
    let lang = languages::by_extension(ext).unwrap_or_else(|| {
        die(&format!(
            "{}: unsupported extension '.{ext}' (file left untouched)",
            path.display()
        ))
    });
    let src = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| die(&format!("{}: {e}", path.display())));

    let out = strip_comments(&src, lang, keep_doc)
        .unwrap_or_else(|e| die(&format!("{}: {e}", path.display())));

    if check {
        if out != src {
            println!("{}: would remove comments", path.display());
            exit(1);
        }
        exit(0);
    }
    if to_stdout {
        print!("{out}");
        return;
    }
    if out != src {
        write_atomic(&path, &out).unwrap_or_else(|e| die(&format!("{}: {e}", path.display())));
    }
}

const TASK_LABEL: &str = "Strip Comments";

fn task_entry(exe: &str) -> String {
    format!(
        r#"  {{
    "label": "{TASK_LABEL}",
    "command": "{exe} \"$ZED_FILE\"",
    "use_new_terminal": false,
    "allow_concurrent_runs": true,
    "reveal": "never",
    "hide": "on_success",
    "save": "current"
  }}"#
    )
}

/// Add the 'Strip Comments' task to ~/.config/zed/tasks.json.
///
/// Missing file: written whole. Existing file: the entry is spliced before
/// the final `]` so user comments and trailing commas (Zed allows both) are
/// left intact. Anything unexpected -> back off and print the snippet to
/// paste manually; the original file is backed up before any modification.
fn install_zed_task() {
    let exe = std::env::current_exe()
        .ok()
        .and_then(|p| p.canonicalize().ok())
        .unwrap_or_else(|| die("cannot determine own binary path"));
    let entry = task_entry(&exe.to_string_lossy());

    let home = std::env::var("HOME")
        .unwrap_or_else(|_| die(&format!("$HOME not set; add this to Zed's tasks.json yourself:\n[\n{entry}\n]")));
    let dir = Path::new(&home).join(".config/zed");
    let path = dir.join("tasks.json");

    let existing = match std::fs::read_to_string(&path) {
        Ok(s) => s,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            std::fs::create_dir_all(&dir)
                .unwrap_or_else(|e| die(&format!("{}: {e}", dir.display())));
            std::fs::write(&path, format!("[\n{entry}\n]\n"))
                .unwrap_or_else(|e| die(&format!("{}: {e}", path.display())));
            println!("Created {} with the '{TASK_LABEL}' task.", path.display());
            println!("In Zed: cmd-shift-p -> 'task: spawn' -> {TASK_LABEL}");
            return;
        }
        Err(e) => die(&format!("{}: {e}", path.display())),
    };

    if existing.contains(&format!("\"{TASK_LABEL}\"")) {
        println!("'{TASK_LABEL}' already present in {} — nothing to do.", path.display());
        return;
    }

    let manual = format!(
        "couldn't safely edit {}; paste this entry into its top-level array yourself:\n{entry}",
        path.display()
    );
    // Splice before the final `]`. Decide on a separating comma by the last
    // meaningful (non-comment, non-blank) character before it.
    // Handles // comments only; a /* */ straddling the final `]` defeats this,
    // in which case we bail to manual paste rather than risk mangling the file.
    let close = existing.rfind(']').unwrap_or_else(|| die(&manual));
    let before = &existing[..close];
    let last_meaningful = before
        .lines()
        .map(|l| l.find("//").map_or(l, |i| &l[..i]).trim_end())
        .filter(|l| !l.trim().is_empty())
        .next_back()
        .and_then(|l| l.chars().next_back())
        .unwrap_or_else(|| die(&manual));
    let sep = match last_meaningful {
        '[' | ',' => "",
        '*' | '/' => die(&manual), // block comment near the end; don't guess
        _ => ",",
    };

    let backup = path.with_extension("json.bak");
    std::fs::copy(&path, &backup).unwrap_or_else(|e| die(&format!("{}: {e}", backup.display())));
    let (trimmed, rest) = (before.trim_end(), &existing[close..]);
    let updated = if sep.is_empty() {
        format!("{trimmed}\n{entry}\n{rest}")
    } else if trimmed.lines().next_back().is_some_and(|l| l.contains("//")) {
        // the comma can't ride on a comment line; give it its own
        format!("{trimmed}\n  ,\n{entry}\n{rest}")
    } else {
        format!("{trimmed},\n{entry}\n{rest}")
    };
    std::fs::write(&path, updated).unwrap_or_else(|e| die(&format!("{}: {e}", path.display())));
    println!(
        "Added '{TASK_LABEL}' to {} (backup at {}).",
        path.display(),
        backup.display()
    );
    println!("In Zed: cmd-shift-p -> 'task: spawn' -> {TASK_LABEL}");
}

/// Write via a temp file in the same directory + rename, so a crash can
/// never leave a truncated file. Preserves the original file's permissions.
fn write_atomic(path: &Path, contents: &str) -> std::io::Result<()> {
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    let name = path.file_name().unwrap().to_string_lossy();
    let tmp = dir.join(format!(".{name}.sc-tmp{}", std::process::id()));
    let result = (|| {
        std::fs::write(&tmp, contents)?;
        let perms = std::fs::metadata(path)?.permissions();
        std::fs::set_permissions(&tmp, perms)?;
        std::fs::rename(&tmp, path)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    result
}
