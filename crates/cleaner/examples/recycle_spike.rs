//! Recycle Bin spike (milestone a): can we guarantee that "Move to Recycle Bin" never
//! silently becomes a permanent delete?
//!
//! Compares two ways of recycling:
//!   A. the `trash` crate (`trash::delete`), which uses IFileOperation with
//!      `FOF_NO_UI | FOF_ALLOWUNDO | FOF_WANTNUKEWARNING`;
//!   B. IFileOperation driven directly, with `FOFX_RECYCLEONDELETE` and a progress sink that
//!      vetoes any item whose `PreDeleteItem` flags lack `TSF_DELETE_RECYCLE_IF_POSSIBLE`
//!      (i.e. Windows is about to destroy it instead of recycling it).
//!
//! SAFETY OF THIS PROGRAM: it only ever deletes files it has just created inside
//! `%TEMP%\clearspace-recycle-spike-<pid>\`. Scenarios that change Recycle Bin settings in the
//! registry (bin disabled, tiny quota) or use the `\\localhost\C$` admin share run only when
//! `CLEARSPACE_SPIKE_CI=1` (GitHub Actions), and restore the registry afterwards.
//!
//! Usage: `cargo run -p clearspace-cleaner --example recycle_spike [-- --report FILE]`

#![allow(clippy::disallowed_methods)] // The spike lives in the cleaner crate on purpose.

#[cfg(not(windows))]
fn main() {
    eprintln!("The Recycle Bin spike only runs on Windows.");
}

#[cfg(windows)]
fn main() -> std::process::ExitCode {
    win::main()
}

#[cfg(windows)]
mod win {
    use std::ffi::OsString;
    use std::fs;
    use std::io::Write;
    use std::os::windows::ffi::OsStrExt;
    use std::os::windows::fs::OpenOptionsExt;
    use std::path::{Path, PathBuf};
    use std::process::{Command, ExitCode, Stdio};
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};

    use windows::Win32::Foundation::E_ABORT;
    use windows::Win32::Storage::FileSystem::GetVolumeNameForVolumeMountPointW;
    use windows::Win32::System::Com::{
        CLSCTX_ALL, COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE, CoCreateInstance,
        CoInitializeEx, CoTaskMemFree,
    };
    use windows::Win32::System::Registry::{
        HKEY, HKEY_CURRENT_USER, KEY_READ, KEY_WRITE, REG_DWORD, REG_OPTION_NON_VOLATILE,
        RRF_RT_REG_DWORD, RegCloseKey, RegCreateKeyExW, RegDeleteValueW, RegGetValueW,
        RegSetValueExW,
    };
    use windows::Win32::UI::Shell::{
        FOF_ALLOWUNDO, FOF_NOCONFIRMATION, FOF_NOCONFIRMMKDIR, FOF_NOERRORUI, FOF_SILENT,
        FOF_WANTNUKEWARNING, FOFX_RECYCLEONDELETE, FileOperation, IFileOperation,
        IFileOperationProgressSink, IFileOperationProgressSink_Impl, IShellItem,
        SHCreateItemFromParsingName, SIGDN_DESKTOPABSOLUTEPARSING, TSF_DELETE_RECYCLE_IF_POSSIBLE,
    };
    use windows::core::{HRESULT, PCWSTR, Ref, implement};

    const CHILD_TIMEOUT: Duration = Duration::from_secs(60);

    // ---------------------------------------------------------------- parent ------------

    #[derive(Clone, Copy)]
    struct Scenario {
        id: &'static str,
        what: &'static str,
        ci_only: bool,
        registry: Option<(&'static str, u32)>,
    }

    const SCENARIOS: &[Scenario] = &[
        Scenario {
            id: "small-file",
            what: "Small file",
            ci_only: false,
            registry: None,
        },
        Scenario {
            id: "folder",
            what: "Folder with nested files",
            ci_only: false,
            registry: None,
        },
        Scenario {
            id: "locked",
            what: "File held open with no sharing",
            ci_only: false,
            registry: None,
        },
        Scenario {
            id: "readonly",
            what: "Read-only file",
            ci_only: false,
            registry: None,
        },
        Scenario {
            id: "long-path",
            what: "Path longer than 260 chars",
            ci_only: false,
            registry: None,
        },
        Scenario {
            id: "unicode",
            what: "Unicode name",
            ci_only: false,
            registry: None,
        },
        Scenario {
            id: "bin-disabled",
            what: "Recycle Bin turned off for the drive",
            ci_only: true,
            registry: Some(("NukeOnDelete", 1)),
        },
        Scenario {
            id: "over-quota",
            what: "File bigger than the Recycle Bin quota (1 MB quota, 8 MB file)",
            ci_only: true,
            registry: Some(("MaxCapacity", 1)),
        },
        Scenario {
            id: "over-quota-folder",
            what: "Folder bigger than the quota, every file smaller (1 MB quota, 16 × 512 KB)",
            ci_only: true,
            registry: Some(("MaxCapacity", 1)),
        },
        Scenario {
            id: "bin-nearly-full",
            what: "Bin nearly full (10 MB quota, 9 MB already recycled, then a 5 MB file)",
            ci_only: true,
            registry: Some(("MaxCapacity", 10)),
        },
        Scenario {
            id: "network-path",
            what: "File addressed via network share (\\\\localhost\\C$)",
            ci_only: true,
            registry: None,
        },
    ];
    const APPROACHES: &[(&str, &str)] = &[
        ("A", "trash crate"),
        ("B", "IFileOperation + veto sink"),
        ("B2", "B + FOF_WANTNUKEWARNING"),
    ];

    pub fn main() -> ExitCode {
        let args: Vec<OsString> = std::env::args_os().collect();
        if args.get(1).is_some_and(|a| a == "--child") {
            return child(&args[2..]);
        }
        let report_path = args
            .iter()
            .position(|a| a == "--report")
            .and_then(|i| args.get(i + 1))
            .map(PathBuf::from);
        let ci = std::env::var("CLEARSPACE_SPIKE_CI").is_ok_and(|v| v == "1");

        // %TEMP% is often an 8.3 short path (C:\Users\RUNNER~1\...), while the Recycle Bin
        // records long paths. Compare like with like: use the long form throughout.
        let sandbox =
            long_temp_dir().join(format!("clearspace-recycle-spike-{}", std::process::id()));
        fs::create_dir_all(&sandbox).expect("create sandbox");
        let exe = std::env::current_exe().expect("current exe");

        let mut rows = Vec::new();
        for sc in SCENARIOS {
            if sc.ci_only && !ci {
                rows.push(format!("| {} | – | skipped (CI only) | | | | |", sc.what));
                continue;
            }
            let guard = sc
                .registry
                .map(|(name, value)| RegistryGuard::set(&sandbox, name, value));
            for (ap, ap_name) in APPROACHES {
                let dir = sandbox.join(format!("{}-{ap}", sc.id));
                let result = run_child(&exe, sc.id, ap, &dir);
                eprintln!("[{}/{ap}] {result}", sc.id);
                rows.push(format!("| {} | {ap_name} | {} |", sc.what, row(&result)));
            }
            drop(guard);
        }
        let perf = run_child(&exe, "perf", "-", &sandbox.join("perf"));
        eprintln!("[perf] {perf}");

        let mut md = String::new();
        md.push_str(&format!(
            "## Recycle Bin spike results\n\nOS: {}\n\n",
            os_version()
        ));
        md.push_str("| Scenario | Approach | Outcome | Op result | Pre-delete flags | Post-delete | Undo |\n");
        md.push_str("|---|---|---|---|---|---|---|\n");
        for r in &rows {
            md.push_str(r);
            md.push('\n');
        }
        md.push_str(&format!(
            "\nPerformance (300 small files): {}\n",
            field(&perf, "perf")
        ));
        println!("{md}");
        if let Some(p) = report_path
            && let Ok(mut f) = fs::OpenOptions::new().write(true).create_new(true).open(&p)
        {
            let _ = f.write_all(md.as_bytes());
        }
        // Remove the sandbox (only our own directory under %TEMP%).
        if sandbox.starts_with(long_temp_dir())
            && sandbox
                .file_name()
                .is_some_and(|n| n.to_string_lossy().starts_with("clearspace-recycle-spike-"))
        {
            let _ = fs::remove_dir_all(&sandbox);
        }
        ExitCode::SUCCESS
    }

    fn long_temp_dir() -> PathBuf {
        let t = std::env::temp_dir();
        match fs::canonicalize(&t) {
            Ok(c) => {
                let s = c.to_string_lossy();
                match s.strip_prefix(r"\\?\") {
                    Some(rest) if !rest.starts_with("UNC") => PathBuf::from(rest),
                    _ => t,
                }
            }
            Err(_) => t,
        }
    }

    fn row(result: &str) -> String {
        [
            field(result, "outcome"),
            field(result, "op"),
            field(result, "pre"),
            field(result, "post"),
            field(result, "undo"),
        ]
        .join(" | ")
    }

    fn field(result: &str, key: &str) -> String {
        result
            .split(';')
            .find_map(|kv| kv.strip_prefix(&format!("{key}=")))
            .unwrap_or("")
            .replace('|', "/")
    }

    fn run_child(exe: &Path, scenario: &str, approach: &str, dir: &Path) -> String {
        let mut child = match Command::new(exe)
            .arg("--child")
            .arg(scenario)
            .arg(approach)
            .arg(dir)
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
        {
            Ok(c) => c,
            Err(e) => return format!("outcome=spawn failed: {e}"),
        };
        let start = Instant::now();
        loop {
            match child.try_wait() {
                Ok(Some(_)) => break,
                Ok(None) if start.elapsed() > CHILD_TIMEOUT => {
                    let _ = child.kill();
                    return "outcome=HUNG (a dialog was probably shown; killed after 60 s)".into();
                }
                Ok(None) => std::thread::sleep(Duration::from_millis(100)),
                Err(e) => return format!("outcome=wait failed: {e}"),
            }
        }
        let mut out = String::new();
        if let Some(mut s) = child.stdout.take() {
            use std::io::Read;
            let _ = s.read_to_string(&mut out);
        }
        out.lines()
            .find_map(|l| l.strip_prefix("RESULT|"))
            .map(str::to_owned)
            .unwrap_or_else(|| format!("outcome=child crashed; output: {}", out.trim()))
    }

    fn os_version() -> String {
        Command::new("cmd")
            .args(["/C", "ver"])
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .unwrap_or_default()
    }

    /// Temporarily sets a per-volume Recycle Bin registry value for the volume holding
    /// `path`, restoring the previous state on drop. CI only.
    struct RegistryGuard {
        key: Vec<u16>,
        name: Vec<u16>,
        previous: Option<u32>,
    }

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(Some(0)).collect()
    }

    impl RegistryGuard {
        fn set(path: &Path, name: &str, value: u32) -> RegistryGuard {
            let drive: String = path.to_string_lossy().chars().take(3).collect(); // "C:\"
            let mut vol = [0u16; 64];
            // SAFETY: NUL-terminated input, fixed output buffer.
            unsafe { GetVolumeNameForVolumeMountPointW(PCWSTR(wide(&drive).as_ptr()), &mut vol) }
                .expect("volume name");
            let vol = String::from_utf16_lossy(&vol);
            let guid = &vol[vol.find('{').unwrap()..=vol.find('}').unwrap()];
            let key = wide(&format!(
                r"Software\Microsoft\Windows\CurrentVersion\Explorer\BitBucket\Volume\{guid}"
            ));
            let name_w = wide(name);
            let mut data = 0u32;
            let mut size = 4u32;
            // SAFETY: valid buffers; reads a DWORD if present.
            let previous = unsafe {
                RegGetValueW(
                    HKEY_CURRENT_USER,
                    PCWSTR(key.as_ptr()),
                    PCWSTR(name_w.as_ptr()),
                    RRF_RT_REG_DWORD,
                    None,
                    Some((&mut data as *mut u32).cast()),
                    Some(&mut size),
                )
            }
            .is_ok()
            .then_some(data);
            write_dword(&key, &name_w, Some(value));
            eprintln!("registry: {name}={value} on {guid} (was {previous:?})");
            RegistryGuard {
                key,
                name: name_w,
                previous,
            }
        }
    }

    impl Drop for RegistryGuard {
        fn drop(&mut self) {
            write_dword(&self.key, &self.name, self.previous);
        }
    }

    fn write_dword(key: &[u16], name: &[u16], value: Option<u32>) {
        let mut h = HKEY::default();
        // SAFETY: valid NUL-terminated strings and out-pointer; key closed below.
        unsafe {
            let rc = RegCreateKeyExW(
                HKEY_CURRENT_USER,
                PCWSTR(key.as_ptr()),
                None,
                PCWSTR::null(),
                REG_OPTION_NON_VOLATILE,
                KEY_READ | KEY_WRITE,
                None,
                &mut h,
                None,
            );
            assert!(rc.is_ok(), "RegCreateKeyExW: {rc:?}");
            match value {
                Some(v) => {
                    let _ = RegSetValueExW(
                        h,
                        PCWSTR(name.as_ptr()),
                        None,
                        REG_DWORD,
                        Some(&v.to_le_bytes()),
                    );
                }
                None => {
                    let _ = RegDeleteValueW(h, PCWSTR(name.as_ptr()));
                }
            }
            let _ = RegCloseKey(h);
        }
    }

    // ---------------------------------------------------------------- child -------------

    fn child(args: &[OsString]) -> ExitCode {
        let scenario = args[0].to_string_lossy().into_owned();
        let approach = args[1].to_string_lossy().into_owned();
        let dir = PathBuf::from(&args[2]);
        // SAFETY: first COM call on this thread.
        let _ = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE) };
        fs::create_dir_all(&dir).expect("child sandbox");
        let result = if scenario == "perf" {
            perf(&dir)
        } else {
            run_scenario(&scenario, &approach, &dir)
        };
        println!("RESULT|{result}");
        purge_our_items(&dir);
        ExitCode::SUCCESS
    }

    fn run_scenario(scenario: &str, approach: &str, dir: &Path) -> String {
        // 1. Create the test item.
        let mut _lock = None;
        let target: PathBuf = match scenario {
            "small-file" | "bin-disabled" | "network-path" => write(dir.join("small.txt"), 1000),
            "over-quota" => write(dir.join("big.bin"), 8 << 20),
            "over-quota-folder" => {
                let f = dir.join("many");
                fs::create_dir_all(&f).unwrap();
                for i in 0..16 {
                    write(f.join(format!("part{i}.bin")), 512 << 10);
                }
                f
            }
            "bin-nearly-full" => {
                // Pre-fill the bin with 3 × 3 MB sandbox files, oldest first.
                for i in 0..3 {
                    let p = write(dir.join(format!("older{i}.bin")), 3 << 20);
                    delete_with(approach, std::slice::from_ref(&p));
                    std::thread::sleep(Duration::from_millis(1100));
                }
                write(dir.join("new.bin"), 5 << 20)
            }
            "folder" => {
                let f = dir.join("folder");
                fs::create_dir_all(f.join("sub/deeper")).unwrap();
                write(f.join("a.txt"), 10);
                write(f.join("sub/deeper/b.txt"), 20);
                f
            }
            "locked" => {
                let p = write(dir.join("locked.txt"), 100);
                _lock = Some(
                    fs::OpenOptions::new()
                        .read(true)
                        .share_mode(0)
                        .open(&p)
                        .unwrap(),
                );
                p
            }
            "readonly" => {
                let p = write(dir.join("readonly.txt"), 100);
                let mut perm = fs::metadata(&p).unwrap().permissions();
                perm.set_readonly(true);
                fs::set_permissions(&p, perm).unwrap();
                p
            }
            "long-path" => {
                let mut d = verbatim(dir);
                for i in 0..8 {
                    d.push(format!("long-folder-name-to-exceed-max-path-{i:02}"));
                }
                fs::create_dir_all(&d).unwrap();
                // Return the plain (non-verbatim) long form; both APIs get this path.
                let p = write(d.join("deep.txt"), 100);
                PathBuf::from(p.to_string_lossy().trim_start_matches(r"\\?\"))
            }
            "unicode" => write(dir.join("Résumé 日本語 😀.txt"), 100),
            other => return format!("outcome=unknown scenario {other}"),
        };
        // The path handed to the delete API.
        let delete_path = if scenario == "network-path" {
            let s = target.to_string_lossy();
            PathBuf::from(format!(r"\\localhost\{}${}", &s[..1], &s[2..]))
        } else {
            target.clone()
        };

        // 2. Delete.
        let start = Instant::now();
        let d = delete_with(approach, std::slice::from_ref(&delete_path));
        let (op, pre, mut post) = (d.op, d.pre, d.post);
        let ms = start.elapsed().as_millis();
        drop(_lock);

        // 3. Observe.
        let exists = exists(&target);
        let bin_item = find_in_bin(&target);
        let outcome = match (exists, bin_item.is_some(), d.bin_items > 0) {
            (true, _, _) => "kept (not deleted)".to_string(),
            (false, true, _) => "recycled".to_string(),
            (false, false, true) => "recycled (per shell; not found in bin listing)".to_string(),
            (false, false, false) => "**PERMANENTLY DELETED**".to_string(),
        };
        if scenario == "bin-nearly-full" {
            let kept = (0..3)
                .filter(|i| find_in_bin(&dir.join(format!("older{i}.bin"))).is_some())
                .count();
            post.push_str(&format!(" · older bin items still in bin: {kept}/3"));
        }
        let undo = match bin_item {
            Some(item) => match trash::os_limited::restore_all([item]) {
                Ok(()) if exists_or_retry(&target) => "restored".to_string(),
                Ok(()) => "restore reported ok but file missing".to_string(),
                Err(e) => format!("restore failed: {e}"),
            },
            None => "–".to_string(),
        };
        format!("outcome={outcome} ({ms} ms);op={op};pre={pre};post={post};undo={undo}")
    }

    fn perf(dir: &Path) -> String {
        let make = |name: &str| -> Vec<PathBuf> {
            let d = dir.join(name);
            fs::create_dir_all(&d).unwrap();
            (0..300)
                .map(|i| write(d.join(format!("f{i}.tmp")), 100))
                .collect()
        };
        let a = make("a");
        let t = Instant::now();
        let ra = trash::delete_all(&a).map_or_else(|e| format!("error {e}"), |_| "ok".into());
        let ta = t.elapsed();

        let b = make("b");
        let t = Instant::now();
        let rb = shell_delete(&b, false).op;
        let tb = t.elapsed();

        let c = make("c");
        let t = Instant::now();
        let mut rc = "ok".to_string();
        for p in &c {
            let r = shell_delete(std::slice::from_ref(p), false).op;
            if r != "ok" {
                rc = r;
            }
        }
        let tc = t.elapsed();
        let gone = |v: &[PathBuf]| v.iter().filter(|p| !exists(p)).count();
        format!(
            "perf=trash::delete_all {} ms ({ra}, {} gone) · IFileOperation batch {} ms ({rb}, {} gone) · IFileOperation one-per-item {} ms ({rc}, {} gone)",
            ta.as_millis(),
            gone(&a),
            tb.as_millis(),
            gone(&b),
            tc.as_millis(),
            gone(&c)
        )
    }

    struct Deleted {
        op: String,
        pre: String,
        post: String,
        /// Items the shell reported as now living in the Recycle Bin (B/B2 only).
        bin_items: usize,
    }

    fn delete_with(approach: &str, paths: &[PathBuf]) -> Deleted {
        match approach {
            "A" => Deleted {
                op: match trash::delete_all(paths) {
                    Ok(()) => "ok".to_string(),
                    Err(e) => format!("error: {e}"),
                },
                pre: "n/a".into(),
                post: "n/a".into(),
                bin_items: 0,
            },
            "B2" => shell_delete(paths, true),
            _ => shell_delete(paths, false),
        }
    }

    fn write(p: PathBuf, size: usize) -> PathBuf {
        fs::write(&p, vec![b'x'; size]).unwrap();
        p
    }

    fn verbatim(p: &Path) -> PathBuf {
        PathBuf::from(format!(r"\\?\{}", p.display()))
    }

    fn exists(p: &Path) -> bool {
        fs::symlink_metadata(verbatim(p)).is_ok()
    }

    fn exists_or_retry(p: &Path) -> bool {
        (0..20).any(|_| {
            let e = exists(p);
            if !e {
                std::thread::sleep(Duration::from_millis(50));
            }
            e
        })
    }

    fn same_path(a: &Path, b: &Path) -> bool {
        let n = |p: &Path| {
            p.to_string_lossy()
                .trim_start_matches(r"\\?\")
                .to_lowercase()
        };
        n(a) == n(b)
    }

    fn find_in_bin(original: &Path) -> Option<trash::TrashItem> {
        trash::os_limited::list()
            .ok()?
            .into_iter()
            .find(|i| same_path(&i.original_path(), original))
    }

    /// Removes from the Recycle Bin only items that originally lived in our sandbox.
    fn purge_our_items(sandbox: &Path) {
        let ours: Vec<_> = trash::os_limited::list()
            .unwrap_or_default()
            .into_iter()
            .filter(|i| {
                let orig = i.original_path().to_string_lossy().to_lowercase();
                orig.starts_with(&sandbox.to_string_lossy().to_lowercase())
                    && orig.contains("clearspace-recycle-spike-")
            })
            .collect();
        if !ours.is_empty() {
            let _ = trash::os_limited::purge_all(ours);
        }
    }

    // ------------------------------------------------------- IFileOperation + sink --------

    #[derive(Default)]
    struct SinkLog {
        pre: Vec<String>,
        post: Vec<String>,
        vetoed: usize,
    }

    #[implement(IFileOperationProgressSink)]
    struct VetoSink {
        log: Arc<Mutex<SinkLog>>,
    }

    fn item_name(item: Option<&IShellItem>) -> String {
        let Some(item) = item else {
            return "NULL".into();
        };
        // SAFETY: GetDisplayName returns a CoTaskMem string we free after copying.
        unsafe {
            match item.GetDisplayName(SIGDN_DESKTOPABSOLUTEPARSING) {
                Ok(p) => {
                    let s = p.to_string().unwrap_or_default();
                    CoTaskMemFree(Some(p.0 as *const _));
                    s
                }
                Err(e) => format!("<{e}>"),
            }
        }
    }

    impl IFileOperationProgressSink_Impl for VetoSink_Impl {
        fn StartOperations(&self) -> windows::core::Result<()> {
            Ok(())
        }
        fn FinishOperations(&self, _hr: HRESULT) -> windows::core::Result<()> {
            Ok(())
        }
        fn PreRenameItem(
            &self,
            _: u32,
            _: Ref<IShellItem>,
            _: &PCWSTR,
        ) -> windows::core::Result<()> {
            Ok(())
        }
        fn PostRenameItem(
            &self,
            _: u32,
            _: Ref<IShellItem>,
            _: &PCWSTR,
            _: HRESULT,
            _: Ref<IShellItem>,
        ) -> windows::core::Result<()> {
            Ok(())
        }
        fn PreMoveItem(
            &self,
            _: u32,
            _: Ref<IShellItem>,
            _: Ref<IShellItem>,
            _: &PCWSTR,
        ) -> windows::core::Result<()> {
            Ok(())
        }
        fn PostMoveItem(
            &self,
            _: u32,
            _: Ref<IShellItem>,
            _: Ref<IShellItem>,
            _: &PCWSTR,
            _: HRESULT,
            _: Ref<IShellItem>,
        ) -> windows::core::Result<()> {
            Ok(())
        }
        fn PreCopyItem(
            &self,
            _: u32,
            _: Ref<IShellItem>,
            _: Ref<IShellItem>,
            _: &PCWSTR,
        ) -> windows::core::Result<()> {
            Ok(())
        }
        fn PostCopyItem(
            &self,
            _: u32,
            _: Ref<IShellItem>,
            _: Ref<IShellItem>,
            _: &PCWSTR,
            _: HRESULT,
            _: Ref<IShellItem>,
        ) -> windows::core::Result<()> {
            Ok(())
        }
        fn PreDeleteItem(&self, flags: u32, item: Ref<IShellItem>) -> windows::core::Result<()> {
            let recycle = flags & TSF_DELETE_RECYCLE_IF_POSSIBLE.0 as u32 != 0;
            let mut log = self.log.lock().unwrap();
            log.pre.push(format!(
                "0x{flags:x}{}",
                if recycle {
                    " (recycle)"
                } else {
                    " (NO recycle → veto)"
                }
            ));
            let _ = item;
            if !recycle {
                log.vetoed += 1;
                // Refuse: Windows would destroy this item instead of recycling it.
                return Err(E_ABORT.into());
            }
            Ok(())
        }
        fn PostDeleteItem(
            &self,
            _flags: u32,
            _item: Ref<IShellItem>,
            hr: HRESULT,
            newly_created: Ref<IShellItem>,
        ) -> windows::core::Result<()> {
            let bin = item_name(newly_created.as_ref());
            self.log
                .lock()
                .unwrap()
                .post
                .push(format!("hr=0x{:08x} bin-item={bin}", hr.0 as u32));
            Ok(())
        }
        fn PreNewItem(&self, _: u32, _: Ref<IShellItem>, _: &PCWSTR) -> windows::core::Result<()> {
            Ok(())
        }
        fn PostNewItem(
            &self,
            _: u32,
            _: Ref<IShellItem>,
            _: &PCWSTR,
            _: &PCWSTR,
            _: u32,
            _: HRESULT,
            _: Ref<IShellItem>,
        ) -> windows::core::Result<()> {
            Ok(())
        }
        fn UpdateProgress(&self, _: u32, _: u32) -> windows::core::Result<()> {
            Ok(())
        }
        fn ResetTimer(&self) -> windows::core::Result<()> {
            Ok(())
        }
        fn PauseTimer(&self) -> windows::core::Result<()> {
            Ok(())
        }
        fn ResumeTimer(&self) -> windows::core::Result<()> {
            Ok(())
        }
    }

    /// Approaches B / B2 (B2 adds FOF_WANTNUKEWARNING).
    fn shell_delete(paths: &[PathBuf], nuke_warning: bool) -> Deleted {
        let log = Arc::new(Mutex::new(SinkLog::default()));
        let r: windows::core::Result<bool> = (|| {
            // SAFETY: COM is initialised (STA) on this thread; all interface pointers are
            // owned by windows-rs smart pointers; strings are NUL-terminated and outlive calls.
            unsafe {
                let op: IFileOperation = CoCreateInstance(&FileOperation, None, CLSCTX_ALL)?;
                let mut flags = FOF_SILENT
                    | FOF_NOCONFIRMATION
                    | FOF_NOERRORUI
                    | FOF_NOCONFIRMMKDIR
                    | FOF_ALLOWUNDO
                    | FOFX_RECYCLEONDELETE;
                if nuke_warning {
                    flags |= FOF_WANTNUKEWARNING;
                }
                op.SetOperationFlags(flags)?;
                let sink: IFileOperationProgressSink = VetoSink {
                    log: Arc::clone(&log),
                }
                .into();
                let cookie = op.Advise(&sink)?;
                for p in paths {
                    let w: Vec<u16> = p.as_os_str().encode_wide().chain(Some(0)).collect();
                    let item: IShellItem = SHCreateItemFromParsingName(PCWSTR(w.as_ptr()), None)?;
                    op.DeleteItem(&item, None)?;
                }
                let perform = op.PerformOperations();
                let aborted = op.GetAnyOperationsAborted()?.as_bool();
                let _ = op.Unadvise(cookie);
                perform?;
                Ok(aborted)
            }
        })();
        let log = log.lock().unwrap();
        let op = match r {
            Ok(false) => "ok".to_string(),
            Ok(true) => format!("aborted ({} vetoed)", log.vetoed),
            Err(e) => format!("error 0x{:08x} {}", e.code().0 as u32, e.message()),
        };
        let pre = if log.pre.is_empty() {
            "–".into()
        } else {
            summarize(&log.pre)
        };
        let post = if log.post.is_empty() {
            "–".into()
        } else {
            summarize(&log.post)
        };
        let bin_items = log
            .post
            .iter()
            .filter(|p| !p.ends_with("bin-item=NULL"))
            .count();
        Deleted {
            op,
            pre,
            post,
            bin_items,
        }
    }

    fn summarize(v: &[String]) -> String {
        if v.len() <= 2 {
            v.join(", ")
        } else {
            format!("{} … ({} items)", v[0], v.len())
        }
    }
}
