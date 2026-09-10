use anyhow::Result;
use multitor_ipc::{DisplayBounds, DisplayInfo};
use std::mem::zeroed;
use tracing::{debug, info};
use windows::core::PCWSTR;
use windows::Win32::Foundation::{BOOL, LPARAM, RECT};
use windows::Win32::Graphics::Gdi::{
    EnumDisplayDevicesW, EnumDisplayMonitors, EnumDisplaySettingsW, GetMonitorInfoW, DEVMODEW,
    DISPLAY_DEVICEW, ENUM_CURRENT_SETTINGS, HDC, HMONITOR, MONITORINFOEXW,
};

/// Enumerate all active displays connected to the Windows desktop session.
pub fn enumerate_displays() -> Result<Vec<DisplayInfo>> {
    let mut displays: Vec<DisplayInfo> = Vec::new();
    let displays_ptr = &mut displays as *mut Vec<DisplayInfo> as isize;

    unsafe {
        let _ = EnumDisplayMonitors(
            HDC::default(),
            None,
            Some(monitor_enum_proc),
            LPARAM(displays_ptr),
        );
    }

    info!("Discovered {} active display(s)", displays.len());
    for d in &displays {
        debug!(
            "Display #{}: '{}' ({}) bounds=[{},{} {}x{}] @ {}Hz primary={} virtual={}",
            d.id,
            d.friendly_name,
            d.device_name,
            d.bounds.x,
            d.bounds.y,
            d.bounds.width,
            d.bounds.height,
            d.refresh_rate,
            d.is_primary,
            d.is_virtual
        );
    }

    Ok(displays)
}

unsafe extern "system" fn monitor_enum_proc(
    hmonitor: HMONITOR,
    _: HDC,
    _: *mut RECT,
    lparam: LPARAM,
) -> BOOL {
    let displays = &mut *(lparam.0 as *mut Vec<DisplayInfo>);

    let mut mi: MONITORINFOEXW = zeroed();
    mi.monitorInfo.cbSize = std::mem::size_of::<MONITORINFOEXW>() as u32;

    if GetMonitorInfoW(hmonitor, &mut mi.monitorInfo as *mut _ as *mut _).as_bool() {
        let rc = mi.monitorInfo.rcMonitor;
        let width = (rc.right - rc.left).max(0) as u32;
        let height = (rc.bottom - rc.top).max(0) as u32;
        let is_primary = (mi.monitorInfo.dwFlags & 1) != 0; // MONITORINFOF_PRIMARY = 1

        let device_name = String::from_utf16_lossy(&mi.szDevice)
            .trim_matches('\0')
            .to_string();

        let mut devmode: DEVMODEW = zeroed();
        devmode.dmSize = std::mem::size_of::<DEVMODEW>() as u16;
        let mut refresh_rate = 60;

        let device_name_u16: Vec<u16> = mi
            .szDevice
            .iter()
            .take_while(|&&c| c != 0)
            .cloned()
            .chain(std::iter::once(0))
            .collect();

        if EnumDisplaySettingsW(
            PCWSTR(device_name_u16.as_ptr()),
            ENUM_CURRENT_SETTINGS,
            &mut devmode,
        )
        .as_bool()
            && devmode.dmDisplayFrequency > 0
        {
            refresh_rate = devmode.dmDisplayFrequency;
        }

        // Get friendly monitor string and check hardware ID for virtual display
        let (friendly_name, is_virtual) = query_display_device_info(&device_name_u16);

        // This ID is a snapshot-local index. Persistent identity is device_name;
        // ConfigManager maps it to the user's stable logical monitor ID.
        let id = displays.len() as u32 + 1;
        displays.push(DisplayInfo {
            id,
            device_name,
            friendly_name,
            bounds: DisplayBounds::new(rc.left, rc.top, width, height),
            refresh_rate,
            is_primary,
            is_virtual,
        });
    }

    BOOL(1) // Continue enumeration
}

fn query_display_device_info(device_name: &[u16]) -> (String, bool) {
    let mut dd: DISPLAY_DEVICEW = unsafe { zeroed() };
    dd.cb = std::mem::size_of::<DISPLAY_DEVICEW>() as u32;

    let mut friendly_name = "Generic Display".to_string();
    let mut is_virtual = false;

    unsafe {
        if EnumDisplayDevicesW(PCWSTR(device_name.as_ptr()), 0, &mut dd, 0).as_bool() {
            let str_name = String::from_utf16_lossy(&dd.DeviceString)
                .trim_matches('\0')
                .to_string();
            if !str_name.is_empty() {
                friendly_name = str_name;
            }

            let device_id = String::from_utf16_lossy(&dd.DeviceID)
                .trim_matches('\0')
                .to_uppercase();

            // `is_virtual` means an EvertyDisplay-managed MttVDD output, not
            // every indirect/USB/remote display on the machine.
            if is_managed_virtual_device(&device_id, &friendly_name) {
                is_virtual = true;
            }
        }
    }

    (friendly_name, is_virtual)
}

fn is_managed_virtual_device(device_id: &str, friendly_name: &str) -> bool {
    let device_upper = device_id.to_uppercase();
    let friendly_upper = friendly_name.to_uppercase();
    device_upper.contains("MTTVDD")
        || device_upper.contains("MTT")
        || friendly_upper == "VIRTUAL DISPLAY DRIVER"
        || friendly_upper.contains("MTTVDD")
}

/// Change display resolution and refresh rate via ChangeDisplaySettingsExW
pub fn set_display_mode(
    device_name: &str,
    width: u32,
    height: u32,
    refresh_rate: u32,
) -> Result<()> {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::Graphics::Gdi::{
        ChangeDisplaySettingsExW, EnumDisplaySettingsW, CDS_UPDATEREGISTRY, DISP_CHANGE_SUCCESSFUL,
        ENUM_DISPLAY_SETTINGS_MODE,
    };

    let name_u16: Vec<u16> = device_name
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();

    // Find a matching supported DEVMODE for this display
    let mut candidate: DEVMODEW = unsafe { zeroed() };
    candidate.dmSize = std::mem::size_of::<DEVMODEW>() as u16;
    let mut selected: Option<DEVMODEW> = None;
    let mut i = 0;

    while unsafe {
        EnumDisplaySettingsW(
            PCWSTR(name_u16.as_ptr()),
            ENUM_DISPLAY_SETTINGS_MODE(i),
            &mut candidate,
        )
    }
    .as_bool()
    {
        if candidate.dmPelsWidth == width && candidate.dmPelsHeight == height {
            if refresh_rate != 0 && candidate.dmDisplayFrequency == refresh_rate {
                selected = Some(candidate);
                break;
            }
            if refresh_rate == 0
                && selected
                    .as_ref()
                    .map(|mode| candidate.dmDisplayFrequency > mode.dmDisplayFrequency)
                    .unwrap_or(true)
            {
                // A zero preferred rate means resolution is mandatory while the
                // highest rate actually advertised by this output is acceptable.
                selected = Some(candidate);
            }
        }
        i += 1;
    }

    let Some(mut devmode) = selected else {
        anyhow::bail!(
            "Mode {}x{} @ {}Hz not found for display {}",
            width,
            height,
            refresh_rate,
            device_name
        );
    };

    // Preserve current desktop position
    let mut cur_devmode: DEVMODEW = unsafe { zeroed() };
    cur_devmode.dmSize = std::mem::size_of::<DEVMODEW>() as u16;
    if unsafe {
        EnumDisplaySettingsW(
            PCWSTR(name_u16.as_ptr()),
            ENUM_CURRENT_SETTINGS,
            &mut cur_devmode,
        )
    }
    .as_bool()
    {
        unsafe {
            devmode.Anonymous1.Anonymous2.dmPosition = cur_devmode.Anonymous1.Anonymous2.dmPosition;
        }
        devmode.dmFields = windows::Win32::Graphics::Gdi::DEVMODE_FIELD_FLAGS(
            devmode.dmFields.0 | windows::Win32::Graphics::Gdi::DM_POSITION.0,
        );
    }

    use windows::Win32::Graphics::Gdi::CDS_NORESET;

    let res = unsafe {
        ChangeDisplaySettingsExW(
            PCWSTR(name_u16.as_ptr()),
            Some(&devmode),
            HWND::default(),
            CDS_UPDATEREGISTRY | CDS_NORESET,
            None,
        )
    };

    if res != DISP_CHANGE_SUCCESSFUL {
        anyhow::bail!(
            "ChangeDisplaySettingsExW (stage 1) for {} returned error: {:?}",
            device_name,
            res
        );
    }

    // Apply / commit the changes across the desktop
    let commit_res = unsafe {
        ChangeDisplaySettingsExW(
            PCWSTR::null(),
            None,
            HWND::default(),
            windows::Win32::Graphics::Gdi::CDS_TYPE(0),
            None,
        )
    };

    if commit_res == DISP_CHANGE_SUCCESSFUL {
        info!(
            "Successfully changed display {} mode to {}x{} @ {}Hz",
            device_name, width, height, devmode.dmDisplayFrequency
        );
        Ok(())
    } else {
        anyhow::bail!(
            "ChangeDisplaySettingsExW (commit) returned error: {:?}",
            commit_res
        );
    }
}

/// Set desktop position of a display monitor in Windows OS
#[allow(dead_code)]
pub fn set_display_position(device_name: &str, x: i32, y: i32) -> Result<()> {
    set_display_layout(&[(device_name.to_string(), x, y)])
}

/// Apply all display positions as one atomic Windows topology transaction.
/// Staging every output before the final commit avoids intermediate layouts,
/// cursor jumps and display flicker while monitors are being rearranged.
pub fn set_display_layout(positions: &[(String, i32, i32)]) -> Result<()> {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::Graphics::Gdi::{
        ChangeDisplaySettingsExW, CDS_NORESET, CDS_UPDATEREGISTRY, DISP_CHANGE_SUCCESSFUL,
        DM_POSITION,
    };

    if positions.is_empty() {
        return Ok(());
    }

    type StagedDisplayLayout = (Vec<u16>, DEVMODEW, DEVMODEW, String, i32, i32);

    let mut staged: Vec<StagedDisplayLayout> = Vec::with_capacity(positions.len());
    for (device_name, x, y) in positions {
        let name_u16: Vec<u16> = device_name
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();
        let mut devmode: DEVMODEW = unsafe { zeroed() };
        devmode.dmSize = std::mem::size_of::<DEVMODEW>() as u16;
        if !unsafe {
            EnumDisplaySettingsW(
                PCWSTR(name_u16.as_ptr()),
                ENUM_CURRENT_SETTINGS,
                &mut devmode,
            )
        }
        .as_bool()
        {
            anyhow::bail!("Could not read current display mode for {}", device_name);
        }
        let original_mode = devmode;
        devmode.Anonymous1.Anonymous2.dmPosition.x = *x;
        devmode.Anonymous1.Anonymous2.dmPosition.y = *y;
        devmode.dmFields =
            windows::Win32::Graphics::Gdi::DEVMODE_FIELD_FLAGS(devmode.dmFields.0 | DM_POSITION.0);
        staged.push((
            name_u16,
            devmode,
            original_mode,
            device_name.clone(),
            *x,
            *y,
        ));
    }

    let restore_original_layout = |entries: &[StagedDisplayLayout]| {
        for (name_u16, _, original_mode, _, _, _) in entries {
            let _ = unsafe {
                ChangeDisplaySettingsExW(
                    PCWSTR(name_u16.as_ptr()),
                    Some(original_mode),
                    HWND::default(),
                    CDS_UPDATEREGISTRY | CDS_NORESET,
                    None,
                )
            };
        }
        let _ = unsafe {
            ChangeDisplaySettingsExW(
                PCWSTR::null(),
                None,
                HWND::default(),
                windows::Win32::Graphics::Gdi::CDS_TYPE(0),
                None,
            )
        };
    };

    for (name_u16, devmode, _, device_name, _, _) in &staged {
        let result = unsafe {
            ChangeDisplaySettingsExW(
                PCWSTR(name_u16.as_ptr()),
                Some(devmode),
                HWND::default(),
                CDS_UPDATEREGISTRY | CDS_NORESET,
                None,
            )
        };
        if result != DISP_CHANGE_SUCCESSFUL {
            restore_original_layout(&staged);
            anyhow::bail!("Could not stage position for {}: {:?}", device_name, result);
        }
    }

    let commit_res = unsafe {
        ChangeDisplaySettingsExW(
            PCWSTR::null(),
            None,
            HWND::default(),
            windows::Win32::Graphics::Gdi::CDS_TYPE(0),
            None,
        )
    };

    if commit_res == DISP_CHANGE_SUCCESSFUL {
        for (_, _, _, device_name, x, y) in staged {
            info!(
                "Updated display {} desktop position to ({}, {})",
                device_name, x, y
            );
        }
        Ok(())
    } else {
        restore_original_layout(&staged);
        anyhow::bail!(
            "ChangeDisplaySettingsExW commit returned error: {:?}",
            commit_res
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_mtt_outputs_are_managed_as_virtual() {
        assert!(is_managed_virtual_device(
            r"ROOT\MTTVDD\0000",
            "Virtual Display Driver"
        ));
        assert!(!is_managed_virtual_device(
            r"ROOT\DISPLAY\0000",
            "Generic Display"
        ));
        assert!(!is_managed_virtual_device(
            r"ROOT\PARSEC\0000",
            "Parsec Virtual Display Adapter"
        ));
    }

    #[test]
    fn test_enumerate_modes() {
        let displays = enumerate_displays().expect("Failed to enumerate displays");
        for d in &displays {
            println!(
                "\n=== Modes for {} ({}) ===",
                d.device_name, d.friendly_name
            );
            let name_u16: Vec<u16> = d
                .device_name
                .encode_utf16()
                .chain(std::iter::once(0))
                .collect();
            let mut devmode: DEVMODEW = unsafe { zeroed() };
            devmode.dmSize = std::mem::size_of::<DEVMODEW>() as u16;
            let mut i = 0;
            while unsafe {
                EnumDisplaySettingsW(
                    PCWSTR(name_u16.as_ptr()),
                    windows::Win32::Graphics::Gdi::ENUM_DISPLAY_SETTINGS_MODE(i),
                    &mut devmode,
                )
            }
            .as_bool()
            {
                if devmode.dmPelsWidth >= 1920 {
                    println!(
                        "Mode #{}: {}x{} @ {}Hz",
                        i, devmode.dmPelsWidth, devmode.dmPelsHeight, devmode.dmDisplayFrequency
                    );
                }
                i += 1;
            }
        }
    }

    #[test]
    fn test_query_display_config() {
        use windows::Win32::Devices::Display::{
            GetDisplayConfigBufferSizes, QueryDisplayConfig, DISPLAYCONFIG_MODE_INFO,
            DISPLAYCONFIG_MODE_INFO_TYPE_SOURCE, DISPLAYCONFIG_PATH_INFO, QDC_ONLY_ACTIVE_PATHS,
        };

        let mut num_paths = 0;
        let mut num_modes = 0;
        let res = unsafe {
            GetDisplayConfigBufferSizes(QDC_ONLY_ACTIVE_PATHS, &mut num_paths, &mut num_modes)
        };
        println!(
            "Buffer sizes: paths={}, modes={}, err={:?}",
            num_paths, num_modes, res
        );

        let mut paths: Vec<DISPLAYCONFIG_PATH_INFO> = vec![unsafe { zeroed() }; num_paths as usize];
        let mut modes: Vec<DISPLAYCONFIG_MODE_INFO> = vec![unsafe { zeroed() }; num_modes as usize];

        let q_res = unsafe {
            QueryDisplayConfig(
                QDC_ONLY_ACTIVE_PATHS,
                &mut num_paths,
                paths.as_mut_ptr(),
                &mut num_modes,
                modes.as_mut_ptr(),
                None,
            )
        };
        println!(
            "QueryDisplayConfig result: err={:?}, returned paths={}, modes={}",
            q_res, num_paths, num_modes
        );

        for (i, m) in modes.iter().enumerate().take(num_modes as usize) {
            if m.infoType == DISPLAYCONFIG_MODE_INFO_TYPE_SOURCE {
                unsafe {
                    let sm = m.Anonymous.sourceMode;
                    println!(
                        "Source Mode #{}: ID={}, Adapter={:?}, Pos=({}, {}), Size={}x{}",
                        i, m.id, m.adapterId, sm.position.x, sm.position.y, sm.width, sm.height
                    );
                }
            }
        }
    }
}
