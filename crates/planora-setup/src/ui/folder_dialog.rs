//! Ordnerauswahl „Durchsuchen“: der Windows-Dialog `IFileOpenDialog` mit `FOS_PICKFOLDERS`,
//! modal über dem Fenster der Hülle (läuft im Oberflächen-Faden, solange er offen ist).
//! windows-sys bringt keine COM-Schnittstellen mit; die wenigen gebrauchten Einträge der
//! Methodentabellen stehen deshalb unten von Hand, in der Reihenfolge aus `shobjidl_core.h`.

use std::path::{Path, PathBuf};

/// Nächster vorhandener Ordner auf dem Weg von `dir` nach oben (Startordner des Dialogs; der
/// vorgeschlagene Ordner gibt es vor der Installation meist noch nicht).
#[cfg_attr(
    not(any(windows, test)),
    expect(dead_code, reason = "nur unter Windows gebraucht")
)]
pub fn existing_ancestor(dir: &Path) -> Option<PathBuf> {
    dir.ancestors().find(|d| d.is_dir()).map(Path::to_path_buf)
}

/// Öffnet den Ordnerdialog, beginnend bei `start` (oder dem nächsten vorhandenen Ordner darüber).
/// `None`, wenn abgebrochen wurde oder der Dialog nicht verfügbar ist.
#[cfg(windows)]
pub fn pick(start: &Path) -> Option<PathBuf> {
    win::pick(start)
}

/// Außerhalb von Windows gibt es keinen Dialog (der Knopf ist dort gesperrt).
#[cfg(not(windows))]
pub fn pick(start: &Path) -> Option<PathBuf> {
    let _ = start;
    None
}

#[cfg(windows)]
mod win {
    use std::ffi::c_void;
    use std::os::windows::ffi::OsStrExt;
    use std::path::{Path, PathBuf};
    use windows_sys::Win32::Foundation::HWND;
    use windows_sys::Win32::System::Com::{
        CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx,
        CoTaskMemFree, CoUninitialize,
    };
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::GetActiveWindow;
    use windows_sys::Win32::UI::Shell::{
        FOS_FORCEFILESYSTEM, FOS_PATHMUSTEXIST, FOS_PICKFOLDERS, FileOpenDialog,
        SHCreateItemFromParsingName, SIGDN, SIGDN_FILESYSPATH,
    };
    use windows_sys::core::{GUID, HRESULT, PWSTR};

    /// `IID_IFileOpenDialog`.
    const IID_IFILEOPENDIALOG: GUID = GUID::from_u128(0xd57c7288_d4ad_4768_be02_9d969532d960);
    /// `IID_IShellItem`.
    const IID_ISHELLITEM: GUID = GUID::from_u128(0x43826d1e_e718_42ee_bc55_a1e261c37bfe);

    /// Anfang jeder Methodentabelle (IUnknown): QueryInterface, AddRef, Release.
    #[repr(C)]
    struct UnknownVtbl {
        _query_interface_add_ref: [usize; 2],
        release: unsafe extern "system" fn(*mut c_void) -> u32,
    }

    /// Methodentabelle von `IFileOpenDialog` bis `GetResult` (IUnknown, IModalWindow,
    /// IFileDialog). Nicht gebrauchte Einträge sind nur Platzhalter gleicher Größe.
    #[repr(C)]
    struct FileDialogVtbl {
        _unknown: [usize; 3],
        show: unsafe extern "system" fn(*mut c_void, HWND) -> HRESULT,
        /// SetFileTypes, SetFileTypeIndex, GetFileTypeIndex, Advise, Unadvise.
        _types_and_events: [usize; 5],
        set_options: unsafe extern "system" fn(*mut c_void, u32) -> HRESULT,
        get_options: unsafe extern "system" fn(*mut c_void, *mut u32) -> HRESULT,
        _set_default_folder: usize,
        set_folder: unsafe extern "system" fn(*mut c_void, *mut c_void) -> HRESULT,
        /// GetFolder, GetCurrentSelection, SetFileName, GetFileName, SetTitle,
        /// SetOkButtonLabel, SetFileNameLabel.
        _names_and_labels: [usize; 7],
        get_result: unsafe extern "system" fn(*mut c_void, *mut *mut c_void) -> HRESULT,
    }

    /// Methodentabelle von `IShellItem` bis `GetDisplayName`.
    #[repr(C)]
    struct ShellItemVtbl {
        _unknown: [usize; 3],
        /// BindToHandler, GetParent.
        _bind_and_parent: [usize; 2],
        get_display_name: unsafe extern "system" fn(*mut c_void, SIGDN, *mut PWSTR) -> HRESULT,
    }

    /// COM-Zeiger, der beim Verwerfen `Release` aufruft.
    struct Com<V> {
        ptr: *mut c_void,
        _vtbl: std::marker::PhantomData<V>,
    }

    impl<V> Com<V> {
        fn new(ptr: *mut c_void) -> Option<Com<V>> {
            (!ptr.is_null()).then_some(Com {
                ptr,
                _vtbl: std::marker::PhantomData,
            })
        }

        fn vtbl(&self) -> &V {
            // SAFETY: `ptr` ist ein gültiger COM-Zeiger der Schnittstelle zu `V`; sein erstes
            // Feld zeigt auf die Methodentabelle, deren Anfang `V` beschreibt.
            unsafe { &**(self.ptr as *mut *const V) }
        }
    }

    impl<V> Drop for Com<V> {
        fn drop(&mut self) {
            // SAFETY: Jede COM-Schnittstelle beginnt mit IUnknown; `Release` ist der dritte
            // Eintrag. Der Zeiger gehört nur dieser Struktur und wird genau einmal freigegeben.
            unsafe {
                let vtbl = *(self.ptr as *const *const UnknownVtbl);
                ((*vtbl).release)(self.ptr);
            }
        }
    }

    fn wide(path: &Path) -> Vec<u16> {
        path.as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect()
    }

    pub(super) fn pick(start: &Path) -> Option<PathBuf> {
        // SAFETY: COM für diesen Faden (Single-Threaded Apartment, wie winit es ohnehin
        // einrichtet). Nur bei Erfolg (auch „schon eingerichtet“) wird es wieder abgemeldet.
        let init = unsafe { CoInitializeEx(std::ptr::null(), COINIT_APARTMENTTHREADED as u32) };
        let result = show(start);
        if init >= 0 {
            // SAFETY: Gegenstück zum erfolgreichen CoInitializeEx oben.
            unsafe { CoUninitialize() };
        }
        result
    }

    fn show(start: &Path) -> Option<PathBuf> {
        let mut raw = std::ptr::null_mut();
        // SAFETY: Klassen- und Schnittstellen-Id sind gültig; `raw` bekommt den Zeiger.
        let hr = unsafe {
            CoCreateInstance(
                &FileOpenDialog,
                std::ptr::null_mut(),
                CLSCTX_INPROC_SERVER,
                &IID_IFILEOPENDIALOG,
                &mut raw,
            )
        };
        if hr < 0 {
            return None;
        }
        let dialog: Com<FileDialogVtbl> = Com::new(raw)?;
        let v = dialog.vtbl();
        let mut options = 0u32;
        // SAFETY: gültiger Dialog; `options` ist ein gültiger Zeiger.
        unsafe { (v.get_options)(dialog.ptr, &mut options) };
        // SAFETY: wie oben.
        unsafe {
            (v.set_options)(
                dialog.ptr,
                options | FOS_PICKFOLDERS | FOS_FORCEFILESYSTEM | FOS_PATHMUSTEXIST,
            )
        };
        if let Some(folder) = super::existing_ancestor(start) {
            let path = wide(&folder);
            let mut item = std::ptr::null_mut();
            // SAFETY: `path` ist nullterminiert; `item` bekommt einen IShellItem-Zeiger.
            let hr = unsafe {
                SHCreateItemFromParsingName(
                    path.as_ptr(),
                    std::ptr::null_mut(),
                    &IID_ISHELLITEM,
                    &mut item,
                )
            };
            if hr >= 0
                && let Some(item) = Com::<ShellItemVtbl>::new(item)
            {
                // SAFETY: gültiger Dialog und gültiges IShellItem (der Dialog hält eine eigene
                // Referenz).
                unsafe { (v.set_folder)(dialog.ptr, item.ptr) };
            }
        }
        // SAFETY: Liest nur das aktive Fenster dieses Fadens (das Fenster der Hülle).
        let owner = unsafe { GetActiveWindow() };
        // SAFETY: gültiger Dialog; `owner` ist ein Fenster oder null. Abbruch liefert einen
        // Fehlercode.
        if unsafe { (v.show)(dialog.ptr, owner) } < 0 {
            return None;
        }
        let mut item = std::ptr::null_mut();
        // SAFETY: gültiger Dialog nach erfolgreichem Show.
        if unsafe { (v.get_result)(dialog.ptr, &mut item) } < 0 {
            return None;
        }
        let item: Com<ShellItemVtbl> = Com::new(item)?;
        let mut name: PWSTR = std::ptr::null_mut();
        // SAFETY: gültiges IShellItem; `name` bekommt eine mit CoTaskMemAlloc belegte Zeichenkette.
        let hr = unsafe { (item.vtbl().get_display_name)(item.ptr, SIGDN_FILESYSPATH, &mut name) };
        if hr < 0 || name.is_null() {
            return None;
        }
        let mut len = 0;
        // SAFETY: `name` ist nullterminiert; gelesen wird nur bis zur Null, danach freigegeben.
        let path = unsafe {
            while *name.add(len) != 0 {
                len += 1;
            }
            let s = String::from_utf16_lossy(std::slice::from_raw_parts(name, len));
            CoTaskMemFree(name as *const c_void);
            s
        };
        Some(PathBuf::from(path))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn start_folder_walks_up_to_an_existing_one() {
        let tmp = std::env::temp_dir();
        let missing = tmp.join("planora-setup-gibt-es-nicht").join("tiefer");
        assert_eq!(existing_ancestor(&missing), Some(tmp.clone()));
        assert_eq!(existing_ancestor(&tmp), Some(tmp));
    }
}
