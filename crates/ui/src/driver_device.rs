use std::path::Path;
use std::process::Command;
use windows::core::{w, PCWSTR};
use windows::Win32::Devices::DeviceAndDriverInstallation::{
    SetupDiCallClassInstaller, SetupDiCreateDeviceInfoList, SetupDiCreateDeviceInfoW,
    SetupDiDestroyDeviceInfoList, SetupDiEnumDeviceInfo, SetupDiGetClassDevsW,
    SetupDiGetDeviceRegistryPropertyW, SetupDiRemoveDevice, SetupDiSetDeviceRegistryPropertyW,
    DICD_GENERATE_ID, DIF_REGISTERDEVICE, DIGCF_PRESENT, GUID_DEVCLASS_DISPLAY, HDEVINFO,
    SPDRP_HARDWAREID, SP_DEVINFO_DATA,
};
use windows::Win32::Foundation::HWND;

pub const HARDWARE_ID: &str = r"Root\MttVDD";
const CREATE_NO_WINDOW: u32 = 0x08000000;

struct DeviceInfoSet(HDEVINFO);

impl Drop for DeviceInfoSet {
    fn drop(&mut self) {
        let _ = unsafe { SetupDiDestroyDeviceInfoList(self.0) };
    }
}

fn hardware_id_bytes(hardware_id: &str) -> Vec<u8> {
    hardware_id
        .encode_utf16()
        .chain([0, 0])
        .flat_map(u16::to_le_bytes)
        .collect()
}

fn property_contains_hardware_id(bytes: &[u8], hardware_id: &str) -> bool {
    let words: Vec<u16> = bytes
        .chunks_exact(2)
        .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
        .collect();
    words
        .split(|word| *word == 0)
        .filter(|entry| !entry.is_empty())
        .any(|entry| String::from_utf16_lossy(entry).eq_ignore_ascii_case(hardware_id))
}

fn present_display_devices() -> Result<DeviceInfoSet, String> {
    unsafe {
        SetupDiGetClassDevsW(
            Some(&GUID_DEVCLASS_DISPLAY),
            PCWSTR::null(),
            HWND::default(),
            DIGCF_PRESENT,
        )
        .map(DeviceInfoSet)
        .map_err(|error| format!("SetupAPI could not enumerate display devices: {error}"))
    }
}

fn matching_devices<F>(mut action: F) -> Result<u32, String>
where
    F: FnMut(HDEVINFO, &mut SP_DEVINFO_DATA) -> Result<(), String>,
{
    let devices = present_display_devices()?;
    let mut matched = 0;
    let mut index = 0;
    loop {
        let mut info = SP_DEVINFO_DATA {
            cbSize: std::mem::size_of::<SP_DEVINFO_DATA>() as u32,
            ..Default::default()
        };
        if unsafe { SetupDiEnumDeviceInfo(devices.0, index, &mut info) }.is_err() {
            break;
        }
        index += 1;

        let mut property = vec![0u8; 4096];
        let mut required = 0;
        if unsafe {
            SetupDiGetDeviceRegistryPropertyW(
                devices.0,
                &info,
                SPDRP_HARDWAREID,
                None,
                Some(&mut property),
                Some(&mut required),
            )
        }
        .is_ok()
            && property_contains_hardware_id(
                &property[..required.min(property.len() as u32) as usize],
                HARDWARE_ID,
            )
        {
            action(devices.0, &mut info)?;
            matched += 1;
        }
    }
    Ok(matched)
}

pub fn is_present() -> Result<bool, String> {
    matching_devices(|_, _| Ok(())).map(|count| count > 0)
}

pub fn remove_all() -> Result<(), String> {
    matching_devices(|devices, info| {
        if unsafe { SetupDiRemoveDevice(devices, info) }.as_bool() {
            Ok(())
        } else {
            Err(format!(
                "SetupAPI could not remove the EvertyDisplay adapter: {}",
                windows::core::Error::from_win32()
            ))
        }
    })
    .map(|_| ())
}

fn run_pnputil(args: &[&str]) -> Result<(), String> {
    let mut command = Command::new("pnputil.exe");
    command.args(args);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    let status = command
        .status()
        .map_err(|error| format!("Could not start pnputil.exe: {error}"))?;
    if status.success() || status.code() == Some(3010) {
        Ok(())
    } else {
        Err(format!(
            "pnputil.exe failed with exit code {}",
            status.code().unwrap_or(-1)
        ))
    }
}

pub fn restart_existing() -> Result<(), String> {
    let _ = run_pnputil(&["/enable-device", "/deviceid", HARDWARE_ID]);
    run_pnputil(&["/restart-device", "/deviceid", HARDWARE_ID])
}

pub fn install(inf: &Path) -> Result<(), String> {
    if is_present()? {
        return restart_existing();
    }

    let devices =
        unsafe { SetupDiCreateDeviceInfoList(Some(&GUID_DEVCLASS_DISPLAY), HWND::default()) }
            .map(DeviceInfoSet)
            .map_err(|error| format!("SetupAPI could not create a device set: {error}"))?;
    let mut info = SP_DEVINFO_DATA {
        cbSize: std::mem::size_of::<SP_DEVINFO_DATA>() as u32,
        ..Default::default()
    };
    unsafe {
        SetupDiCreateDeviceInfoW(
            devices.0,
            w!("Display"),
            &GUID_DEVCLASS_DISPLAY,
            PCWSTR::null(),
            HWND::default(),
            DICD_GENERATE_ID,
            Some(&mut info),
        )
        .map_err(|error| format!("SetupAPI could not create the display device: {error}"))?;
        SetupDiSetDeviceRegistryPropertyW(
            devices.0,
            &mut info,
            SPDRP_HARDWAREID,
            Some(&hardware_id_bytes(HARDWARE_ID)),
        )
        .map_err(|error| format!("SetupAPI could not assign the hardware ID: {error}"))?;
        SetupDiCallClassInstaller(DIF_REGISTERDEVICE, devices.0, Some(&info))
            .map_err(|error| format!("SetupAPI could not register the display device: {error}"))?;
    }
    drop(devices);

    let inf = inf
        .to_str()
        .ok_or_else(|| "The driver INF path is not valid Unicode".to_string())?;
    if let Err(error) = run_pnputil(&["/add-driver", inf, "/install"]) {
        let _ = remove_all();
        return Err(error);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hardware_id_multisz_is_exact_and_case_insensitive() {
        let bytes = hardware_id_bytes(HARDWARE_ID);
        assert!(property_contains_hardware_id(&bytes, r"ROOT\MTTVDD"));
        assert!(!property_contains_hardware_id(&bytes, r"Root\AnotherVdd"));
        assert!(bytes.ends_with(&[0, 0, 0, 0]));
    }

    #[test]
    fn live_driver_is_found_through_setupapi_when_available() {
        if multitor_driver_manager::is_driver_pipe_ready() {
            assert_eq!(is_present(), Ok(true));
        }
    }
}
