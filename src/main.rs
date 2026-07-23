use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::exit;

use rm_comments::{languages, list_comments, strip_comments, Options};

const USAGE: &str = "\
Usage: rm-comments [OPTIONS] <FILE|DIR>
       rm-comments [OPTIONS] --stdin --lang <NAME>
       rm-comments install-zed-task

Remove plain/narration comments from a source file (in place by default).
Doc comments (///, /** */), directives (eslint-disable, # noqa, //go:, ...),
task markers (TODO, FIXME, HACK, XXX, BUG), and the line-1 shebang are KEPT by
default; opt into removing each with the --strip-* flags below. Given a
directory, walk it recursively (honoring .gitignore, skipping hidden dirs) and
process every supported source file. --apply and --stdout are file-only.

Commands:
  install-zed-task     Add a 'rm-comments' task to ~/.config/zed/tasks.json
  help                 Show this help

Options:
  --stdout             Write result to stdout instead of modifying the file
  --check, --dry-run   Report whether changes would be made (exit 1 if so); write nothing
  --stdin              Read source from stdin, write result to stdout (requires --lang)
  --lang <NAME>        Language name or extension (e.g. rust, py) for --stdin
  --strip-doc-comments Also remove doc comments (///, //!, /** */); kept by default
  --strip-directives   Also remove directive comments (eslint-disable, # noqa,
                       //go:, ...); kept by default
  --strip-markers      Also remove task markers (TODO, FIXME, HACK, XXX, BUG);
                       kept by default
  --keep <REGEX>       Preserve comments matching REGEX (repeatable)
  --lines <A-B>        Only remove comments within 1-based line range (repeatable)
  --list               Print all comments as JSON (id, span, text, flags); no changes
  --apply <IDS>        Remove exactly these comma-separated comment ids (from
                       --list) and ignore all keep policies
  -h, --help           Show this help";

fn die(msg: &str) -> ! {
    eprintln!("rm-comments: {msg}");
    exit(2);
}

fn parse_line_range(s: &str) -> (usize, usize) {
    let parse = |t: &str| {
        t.parse::<usize>()
            .ok()
            .filter(|&n| n > 0)
            .unwrap_or_else(|| die(&format!("--lines: invalid line number '{t}'")))
    };
    match s.split_once('-') {
        Some((a, b)) => {
            let (a, b) = (parse(a), parse(b));
            if a > b {
                die(&format!("--lines: range '{s}' is backwards"));
            }
            (a, b)
        }
        None => {
            let n = parse(s);
            (n, n)
        }
    }
}

fn main() {
    let mut file: Option<PathBuf> = None;
    let mut to_stdout = false;
    let mut check = false;
    let mut use_stdin = false;
    let mut lang_name: Option<String> = None;
    let mut list = false;
    let mut opts = Options::default();

    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        let mut value = |flag: &str| args.next().unwrap_or_else(|| die(&format!("{flag} needs a value")));
        match arg.as_str() {
            "install-zed-task" => {
                install_zed_task();
                return;
            }
            "--stdout" => to_stdout = true,
            "--check" | "--dry-run" => check = true,
            "--stdin" => use_stdin = true,
            "--lang" => lang_name = Some(value("--lang")),
            "--strip-doc-comments" => opts.keep_doc_comments = false,
            "--strip-directives" => opts.keep_directives = false,
            "--strip-markers" => opts.keep_markers = false,
            "--keep" => {
                let pat = value("--keep");
                opts.keep_patterns.push(
                    regex::Regex::new(&pat)
                        .unwrap_or_else(|e| die(&format!("--keep: invalid regex '{pat}': {e}"))),
                );
            }
            "--lines" => opts.lines.push(parse_line_range(&value("--lines"))),
            "--list" => list = true,
            "--apply" => {
                let ids = value("--apply")
                    .split(',')
                    .map(|t| {
                        t.trim()
                            .parse::<usize>()
                            .unwrap_or_else(|_| die(&format!("--apply: invalid id '{t}'")))
                    })
                    .collect();
                opts.only_ids = Some(ids);
            }
            "-h" | "--help" | "help" => {
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

    // stdin is its own input source; it can never be a directory.
    if use_stdin {
        let lang_name = lang_name.unwrap_or_else(|| die("--stdin requires --lang <NAME>"));
        let lang = languages::by_name(&lang_name)
            .unwrap_or_else(|| die(&format!("unknown language '{lang_name}'")));
        let mut src = String::new();
        std::io::stdin()
            .read_to_string(&mut src)
            .unwrap_or_else(|e| die(&format!("reading stdin: {e}")));
        if list {
            let comments =
                list_comments(&src, lang).unwrap_or_else(|e| die(&format!("<stdin>: {e}")));
            print!("{}", comments_json("<stdin>", lang.name, &comments));
            return;
        }
        let out =
            strip_comments(&src, lang, &opts).unwrap_or_else(|e| die(&format!("<stdin>: {e}")));
        if check {
            if out != src {
                println!("<stdin>: would remove comments");
                exit(1);
            }
            exit(0);
        }
        print!("{out}");
        return;
    }

    let path = file.unwrap_or_else(|| die(&format!("no file given\n{USAGE}")));
    let mode = if list {
        Mode::List
    } else if check {
        Mode::Check
    } else if to_stdout {
        Mode::Stdout
    } else {
        Mode::Write
    };

    if path.is_dir() {
        // Per-file positional ids / a single output stream have no meaning across many files.
        if to_stdout {
            die("--stdout is not supported for a directory");
        }
        if opts.only_ids.is_some() {
            die("--apply is not supported for a directory");
        }
        run_dir(&path, &opts, mode);
    }

    match process_one(&path, &opts, mode) {
        Ok(out) => match mode {
            Mode::List => print!("{}", out.list_json.unwrap()),
            Mode::Check => {
                if out.changed {
                    println!("{}: would remove comments", path.display());
                    exit(1);
                }
                exit(0);
            }
            Mode::Stdout | Mode::Write => {}
        },
        Err(e) => die(&format!("{}: {e}", path.display())),
    }
}

#[derive(Clone, Copy)]
enum Mode {
    Write,
    Check,
    Stdout,
    List,
}

struct Outcome {
    changed: bool,
    list_json: Option<String>,
}

/// Process a single file: resolve its language, read it, and act per `mode`.
/// Returns an error string (never dies) so directory mode can skip-and-continue;
/// callers own the exit-code / message policy. Only `Write` touches the file.
fn process_one(path: &Path, opts: &Options, mode: Mode) -> Result<Outcome, String> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .ok_or_else(|| "no file extension".to_string())?;
    let lang = languages::by_extension(ext)
        .ok_or_else(|| format!("unsupported extension '.{ext}' (file left untouched)"))?;
    let src = std::fs::read_to_string(path).map_err(|e| e.to_string())?;

    if let Mode::List = mode {
        let comments = list_comments(&src, lang).map_err(|e| e.to_string())?;
        let json = comments_json(&path.display().to_string(), lang.name, &comments);
        return Ok(Outcome { changed: false, list_json: Some(json) });
    }

    let out = strip_comments(&src, lang, opts).map_err(|e| e.to_string())?;
    let changed = out != src;
    match mode {
        Mode::Stdout => print!("{out}"),
        Mode::Write if changed => write_atomic(path, &out).map_err(|e| e.to_string())?,
        _ => {}
    }
    Ok(Outcome { changed, list_json: None })
}

/// Walk a directory (honoring .gitignore, skipping hidden dirs) and run every
/// supported source file through `process_one`. Unknown extensions are silently
/// skipped; a file that errors (unreadable, doesn't parse) is left untouched,
/// reported to stderr, and does not abort the rest of the walk.
fn run_dir(dir: &Path, opts: &Options, mode: Mode) -> ! {
    let mut any_changed = false;
    let mut had_error = false;
    let mut list_items: Vec<String> = Vec::new();

    for result in ignore::WalkBuilder::new(dir)
        .sort_by_file_name(|a, b| a.cmp(b))
        .build()
    {
        let entry = match result {
            Ok(e) => e,
            Err(e) => {
                eprintln!("rm-comments: {e}");
                had_error = true;
                continue;
            }
        };
        if !entry.file_type().is_some_and(|t| t.is_file()) {
            continue;
        }
        let p = entry.path();
        match p.extension().and_then(|e| e.to_str()) {
            Some(ext) if languages::by_extension(ext).is_some() => {}
            _ => continue, // unknown/no extension: not ours to touch
        }
        match process_one(p, opts, mode) {
            Ok(o) => {
                if o.changed {
                    any_changed = true;
                    if let Mode::Check = mode {
                        println!("{}: would remove comments", p.display());
                    }
                }
                if let Some(j) = o.list_json {
                    list_items.push(j.trim_end().to_string());
                }
            }
            Err(e) => {
                eprintln!("rm-comments: {}: {e}", p.display());
                had_error = true;
            }
        }
    }

    if let Mode::List = mode {
        print!("[\n{}\n]\n", list_items.join(",\n"));
    }
    if had_error {
        exit(2);
    }
    if let Mode::Check = mode
        && any_changed
    {
        exit(1);
    }
    exit(0);
}

fn json_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

fn comments_json(file: &str, lang: &str, comments: &[rm_comments::Comment]) -> String {
    let items: Vec<String> = comments
        .iter()
        .map(|c| {
            format!(
                r#"    {{"id": {}, "kind": "{}", "start_line": {}, "end_line": {}, "start_byte": {}, "end_byte": {}, "is_doc": {}, "is_directive": {}, "is_marker": {}, "text": "{}"}}"#,
                c.id,
                c.kind,
                c.start_line,
                c.end_line,
                c.start_byte,
                c.end_byte,
                c.is_doc,
                c.is_directive,
                c.is_marker,
                json_escape(&c.text)
            )
        })
        .collect();
    format!(
        "{{\n  \"file\": \"{}\",\n  \"language\": \"{}\",\n  \"comments\": [\n{}\n  ]\n}}\n",
        json_escape(file),
        lang,
        items.join(",\n")
    )
}

const TASK_LABEL: &str = "rm-comments";

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

/// Add the 'rm-comments' task to ~/.config/zed/tasks.json.
///
/// Missing file: written whole. Existing file: the entry is spliced before
/// the final `]` so user comments and trailing commas (Zed allows both) are
/// left intact. Anything unexpected -> back off and print the snippet to
/// paste manually; the original file is backed up before any modification.
fn install_zed_task() {
    // Deliberately not canonicalized: a brew-installed binary is a symlink
    // into a versioned Cellar path that changes on upgrade, while the
    // symlink itself is stable.
    let exe = std::env::current_exe()
        .ok()
        .filter(|p| p.is_absolute())
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
