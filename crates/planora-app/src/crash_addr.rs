//! Rücksprungadressen für den Absturzbericht (nur Windows).
//!
//! Die Zeilentabellen eines Release-Baus stehen unter Windows in der `.pdb`, die nicht mit
//! ausgeliefert wird; der Ablauf (Backtrace) beim Tester zeigt deshalb nur `<unknown>`. Hier
//! stehen stattdessen die Rücksprungadressen als Versatz zur eigenen exe
//! (`planora-app.exe+0x1a2b3c`) und die Bau-Kennung (PDB-GUID und Age aus dem CodeView-Eintrag,
//! dazu der PE-Zeitstempel). `tools/symbolize.ps1` löst sie mit der passenden `.pdb` des
//! Releases zu Funktion, Datei und Zeile auf.
//!
//! Alles hier läuft im Panik-Hook: kein Indexing ohne Prüfung, höchstens [`MAX_FRAMES`] Rahmen.

/// Höchstens so viele Rahmen werden erfasst und gezeigt (`RtlCaptureStackBackTrace` verlangt
/// auf alten Windows-Versionen weniger als 63).
pub const MAX_FRAMES: usize = 62;
/// Überschrift des Abschnitts im Bericht.
pub const SECTION: &str = "Addresses (for Dennis):";

/// Bau-Kennung der exe.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BuildId {
    /// `TimeDateStamp` aus dem PE-Kopf.
    pub time_stamp: u32,
    /// GUID der passenden `.pdb` (`12345678-9ABC-…`), falls es einen CodeView-Eintrag gibt.
    pub pdb_guid: Option<String>,
    /// Age der passenden `.pdb`.
    pub pdb_age: Option<u32>,
}

/// Ein Rahmen: Versatz in einem Modul, oder ohne Modul die volle Adresse.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Frame {
    pub module: Option<String>,
    pub offset: usize,
}

/// Alles für den Abschnitt [`SECTION`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Addresses {
    /// Dateiname der eigenen exe.
    pub module: String,
    pub build: Option<BuildId>,
    pub frames: Vec<Frame>,
}

const UNAVAILABLE: &str = "not available";
/// Höchstens so viele Debugeinträge werden durchsucht.
const MAX_DEBUG_ENTRIES: usize = 32;
const DEBUG_ENTRY_SIZE: usize = 28;
const IMAGE_DEBUG_TYPE_CODEVIEW: u32 = 2;

fn u16_at(b: &[u8], at: usize) -> Option<u16> {
    let s = b.get(at..at.checked_add(2)?)?;
    Some(u16::from_le_bytes(s.try_into().ok()?))
}

fn u32_at(b: &[u8], at: usize) -> Option<u32> {
    let s = b.get(at..at.checked_add(4)?)?;
    Some(u32::from_le_bytes(s.try_into().ok()?))
}

/// Versatz des optionalen Kopfs und ob es PE32+ ist.
fn optional_header(b: &[u8]) -> Option<(usize, bool)> {
    if b.get(0..2)? != b"MZ" {
        return None;
    }
    let pe = u32_at(b, 0x3c)? as usize;
    if b.get(pe..pe.checked_add(4)?)? != b"PE\0\0" {
        return None;
    }
    let opt = pe.checked_add(24)?;
    match u16_at(b, opt)? {
        0x20b => Some((opt, true)),
        0x10b => Some((opt, false)),
        _ => None,
    }
}

/// `SizeOfImage` aus dem PE-Kopf (reicht schon mit den Köpfen allein).
pub fn size_of_image(b: &[u8]) -> Option<usize> {
    let (opt, _) = optional_header(b)?;
    Some(u32_at(b, opt.checked_add(56)?)? as usize)
}

/// Die Bau-Kennung aus einem geladenen (gemappten) PE-Abbild: Versätze sind RVAs.
pub fn pe_build_id(b: &[u8]) -> Option<BuildId> {
    let (opt, plus) = optional_header(b)?;
    let pe = opt.checked_sub(24)?;
    let time_stamp = u32_at(b, pe.checked_add(8)?)?;
    let (count_at, dirs_at) = if plus { (108, 112) } else { (92, 96) };
    let mut id = BuildId {
        time_stamp,
        pdb_guid: None,
        pdb_age: None,
    };
    let count = u32_at(b, opt.checked_add(count_at)?)?;
    if count <= 6 {
        return Some(id);
    }
    let dir = opt.checked_add(dirs_at)?.checked_add(6 * 8)?;
    let (Some(rva), Some(size)) = (u32_at(b, dir), u32_at(b, dir.checked_add(4)?)) else {
        return Some(id);
    };
    let entries = (size as usize / DEBUG_ENTRY_SIZE).min(MAX_DEBUG_ENTRIES);
    for i in 0..entries {
        let Some(e) = i
            .checked_mul(DEBUG_ENTRY_SIZE)
            .and_then(|o| (rva as usize).checked_add(o))
        else {
            break;
        };
        if u32_at(b, e.saturating_add(12)) != Some(IMAGE_DEBUG_TYPE_CODEVIEW) {
            continue;
        }
        let Some(data) = u32_at(b, e.saturating_add(20)) else {
            continue;
        };
        let data = data as usize;
        let Some(guid) = data
            .checked_add(4)
            .and_then(|s| b.get(data..s))
            .filter(|m| *m == b"RSDS")
            .and_then(|_| b.get(data.checked_add(4)?..data.checked_add(20)?))
        else {
            continue;
        };
        id.pdb_guid = format_guid(guid);
        id.pdb_age = data.checked_add(20).and_then(|a| u32_at(b, a));
        break;
    }
    Some(id)
}

/// GUID im üblichen Format (die ersten drei Teile kleinendig).
fn format_guid(g: &[u8]) -> Option<String> {
    let d1 = u32_at(g, 0)?;
    let d2 = u16_at(g, 4)?;
    let d3 = u16_at(g, 6)?;
    let r = g.get(8..16)?;
    let mut s = format!("{d1:08X}-{d2:04X}-{d3:04X}-");
    for (i, c) in r.iter().enumerate() {
        if i == 2 {
            s.push('-');
        }
        s.push_str(&format!("{c:02X}"));
    }
    Some(s)
}

/// Der Abschnitt [`SECTION`] für den Bericht.
pub fn section_text(a: Option<&Addresses>) -> String {
    let mut t = format!("{SECTION}\n");
    let Some(a) = a else {
        t.push_str(UNAVAILABLE);
        t.push('\n');
        return t;
    };
    t.push_str(&format!("Module: {}\n", a.module));
    let guid = a.build.as_ref().and_then(|b| b.pdb_guid.as_deref());
    t.push_str(&format!("PDB-GUID: {}\n", guid.unwrap_or(UNAVAILABLE)));
    if let Some(age) = a.build.as_ref().and_then(|b| b.pdb_age) {
        t.push_str(&format!("PDB-Age: {age}\n"));
    }
    match &a.build {
        Some(b) => t.push_str(&format!("PE-Time: 0x{:08X}\n", b.time_stamp)),
        None => t.push_str(&format!("PE-Time: {UNAVAILABLE}\n")),
    }
    if a.frames.is_empty() {
        t.push_str("no frames\n");
    }
    for (i, f) in a.frames.iter().take(MAX_FRAMES).enumerate() {
        match &f.module {
            Some(m) => t.push_str(&format!("{i:3}: {m}+0x{:x}\n", f.offset)),
            None => t.push_str(&format!("{i:3}: unknown 0x{:x}\n", f.offset)),
        }
    }
    t
}

/// Erfasst die Rücksprungadressen der aktuellen Aufrufkette und die Bau-Kennung der eigenen
/// exe. `None`, wenn das nicht geht (und immer außerhalb von Windows).
#[cfg(not(windows))]
pub fn capture() -> Option<Addresses> {
    None
}

/// Erfasst die Rücksprungadressen der aktuellen Aufrufkette und die Bau-Kennung der eigenen
/// exe. `None`, wenn das nicht geht (und immer außerhalb von Windows).
#[cfg(windows)]
#[allow(unsafe_code)]
pub fn capture() -> Option<Addresses> {
    use std::ffi::c_void;
    use windows_sys::Win32::System::Diagnostics::Debug::RtlCaptureStackBackTrace;
    use windows_sys::Win32::System::LibraryLoader::{
        GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS, GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
        GetModuleHandleExW, GetModuleHandleW,
    };

    /// Kleinste Seitengröße; die Köpfe eines geladenen Moduls liegen immer in der ersten Seite.
    const PAGE: usize = 0x1000;
    /// Größer ist keine exe von uns; schützt vor Unsinn im Kopf.
    const MAX_IMAGE: usize = 1 << 30;

    // SAFETY: Ein Nullzeiger fragt nach dem Modul der laufenden exe; nur Lesen.
    let base = unsafe { GetModuleHandleW(std::ptr::null()) } as usize;
    if base == 0 {
        return None;
    }
    // SAFETY: Die erste Seite eines geladenen Moduls (DOS- und PE-Kopf) ist immer lesbar
    // eingeblendet, solange das Modul geladen ist – die exe bleibt es bis zum Ende.
    let headers = unsafe { std::slice::from_raw_parts(base as *const u8, PAGE) };
    let size = size_of_image(headers).filter(|s| (PAGE..=MAX_IMAGE).contains(s))?;
    // SAFETY: Das ganze Abbild (`SizeOfImage` ab der Basis) ist eingeblendet; gelesen werden
    // nur Kopf, Debugverzeichnis und CodeView-Eintrag (alles in lesbaren Abschnitten).
    let image = unsafe { std::slice::from_raw_parts(base as *const u8, size) };
    let build = pe_build_id(image);
    let module = module_file_name(base as *mut c_void).unwrap_or_else(|| "exe".to_string());

    let mut raw = [std::ptr::null_mut::<c_void>(); MAX_FRAMES];
    // SAFETY: `raw` bietet Platz für genau `MAX_FRAMES` Zeiger; der Hash wird nicht verlangt.
    let n = unsafe {
        RtlCaptureStackBackTrace(0, MAX_FRAMES as u32, raw.as_mut_ptr(), std::ptr::null_mut())
    } as usize;
    let mut frames = Vec::with_capacity(MAX_FRAMES);
    for &p in raw.iter().take(n.min(MAX_FRAMES)) {
        let a = p as usize;
        if a >= base && a - base < size {
            frames.push(Frame {
                module: Some(module.clone()),
                offset: a - base,
            });
            continue;
        }
        let mut h: *mut c_void = std::ptr::null_mut();
        // SAFETY: Mit FROM_ADDRESS ist der „Name“ eine Adresse; UNCHANGED_REFCOUNT hält das
        // Modul nicht fest. `h` ist ein gültiges Ziel.
        let ok = unsafe {
            GetModuleHandleExW(
                GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS
                    | GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
                p as *const u16,
                &mut h,
            )
        } != 0;
        let other = h as usize;
        frames.push(match module_file_name(h) {
            Some(m) if ok && other != 0 && a >= other => Frame {
                module: Some(m),
                offset: a - other,
            },
            _ => Frame {
                module: None,
                offset: a,
            },
        });
    }
    Some(Addresses {
        module,
        build,
        frames,
    })
}

/// Dateiname (ohne Ordner) des Moduls `h`.
#[cfg(windows)]
#[allow(unsafe_code)]
fn module_file_name(h: *mut std::ffi::c_void) -> Option<String> {
    use windows_sys::Win32::System::LibraryLoader::GetModuleFileNameW;
    if h.is_null() {
        return None;
    }
    let mut buf = [0u16; 1024];
    // SAFETY: `buf` hat genau die übergebene Länge; die Funktion schreibt höchstens so viel.
    let n = unsafe { GetModuleFileNameW(h, buf.as_mut_ptr(), buf.len() as u32) } as usize;
    let path = String::from_utf16_lossy(buf.get(..n)?);
    let name = path.rsplit(['\\', '/']).next()?.to_string();
    (!name.is_empty()).then_some(name)
}
