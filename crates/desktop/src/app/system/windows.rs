use std::{
    os::windows::ffi::OsStrExt,
    path::{Path, PathBuf},
};

use windows::{
    Win32::{
        Foundation::RPC_E_CHANGED_MODE,
        System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx, CoTaskMemFree, CoUninitialize},
        UI::Shell::{Common::ITEMIDLIST, SHOpenFolderAndSelectItems, SHParseDisplayName},
    },
    core::{PCWSTR, Result},
};

pub(super) fn reveal_file(path: &Path) -> Result<()> {
    // GPUI's background worker may be uninitialized or already use another
    // COM apartment. Balance initialization only when this call acquired it.
    let initialized = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
    if initialized.is_err() && initialized != RPC_E_CHANGED_MODE {
        initialized.ok()?;
    }
    let _apartment = Apartment(initialized.is_ok());
    let path = shell_path(path)?;
    let wide: Vec<_> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    let mut item = ItemId(std::ptr::null_mut());
    // The UTF-16 string stays alive through parsing. The returned allocation
    // belongs to ItemId, which releases it on both success and failure.
    unsafe { SHParseDisplayName(PCWSTR(wide.as_ptr()), None, &mut item.0, 0, None)? };
    // With an empty selection array, the absolute ID identifies the file.
    unsafe { SHOpenFolderAndSelectItems(item.0, None, 0) }
}

pub(super) fn shell_path(path: &Path) -> std::io::Result<PathBuf> {
    std::path::absolute(path).map(|absolute| absolute.components().collect())
}

struct Apartment(bool);

impl Drop for Apartment {
    fn drop(&mut self) {
        if self.0 {
            // This thread successfully acquired a COM initialization above.
            unsafe { CoUninitialize() };
        }
    }
}

struct ItemId(*mut ITEMIDLIST);

impl Drop for ItemId {
    fn drop(&mut self) {
        // Shell item IDs returned by SHParseDisplayName use the COM allocator.
        unsafe { CoTaskMemFree(Some(self.0.cast())) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shell_paths_normalize_git_paths_using_the_standard_library() {
        for (source, expected) in [
            (
                r"C:/Projects/测试 repo\file.txt",
                r"C:\Projects\测试 repo\file.txt",
            ),
            (
                r"//server/share/测试 file.txt",
                r"\\server\share\测试 file.txt",
            ),
        ] {
            assert_eq!(shell_path(Path::new(source)).unwrap().as_os_str(), expected);
        }
    }
}
