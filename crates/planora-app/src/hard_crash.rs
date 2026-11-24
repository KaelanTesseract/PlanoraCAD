//! Bericht bei harten Abstürzen (nur Windows): Zugriffsfehler, Stapelüberlauf und andere
//! Ausnahmen des Betriebssystems gehen am Panik-Hook ([`crate::crash`]) vorbei. Ein Filter für
//! nicht behandelte Ausnahmen (`SetUnhandledExceptionFilter`) schreibt dann einen kurzen Bericht
//! im selben Format nach `abstuerze/`, damit der nächste Start ihn findet und die Rückmeldung ihn
//! anhängen kann. Danach geht die Ausnahme weiter an Windows (Fehlerberichterstattung, Ende).
//!
//! Im Filter selbst geschieht fast nichts: Code, Adresse und Rücksprungadressen in feste
//! Speicherplätze, dann weckt er einen beim Start angelegten Schreib-Thread und wartet höchstens
//! [`WRITE_TIMEOUT`] auf ihn. So reicht der Rest des Stapels auch nach einem Stapelüberlauf; alles,
//! was Speicher braucht (Text, Datei), läuft im Schreib-Thread mit eigenem Stapel.
//!
//! Ein Ende von außen (Taskmanager, Abschuss des Prozessbaums) und `abort`/`__fastfail` gehen an
//! jedem Filter vorbei; dafür zeigt der nächste Start den Hinweis mit der Mitschrift aus dem
//! Laufmarker ([`crate::crash::start_run`]). Läuft ein Debugger, bekommt er die Ausnahme, und der Filter
//! schweigt. Funktionsnamen zu den Rücksprungadressen liefert `tools/symbolize.ps1` mit der
//! `.pdb` des Releases.

/// Höchstens so lange wartet der Filter auf den Bericht.
pub const WRITE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);

/// Name der Windows-Ausnahme `code` für den Bericht (englisch wie der ganze Bericht).
pub fn exception_name(code: u32) -> &'static str {
    match code {
        0xC000_0005 => "access violation",
        0xC000_00FD => "stack overflow",
        0xC000_0094 => "integer division by zero",
        0xC000_0095 => "integer overflow",
        0xC000_001D => "illegal instruction",
        0xC000_0096 => "privileged instruction",
        0xC000_0006 => "in-page error",
        0xC000_008C => "array bounds exceeded",
        0xC000_0409 => "stack buffer overrun",
        0x8000_0003 => "breakpoint",
        _ => "unknown exception",
    }
}

/// Zeile „Error“ des Berichts zur Ausnahme `code`.
pub fn message(code: u32) -> String {
    format!("Windows exception 0x{code:08X} ({})", exception_name(code))
}

#[cfg(not(windows))]
pub fn install(_dir: std::path::PathBuf, _mask: crate::crash::UserMask) {}

#[cfg(windows)]
pub use imp::install;

#[cfg(windows)]
#[allow(unsafe_code)]
mod imp {
    use super::{WRITE_TIMEOUT, message};
    use crate::crash::{self, ReportInput, UserMask};
    use crate::crash_addr::{self, MAX_FRAMES};
    use std::path::PathBuf;
    use std::sync::OnceLock;
    use std::sync::atomic::{AtomicU8, AtomicU32, AtomicUsize, Ordering};
    use windows_sys::Win32::System::Diagnostics::Debug::{
        EXCEPTION_POINTERS, SetUnhandledExceptionFilter,
    };

    /// Noch kein Absturz.
    const IDLE: u8 = 0;
    /// Der Filter legt gerade die Angaben ab.
    const FILLING: u8 = 3;
    /// Der Filter hat die Angaben abgelegt; der Schreib-Thread ist dran.
    const REQUESTED: u8 = 1;
    /// Der Bericht ist geschrieben (oder es ging nicht).
    const DONE: u8 = 2;
    /// `EXCEPTION_CONTINUE_SEARCH`: Windows behandelt die Ausnahme weiter wie ohne Filter.
    const CONTINUE_SEARCH: i32 = 0;

    static STATE: AtomicU8 = AtomicU8::new(IDLE);
    static CODE: AtomicU32 = AtomicU32::new(0);
    static ADDRESS: AtomicUsize = AtomicUsize::new(0);
    static COUNT: AtomicUsize = AtomicUsize::new(0);
    static FRAMES: [AtomicUsize; MAX_FRAMES] = [const { AtomicUsize::new(0) }; MAX_FRAMES];
    static WRITER: OnceLock<std::thread::Thread> = OnceLock::new();

    /// Legt den Schreib-Thread an und setzt den Filter (einmal je Prozess; ein zweiter Aufruf
    /// tut nichts). `dir` ist der Ordner der Berichte, `mask` ersetzt den Benutzerordner.
    pub fn install(dir: PathBuf, mask: UserMask) {
        if WRITER.get().is_some() {
            return;
        }
        let spawned = std::thread::Builder::new()
            .name("planora-crash-writer".into())
            .spawn(move || {
                loop {
                    std::thread::park();
                    if STATE.load(Ordering::Acquire) == REQUESTED {
                        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                            let _ = crash::write_report(&dir, &collect(), &mask);
                        }));
                        STATE.store(DONE, Ordering::Release);
                        return;
                    }
                }
            });
        let Ok(handle) = spawned else {
            return;
        };
        let _ = WRITER.set(handle.thread().clone());
        // SAFETY: `filter` hat die verlangte Form und bleibt für die ganze Laufzeit gültig.
        unsafe {
            SetUnhandledExceptionFilter(Some(filter));
        }
    }

    /// Der Filter: legt die Angaben ab, weckt den Schreib-Thread, wartet kurz, gibt die
    /// Ausnahme weiter. Kein Speicher, kaum Stapel.
    unsafe extern "system" fn filter(info: *const EXCEPTION_POINTERS) -> i32 {
        if STATE
            .compare_exchange(IDLE, FILLING, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return CONTINUE_SEARCH;
        }
        // SAFETY: Windows übergibt gültige Zeiger auf den Ausnahmedatensatz (oder null).
        let record = unsafe { info.as_ref().and_then(|i| i.ExceptionRecord.as_ref()) };
        if let Some(r) = record {
            CODE.store(r.ExceptionCode as u32, Ordering::Relaxed);
            ADDRESS.store(r.ExceptionAddress as usize, Ordering::Relaxed);
        }
        let mut raw = [0usize; MAX_FRAMES];
        let n = crash_addr::capture_raw(&mut raw);
        for (slot, a) in FRAMES.iter().zip(raw.iter()).take(n) {
            slot.store(*a, Ordering::Relaxed);
        }
        COUNT.store(n, Ordering::Relaxed);
        STATE.store(REQUESTED, Ordering::Release);
        if let Some(w) = WRITER.get() {
            w.unpark();
            let start = std::time::Instant::now();
            while STATE.load(Ordering::Acquire) != DONE && start.elapsed() < WRITE_TIMEOUT {
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
        }
        CONTINUE_SEARCH
    }

    /// Die Angaben für den Bericht (im Schreib-Thread).
    fn collect() -> ReportInput {
        let code = CODE.load(Ordering::Relaxed);
        let address = ADDRESS.load(Ordering::Relaxed);
        let n = COUNT.load(Ordering::Relaxed).min(MAX_FRAMES);
        let raw: Vec<usize> = FRAMES
            .iter()
            .take(n)
            .map(|a| a.load(Ordering::Relaxed))
            .collect();
        // Die Stelle der Ausnahme als Versatz im Modul, wie die Rahmen.
        let location = crash_addr::from_raw(&[address])
            .and_then(|a| a.frames.into_iter().next())
            .map_or_else(
                || format!("0x{address:x}"),
                |f| match f.module {
                    Some(m) => format!("{m}+0x{:x}", f.offset),
                    None => format!("0x{:x}", f.offset),
                },
            );
        let (steps, log, document) = crash::diag_snapshot();
        ReportInput {
            version: crate::RUNNING_VERSION.to_string(),
            message: message(code),
            location,
            thread: "<faulting thread>".to_string(),
            backtrace: "not available (hard crash, see addresses)".to_string(),
            addresses: crash_addr::from_raw(&raw),
            steps,
            log,
            document,
            time: crate::local_time::now_local(),
        }
    }
}
