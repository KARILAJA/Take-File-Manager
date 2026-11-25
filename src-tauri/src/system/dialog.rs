use std::path::PathBuf;

#[cfg(windows)]
fn to_wide_null(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Opens the native modern Windows Explorer folder picker (`IFileOpenDialog` with `FOS_PICKFOLDERS`).
/// Runs completely in-process on an STA COM thread with zero console/CMD/PowerShell windows.
pub fn pick_folder_native(
    owner_hwnd: Option<isize>,
    initial_dir: Option<&str>,
    title: &str,
) -> Option<String> {
    #[cfg(windows)]
    {
        use windows::core::PCWSTR;
        use windows::Win32::Foundation::{HWND, RPC_E_CHANGED_MODE};
        use windows::Win32::System::Com::{
            CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize, CLSCTX_ALL,
            COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE,
        };
        use windows::Win32::UI::Shell::{
            FileOpenDialog, IFileOpenDialog, IShellItem, SHCreateItemFromParsingName,
            FOS_FORCEFILESYSTEM, FOS_PATHMUSTEXIST, FOS_PICKFOLDERS, SIGDN_FILESYSPATH,
        };

        unsafe {
            let hr = CoInitializeEx(None, COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE);
            let should_uninit = hr.is_ok();
            if hr.is_err() && hr != RPC_E_CHANGED_MODE {
                return None;
            }

            struct ComGuard(bool);
            impl Drop for ComGuard {
                fn drop(&mut self) {
                    if self.0 {
                        unsafe { CoUninitialize() };
                    }
                }
            }
            let _guard = ComGuard(should_uninit);

            let dialog: IFileOpenDialog =
                CoCreateInstance(&FileOpenDialog, None, CLSCTX_ALL).ok()?;

            let opts = dialog.GetOptions().ok()?;
            dialog
                .SetOptions(opts | FOS_PICKFOLDERS | FOS_FORCEFILESYSTEM | FOS_PATHMUSTEXIST)
                .ok()?;

            let wide_title = to_wide_null(title);
            let _ = dialog.SetTitle(PCWSTR::from_raw(wide_title.as_ptr()));

            if let Some(dir) = initial_dir.map(str::trim).filter(|s| !s.is_empty()) {
                let normalized = dir.replace('/', "\\");
                if PathBuf::from(&normalized).exists() {
                    let wide_dir = to_wide_null(&normalized);
                    if let Ok(item) = SHCreateItemFromParsingName::<_, _, IShellItem>(
                        PCWSTR::from_raw(wide_dir.as_ptr()),
                        None,
                    ) {
                        let _ = dialog.SetFolder(&item);
                    }
                }
            }

            let hwnd = owner_hwnd
                .filter(|&h| h != 0)
                .map(|h| HWND(h as *mut core::ffi::c_void));

            if dialog.Show(hwnd).is_err() {
                return None;
            }

            let result_item = dialog.GetResult().ok()?;
            let display_ptr = result_item.GetDisplayName(SIGDN_FILESYSPATH).ok()?;
            let selected = display_ptr.to_string().ok();
            CoTaskMemFree(Some(display_ptr.0 as *const core::ffi::c_void));
            selected.filter(|s| !s.trim().is_empty())
        }
    }
    #[cfg(not(windows))]
    {
        let _ = (owner_hwnd, title);
        initial_dir.map(|s| s.to_string())
    }
}

/// Opens the native modern Windows Explorer file picker (`IFileOpenDialog`).
pub fn pick_file_native(
    owner_hwnd: Option<isize>,
    title: &str,
    filters: &[(&str, &str)],
) -> Option<String> {
    #[cfg(windows)]
    {
        use windows::core::PCWSTR;
        use windows::Win32::Foundation::{HWND, RPC_E_CHANGED_MODE};
        use windows::Win32::System::Com::{
            CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize, CLSCTX_ALL,
            COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE,
        };
        use windows::Win32::UI::Shell::Common::COMDLG_FILTERSPEC;
        use windows::Win32::UI::Shell::{
            FileOpenDialog, IFileOpenDialog, FOS_FILEMUSTEXIST, FOS_FORCEFILESYSTEM,
            FOS_PATHMUSTEXIST, SIGDN_FILESYSPATH,
        };

        unsafe {
            let hr = CoInitializeEx(None, COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE);
            let should_uninit = hr.is_ok();
            if hr.is_err() && hr != RPC_E_CHANGED_MODE {
                return None;
            }

            struct ComGuard(bool);
            impl Drop for ComGuard {
                fn drop(&mut self) {
                    if self.0 {
                        unsafe { CoUninitialize() };
                    }
                }
            }
            let _guard = ComGuard(should_uninit);

            let dialog: IFileOpenDialog =
                CoCreateInstance(&FileOpenDialog, None, CLSCTX_ALL).ok()?;

            let opts = dialog.GetOptions().ok()?;
            dialog
                .SetOptions(opts | FOS_FORCEFILESYSTEM | FOS_PATHMUSTEXIST | FOS_FILEMUSTEXIST)
                .ok()?;

            let wide_title = to_wide_null(title);
            let _ = dialog.SetTitle(PCWSTR::from_raw(wide_title.as_ptr()));

            // Keep wide buffers alive until after SetFileTypes / Show
            let wide_pairs: Vec<(Vec<u16>, Vec<u16>)> = filters
                .iter()
                .map(|(name, spec)| (to_wide_null(name), to_wide_null(spec)))
                .collect();
            if !wide_pairs.is_empty() {
                let specs: Vec<COMDLG_FILTERSPEC> = wide_pairs
                    .iter()
                    .map(|(n, s)| COMDLG_FILTERSPEC {
                        pszName: PCWSTR::from_raw(n.as_ptr()),
                        pszSpec: PCWSTR::from_raw(s.as_ptr()),
                    })
                    .collect();
                let _ = dialog.SetFileTypes(&specs);
            }

            let hwnd = owner_hwnd
                .filter(|&h| h != 0)
                .map(|h| HWND(h as *mut core::ffi::c_void));

            if dialog.Show(hwnd).is_err() {
                return None;
            }

            let result_item = dialog.GetResult().ok()?;
            let display_ptr = result_item.GetDisplayName(SIGDN_FILESYSPATH).ok()?;
            let selected = display_ptr.to_string().ok();
            CoTaskMemFree(Some(display_ptr.0 as *const core::ffi::c_void));
            selected.filter(|s| !s.trim().is_empty())
        }
    }
    #[cfg(not(windows))]
    {
        let _ = (owner_hwnd, title, filters);
        None
    }
}
