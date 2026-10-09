//! `cs-scan` — scan a drive or folder and print a summary. Read-only: it never modifies
//! anything except the CSV file you explicitly ask it to create (and never overwrites one).

use std::io::Write;
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Duration;

use clearspace_model::{NodeId, csv::write_csv, flags};
use clearspace_scanner::paths::normalize_root;
use clearspace_scanner::scan::{Progress, ScanOptions, ScanOutcome, ScanStatus, start};
use clearspace_scanner::source::FsSource;
use clearspace_scanner::std_source::StdSource;

const USAGE: &str = "\
Usage: cs-scan <PATH> [options]

Scans a drive (e.g. C:) or folder without administrator rights and prints where the space
goes. Nothing is ever deleted or changed. Press Ctrl+C to stop early (partial results are
still shown).

Options:
  --threads N        worker threads (default: CPU count, 2..8)
  --top N            rows to show in each table (default: 15)
  --csv FILE         also export every entry to a new CSV file (never overwrites)
  --source std       use the portable std::fs backend instead of the native one
  --quiet            no live progress line
  -h, --help         show this help";

struct Args {
    root: PathBuf,
    threads: Option<usize>,
    top: usize,
    csv: Option<PathBuf>,
    portable: bool,
    quiet: bool,
}

fn parse_args() -> Result<Args, String> {
    let mut it = std::env::args_os().skip(1);
    let mut root = None;
    let mut args = Args {
        root: PathBuf::new(),
        threads: None,
        top: 15,
        csv: None,
        portable: false,
        quiet: false,
    };
    while let Some(a) = it.next() {
        let mut value = |name: &str| it.next().ok_or_else(|| format!("{name} needs a value"));
        match a.to_str() {
            Some("-h" | "--help") => return Err(String::new()),
            Some("--threads") => {
                args.threads = Some(
                    value("--threads")?
                        .to_string_lossy()
                        .parse()
                        .map_err(|_| "bad --threads")?,
                )
            }
            Some("--top") => {
                args.top = value("--top")?
                    .to_string_lossy()
                    .parse()
                    .map_err(|_| "bad --top")?
            }
            Some("--csv") => args.csv = Some(value("--csv")?.into()),
            Some("--source") => match value("--source")?.to_str() {
                Some("std") => args.portable = true,
                Some("native") => args.portable = false,
                _ => return Err("--source must be std or native".into()),
            },
            Some("--quiet") => args.quiet = true,
            Some(s) if s.starts_with('-') => return Err(format!("unknown option {s}")),
            _ if root.is_none() => root = Some(PathBuf::from(a)),
            _ => return Err("only one PATH may be given".into()),
        }
    }
    args.root = root.ok_or("missing PATH")?;
    Ok(args)
}

fn main() -> ExitCode {
    let args = match parse_args() {
        Ok(a) => a,
        Err(e) => {
            if !e.is_empty() {
                eprintln!("error: {e}\n");
            }
            eprintln!("{USAGE}");
            return ExitCode::from(2);
        }
    };
    let root = match normalize_root(&args.root) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("error: {}: {e}", args.root.display());
            return ExitCode::from(2);
        }
    };
    let mut options = ScanOptions::default();
    if let Some(t) = args.threads {
        options.threads = t;
    }
    if args.portable {
        run(StdSource, root, options, &args)
    } else {
        run(clearspace_scanner::default_source(), root, options, &args)
    }
}

fn run<S: FsSource>(source: S, root: PathBuf, options: ScanOptions, args: &Args) -> ExitCode {
    eprintln!(
        "Scanning {} with {} threads…",
        root.display(),
        options.threads
    );
    let handle = start(source, root, options);
    let token = handle.cancel_token();
    let _ = ctrlc::set_handler(move || {
        eprintln!("\nStopping…");
        token.cancel();
    });
    while !handle.is_finished() {
        std::thread::sleep(Duration::from_millis(200));
        if !args.quiet {
            eprint!("\r{:<100}", progress_line(&handle.progress()));
            let _ = std::io::stderr().flush();
        }
    }
    if !args.quiet {
        eprintln!();
    }
    let outcome = handle.wait();
    report(&outcome, args.top);
    if let Some(csv) = &args.csv {
        let file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(csv);
        match file.and_then(|f| write_csv(&outcome.tree, f)) {
            Ok(()) => println!("\nCSV written to {}", csv.display()),
            Err(e) => {
                eprintln!("error: could not write {}: {e}", csv.display());
                return ExitCode::FAILURE;
            }
        }
    }
    match outcome.status {
        ScanStatus::Completed | ScanStatus::Cancelled => ExitCode::SUCCESS,
        _ => ExitCode::FAILURE,
    }
}

fn progress_line(p: &Progress) -> String {
    format!(
        "  {} files · {} folders · {} · {} issues · {:.1}s",
        thousands(p.files),
        thousands(p.dirs),
        human(p.bytes),
        p.issues,
        p.elapsed.as_secs_f64()
    )
}

fn report(o: &ScanOutcome, top: usize) {
    let t = &o.tree;
    let root = t.node(NodeId::ROOT);
    let secs = o.progress.elapsed.as_secs_f64().max(0.001);
    println!("\n== {} ==", t.path(NodeId::ROOT).display());
    println!("Status:        {}", status_text(&o.status));
    println!(
        "Size:          {} ({} bytes)",
        human(root.size),
        thousands(root.size)
    );
    println!("Size on disk:  {}", human(root.alloc));
    println!("Files:         {}", thousands(u64::from(root.files)));
    println!("Folders:       {}", thousands(o.progress.dirs));
    println!(
        "Time:          {secs:.2}s ({} entries/s)",
        thousands(((o.progress.files + o.progress.dirs) as f64 / secs) as u64)
    );
    println!(
        "Memory:        {} for {} entries ({} bytes/entry)",
        human(t.memory_bytes() as u64),
        thousands(t.len() as u64),
        t.memory_bytes() / t.len().max(1)
    );
    let count = |f: u16| {
        (0..t.len() as u32)
            .filter(|&i| t.node(NodeId::from_raw(i)).has(f))
            .count()
    };
    println!(
        "Links skipped: {} · hard-link duplicates: {} · cloud placeholders: {}",
        count(flags::LINK),
        count(flags::HARDLINK_DUP),
        count(flags::CLOUD)
    );

    println!("\nLargest items in the top folder:");
    for id in t.children_by_size(NodeId::ROOT).into_iter().take(top) {
        let n = t.node(id);
        let pct = if root.size > 0 {
            n.size as f64 * 100.0 / root.size as f64
        } else {
            0.0
        };
        let kind = if n.has(flags::LINK) {
            " (link, not followed)"
        } else if n.is_dir() {
            "\\"
        } else {
            ""
        };
        println!(
            "  {:>10}  {pct:5.1}%  {}  {}{kind}",
            human(n.size),
            bar(pct),
            t.name(id).to_string_lossy()
        );
    }

    println!("\nLargest files:");
    for id in t.largest_files(top) {
        println!("  {:>10}  {}", human(t.node(id).size), t.path(id).display());
    }

    println!("\nFile types:");
    for s in t.extension_breakdown().into_iter().take(top) {
        let name = match t.exts().name(s.ext) {
            "" => "(no extension)".to_string(),
            n => format!(".{n}"),
        };
        println!(
            "  {:>10}  {:>10} files  {name}",
            human(s.size),
            thousands(s.files)
        );
    }

    if !o.issues.is_empty() {
        println!("\nFolders that couldn't be read ({}):", o.issues.len());
        for i in o.issues.iter().take(top) {
            let partial = if i.partial { " (partly read)" } else { "" };
            println!("  {}: {}{partial}", i.path.display(), i.error);
        }
        if o.issues.len() > top {
            println!("  … and {} more", o.issues.len() - top);
        }
    }
}

fn status_text(s: &ScanStatus) -> String {
    match s {
        ScanStatus::Running => "still running".into(),
        ScanStatus::Completed => "complete".into(),
        ScanStatus::Cancelled => "stopped early (partial results)".into(),
        ScanStatus::VolumeLost => "the drive disappeared (partial results)".into(),
        ScanStatus::OutOfMemory(e) => format!("stopped: {e} (partial results)"),
        ScanStatus::RootUnavailable(e) => format!("could not open the folder: {e}"),
    }
}

fn bar(pct: f64) -> String {
    let filled = ((pct / 5.0).round() as usize).min(20);
    format!("{}{}", "█".repeat(filled), "·".repeat(20 - filled))
}

/// Sizes the way Windows Explorer shows them (1 KB = 1024 bytes).
fn human(bytes: u64) -> String {
    const UNITS: [&str; 6] = ["bytes", "KB", "MB", "GB", "TB", "PB"];
    let mut v = bytes as f64;
    let mut u = 0;
    while v >= 1024.0 && u < UNITS.len() - 1 {
        v /= 1024.0;
        u += 1;
    }
    if u == 0 {
        format!("{bytes} bytes")
    } else {
        format!("{v:.1} {}", UNITS[u])
    }
}

fn thousands(n: u64) -> String {
    let s = n.to_string();
    let mut out = String::with_capacity(s.len() + s.len() / 3);
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}
