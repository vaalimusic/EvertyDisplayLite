use anyhow::{bail, Result};
use tracing::{info, warn};
use windows::core::Interface;
use windows::Win32::Graphics::Direct3D11::{ID3D11Device, ID3D11Texture2D};
use windows::Win32::Graphics::Dxgi::{
    CreateDXGIFactory1, IDXGIFactory1, IDXGIOutput1, IDXGIOutputDuplication,
    DXGI_ERROR_ACCESS_LOST, DXGI_ERROR_WAIT_TIMEOUT, DXGI_OUTDUPL_FRAME_INFO, DXGI_OUTPUT_DESC,
};

pub struct DxgiOutputDuplicator {
    duplication: IDXGIOutputDuplication,
    output_desc: DXGI_OUTPUT_DESC,
    has_acquired_frame: bool,
}

impl DxgiOutputDuplicator {
    /// Create duplication for a specific display device name (e.g. "\\\\.\\DISPLAY2")
    pub fn new_for_device(d3d_device: &ID3D11Device, target_device_name: &str) -> Result<Self> {
        let factory: IDXGIFactory1 = unsafe { CreateDXGIFactory1()? };
        let mut adapter_index = 0;

        while let Ok(adapter) = unsafe { factory.EnumAdapters1(adapter_index) } {
            adapter_index += 1;
            let mut output_index = 0;

            while let Ok(output) = unsafe { adapter.EnumOutputs(output_index) } {
                output_index += 1;
                let desc = unsafe { output.GetDesc()? };

                let device_name = String::from_utf16_lossy(&desc.DeviceName)
                    .trim_matches('\0')
                    .to_string();

                if device_name.eq_ignore_ascii_case(target_device_name) {
                    info!(
                        "Found matching DXGI Output for {}: '{}'",
                        target_device_name, device_name
                    );
                    let output1: IDXGIOutput1 = output.cast()?;

                    unsafe {
                        use windows::Win32::System::StationsAndDesktops::{
                            OpenInputDesktop, SetThreadDesktop, DESKTOP_ACCESS_FLAGS,
                            DESKTOP_CONTROL_FLAGS,
                        };
                        if let Ok(hdesk) = OpenInputDesktop(
                            DESKTOP_CONTROL_FLAGS(0),
                            false,
                            DESKTOP_ACCESS_FLAGS(0x1FF),
                        ) {
                            let _ = SetThreadDesktop(hdesk);
                        }
                    }

                    let duplication = unsafe { output1.DuplicateOutput(d3d_device)? };

                    return Ok(Self {
                        duplication,
                        output_desc: desc,
                        has_acquired_frame: false,
                    });
                }
            }
        }

        bail!(
            "Could not find DXGI output matching device name: {}",
            target_device_name
        );
    }

    /// Try to acquire the next desktop frame texture within timeout_ms.
    /// Returns Ok(Some((texture, frame_info))) if frame arrived, Ok(None) if timed out.
    pub fn acquire_next_frame(
        &mut self,
        timeout_ms: u32,
    ) -> Result<Option<(ID3D11Texture2D, DXGI_OUTDUPL_FRAME_INFO)>> {
        if self.has_acquired_frame {
            self.release_frame()?;
        }

        let mut frame_info = DXGI_OUTDUPL_FRAME_INFO::default();
        let mut desktop_resource = None;

        let hr = unsafe {
            self.duplication
                .AcquireNextFrame(timeout_ms, &mut frame_info, &mut desktop_resource)
        };

        if let Err(err) = hr {
            if err.code() == DXGI_ERROR_WAIT_TIMEOUT {
                return Ok(None);
            }
            if err.code() == DXGI_ERROR_ACCESS_LOST {
                warn!(
                    "DXGI desktop duplication access lost (e.g. mode switch or desktop transition)"
                );
                return Err(err.into());
            }
            return Err(err.into());
        }

        self.has_acquired_frame = true;

        if let Some(resource) = desktop_resource {
            let texture: ID3D11Texture2D = resource.cast()?;
            Ok(Some((texture, frame_info)))
        } else {
            Ok(None)
        }
    }

    pub fn release_frame(&mut self) -> Result<()> {
        if self.has_acquired_frame {
            unsafe {
                self.duplication.ReleaseFrame()?;
            }
            self.has_acquired_frame = false;
        }
        Ok(())
    }

    pub fn width(&self) -> u32 {
        (self.output_desc.DesktopCoordinates.right - self.output_desc.DesktopCoordinates.left)
            .max(0) as u32
    }

    pub fn height(&self) -> u32 {
        (self.output_desc.DesktopCoordinates.bottom - self.output_desc.DesktopCoordinates.top)
            .max(0) as u32
    }
}

impl Drop for DxgiOutputDuplicator {
    fn drop(&mut self) {
        let _ = self.release_frame();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows::Win32::Graphics::Direct3D::{D3D_DRIVER_TYPE_UNKNOWN, D3D_FEATURE_LEVEL_11_0};
    use windows::Win32::Graphics::Direct3D11::{
        D3D11CreateDevice, D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_SDK_VERSION,
    };

    #[test]
    fn test_duplicate_all_outputs() {
        let factory: IDXGIFactory1 = match unsafe { CreateDXGIFactory1() } {
            Ok(factory) => factory,
            Err(error) => {
                println!("DXGI factory is unavailable in this session: {error:?}");
                return;
            }
        };
        let mut adapter_index = 0;

        while let Ok(adapter) = unsafe { factory.EnumAdapters1(adapter_index) } {
            adapter_index += 1;
            let desc = match unsafe { adapter.GetDesc1() } {
                Ok(desc) => desc,
                Err(error) => {
                    println!("  Could not read adapter description: {error:?}");
                    continue;
                }
            };
            let adapter_name = String::from_utf16_lossy(&desc.Description)
                .trim_matches('\0')
                .to_string();
            println!("\nAdapter #{}: {}", adapter_index - 1, adapter_name);

            let mut device = None;
            let mut feature_level = D3D_FEATURE_LEVEL_11_0;
            let levels = [D3D_FEATURE_LEVEL_11_0];

            let adapter0: windows::Win32::Graphics::Dxgi::IDXGIAdapter = match adapter.cast() {
                Ok(adapter) => adapter,
                Err(error) => {
                    println!("  Could not access the base DXGI adapter: {error:?}");
                    continue;
                }
            };
            let res = unsafe {
                D3D11CreateDevice(
                    Some(&adapter0),
                    D3D_DRIVER_TYPE_UNKNOWN,
                    None,
                    D3D11_CREATE_DEVICE_BGRA_SUPPORT,
                    Some(&levels),
                    D3D11_SDK_VERSION,
                    Some(&mut device),
                    Some(&mut feature_level),
                    None,
                )
            };

            let device = match (res, device) {
                (Ok(_), Some(d)) => d,
                (e, _) => {
                    println!("  Failed to create D3D11Device on this adapter: {:?}", e);
                    continue;
                }
            };

            let mut output_index = 0;
            while let Ok(output) = unsafe { adapter.EnumOutputs(output_index) } {
                output_index += 1;
                let odesc = match unsafe { output.GetDesc() } {
                    Ok(desc) => desc,
                    Err(error) => {
                        println!("  Could not read output description: {error:?}");
                        continue;
                    }
                };
                let oname = String::from_utf16_lossy(&odesc.DeviceName)
                    .trim_matches('\0')
                    .to_string();
                println!(
                    "  Output #{}: '{}' coords: {:?}",
                    output_index - 1,
                    oname,
                    odesc.DesktopCoordinates
                );

                let output1: Result<IDXGIOutput1, _> = output.cast();
                if let Ok(o1) = output1 {
                    unsafe {
                        use windows::Win32::System::StationsAndDesktops::{
                            OpenInputDesktop, SetThreadDesktop, DESKTOP_ACCESS_FLAGS,
                            DESKTOP_CONTROL_FLAGS,
                        };
                        match OpenInputDesktop(
                            DESKTOP_CONTROL_FLAGS(0),
                            false,
                            DESKTOP_ACCESS_FLAGS(0x1FF),
                        ) {
                            Ok(hdesk) => {
                                println!("    OpenInputDesktop: Ok({:?})", hdesk);
                                let st = SetThreadDesktop(hdesk);
                                println!("    SetThreadDesktop: {:?}", st);
                            }
                            Err(e) => println!("    OpenInputDesktop failed: {:?}", e),
                        }
                    }
                    let dup_res = unsafe { o1.DuplicateOutput(&device) };
                    match dup_res {
                        Ok(_) => println!("    -> DuplicateOutput SUCCESS!"),
                        Err(e) => println!("    -> DuplicateOutput FAILED: {:?}", e),
                    }
                }
            }
        }
    }
}
