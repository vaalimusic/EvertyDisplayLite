use anyhow::Result;
use std::env;
use tracing::info;
use windows::core::w;
use windows::Win32::System::Registry::{
    RegCloseKey, RegDeleteValueW, RegOpenKeyExW, RegQueryValueExW, RegSetValueExW, HKEY,
    HKEY_CURRENT_USER, KEY_READ, KEY_WRITE, REG_SZ,
};

const RUN_KEY: windows::core::PCWSTR = w!("Software\\Microsoft\\Windows\\CurrentVersion\\Run");
const APP_NAME: windows::core::PCWSTR = w!("EvertyDisplayService");
const OLD_APP_NAME: windows::core::PCWSTR = w!("Multitor");

pub fn is_autostart_enabled() -> bool {
    unsafe {
        let mut hkey = HKEY::default();
        if RegOpenKeyExW(HKEY_CURRENT_USER, RUN_KEY, 0, KEY_READ, &mut hkey).is_err() {
            return false;
        }

        let mut data_size = 0u32;
        let mut status = RegQueryValueExW(hkey, APP_NAME, None, None, None, Some(&mut data_size));
        if status.is_err() || data_size == 0 {
            status = RegQueryValueExW(hkey, OLD_APP_NAME, None, None, None, Some(&mut data_size));
        }

        let _ = RegCloseKey(hkey);
        status.is_ok() && data_size > 0
    }
}

pub fn set_autostart(enable: bool) -> Result<()> {
    unsafe {
        let mut hkey = HKEY::default();
        if enable {
            let exe_path = env::current_exe()?;
            let exe_str = format!("\"{}\"", exe_path.to_string_lossy());
            let wide_chars: Vec<u16> = exe_str.encode_utf16().chain(std::iter::once(0)).collect();

            RegOpenKeyExW(HKEY_CURRENT_USER, RUN_KEY, 0, KEY_WRITE, &mut hkey).ok()?;

            let byte_slice =
                std::slice::from_raw_parts(wide_chars.as_ptr() as *const u8, wide_chars.len() * 2);

            RegSetValueExW(hkey, APP_NAME, 0, REG_SZ, Some(byte_slice)).ok()?;
            let _ = RegDeleteValueW(hkey, OLD_APP_NAME);
            let _ = RegCloseKey(hkey);
            info!(
                "EvertyDisplayService autostart enabled in registry: {}",
                exe_str
            );
        } else {
            if RegOpenKeyExW(HKEY_CURRENT_USER, RUN_KEY, 0, KEY_WRITE, &mut hkey).is_ok() {
                let _ = RegDeleteValueW(hkey, APP_NAME);
                let _ = RegDeleteValueW(hkey, OLD_APP_NAME);
                let _ = RegCloseKey(hkey);
                info!("EvertyDisplayService autostart disabled in registry");
            }
        }
    }
    Ok(())
}
