//! Settings in `HKCU\Software\VKeyEngine`. Value names are the ones OpenKey used, and the
//! OpenKey key is copied over on first run so existing settings and macros carry over.

use std::ptr::{null, null_mut};
use windows_sys::Win32::Foundation::ERROR_SUCCESS;
use windows_sys::Win32::System::Registry::*;

const KEY: &str = r"Software\VKeyEngine";
const OPENKEY_KEY: &str = r"Software\TuyenMai\OpenKey";
const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const RUN_VALUE: &str = "VKeyEngine";

/// NUL terminated UTF-16.
pub fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain([0]).collect()
}

fn get(name: &str, flags: REG_ROUTINE_FLAGS, buf: *mut core::ffi::c_void, size: &mut u32) -> bool {
    let r = unsafe { RegGetValueW(HKEY_CURRENT_USER, wide(KEY).as_ptr(), wide(name).as_ptr(), flags, null_mut(), buf, size) };
    r == ERROR_SUCCESS
}

fn set(name: &str, ty: REG_VALUE_TYPE, data: *const core::ffi::c_void, size: u32) {
    unsafe { RegSetKeyValueW(HKEY_CURRENT_USER, wide(KEY).as_ptr(), wide(name).as_ptr(), ty, data, size) };
}

/// DWORD value, or `default` when it was never written.
pub fn int(name: &str, default: i64) -> i64 {
    let (mut v, mut size) = (0u32, 4u32);
    if get(name, RRF_RT_REG_DWORD, &mut v as *mut u32 as _, &mut size) {
        v as i64
    } else {
        default
    }
}

pub fn flag(name: &str, default: bool) -> bool {
    int(name, default as i64) != 0
}

pub fn set_int(name: &str, v: i64) {
    let v = v as u32;
    set(name, REG_DWORD, &v as *const u32 as _, 4);
}

pub fn data(name: &str) -> Vec<u8> {
    let mut size = 0u32;
    if !get(name, RRF_RT_REG_BINARY, null_mut(), &mut size) {
        return Vec::new();
    }
    let mut buf = vec![0u8; size as usize];
    if !get(name, RRF_RT_REG_BINARY, buf.as_mut_ptr() as _, &mut size) {
        return Vec::new();
    }
    buf.truncate(size as usize);
    buf
}

pub fn set_data(name: &str, bytes: &[u8]) {
    set(name, REG_BINARY, bytes.as_ptr() as _, bytes.len() as u32);
}

/// First run (no key of our own yet): copy OpenKey's settings.
pub fn import_openkey() {
    unsafe {
        let mut k: HKEY = null_mut();
        if RegOpenKeyExW(HKEY_CURRENT_USER, wide(KEY).as_ptr(), 0, KEY_READ, &mut k) == ERROR_SUCCESS {
            RegCloseKey(k);
            return;
        }
        let mut src: HKEY = null_mut();
        if RegOpenKeyExW(HKEY_CURRENT_USER, wide(OPENKEY_KEY).as_ptr(), 0, KEY_READ, &mut src) != ERROR_SUCCESS {
            return;
        }
        let mut dst: HKEY = null_mut();
        let created = RegCreateKeyExW(
            HKEY_CURRENT_USER,
            wide(KEY).as_ptr(),
            0,
            null(),
            REG_OPTION_NON_VOLATILE,
            KEY_ALL_ACCESS,
            null(),
            &mut dst,
            null_mut(),
        );
        if created == ERROR_SUCCESS {
            RegCopyTreeW(src, null(), dst);
            RegCloseKey(dst);
        }
        RegCloseKey(src);
    }
}

/// Start with Windows (per user `Run` key).
pub fn set_run_on_startup(on: bool) {
    let (key, value) = (wide(RUN_KEY), wide(RUN_VALUE));
    unsafe {
        if !on {
            RegDeleteKeyValueW(HKEY_CURRENT_USER, key.as_ptr(), value.as_ptr());
            return;
        }
        let Ok(exe) = std::env::current_exe() else { return };
        let path = wide(&format!("\"{}\"", exe.display()));
        RegSetKeyValueW(HKEY_CURRENT_USER, key.as_ptr(), value.as_ptr(), REG_SZ, path.as_ptr() as _, (path.len() * 2) as u32);
    }
}
