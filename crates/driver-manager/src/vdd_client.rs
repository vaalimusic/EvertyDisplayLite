use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use tokio::io::AsyncWriteExt;
use tokio::net::windows::named_pipe::ClientOptions;
use tracing::{debug, info, warn};

pub const VDD_PIPE_NAME: &str = r"\\.\pipe\MTTVirtualDisplayPipe";
pub const VDD_SETTINGS_PATH: &str = r"C:\VirtualDisplayDriver\vdd_settings.xml";
const DRIVER_CHANGE_COOLDOWN: std::time::Duration = std::time::Duration::from_secs(12);
const DRIVER_CHANGE_BUDGET_WINDOW: std::time::Duration = std::time::Duration::from_secs(10 * 60);
const MAX_NORMAL_DRIVER_CHANGES_PER_WINDOW: u32 = 10;
const MAX_RECOVERY_DRIVER_CHANGES_PER_WINDOW: u32 = 11;

fn driver_change_gate() -> &'static tokio::sync::Mutex<Option<std::time::Instant>> {
    static GATE: std::sync::OnceLock<tokio::sync::Mutex<Option<std::time::Instant>>> =
        std::sync::OnceLock::new();
    GATE.get_or_init(|| tokio::sync::Mutex::new(None))
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct DriverChangeBudget {
    boot_id: u64,
    changes: u32,
    #[serde(default)]
    window_started_unix_ms: u64,
}

fn unix_time_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn fresh_driver_change_budget(boot_id: u64, now_ms: u64) -> DriverChangeBudget {
    DriverChangeBudget {
        boot_id,
        changes: 0,
        window_started_unix_ms: now_ms,
    }
}

fn normalize_driver_change_budget(
    saved: Option<DriverChangeBudget>,
    boot_id: u64,
    now_ms: u64,
) -> DriverChangeBudget {
    let Some(saved) = saved else {
        return fresh_driver_change_budget(boot_id, now_ms);
    };
    let window_ms = DRIVER_CHANGE_BUDGET_WINDOW.as_millis() as u64;
    let window_is_valid = saved.window_started_unix_ms > 0
        && now_ms >= saved.window_started_unix_ms
        && now_ms.saturating_sub(saved.window_started_unix_ms) < window_ms;
    if saved.boot_id == boot_id && window_is_valid {
        saved
    } else {
        fresh_driver_change_budget(boot_id, now_ms)
    }
}

fn current_windows_boot_id() -> u64 {
    use windows::core::w;
    use windows::Win32::System::Registry::{
        RegCloseKey, RegOpenKeyExW, RegQueryValueExW, HKEY, HKEY_LOCAL_MACHINE, KEY_READ,
    };

    unsafe {
        let mut key = HKEY::default();
        if RegOpenKeyExW(
            HKEY_LOCAL_MACHINE,
            w!("SYSTEM\\CurrentControlSet\\Control\\Session Manager\\Memory Management\\PrefetchParameters"),
            0,
            KEY_READ,
            &mut key,
        )
        .ok()
        .is_ok()
        {
            let mut boot_id = 0u32;
            let mut size = std::mem::size_of::<u32>() as u32;
            let query = RegQueryValueExW(
                key,
                w!("BootId"),
                None,
                None,
                Some((&mut boot_id as *mut u32).cast()),
                Some(&mut size),
            );
            let _ = RegCloseKey(key);
            if query.is_ok() && size == std::mem::size_of::<u32>() as u32 {
                return boot_id as u64;
            }
        }
    }

    // Prefetch can be disabled on Windows Server, in which case BootId is not
    // guaranteed to exist. Derive a stable boot marker from wall-clock time and
    // monotonic uptime instead of disabling all monitor changes on that machine.
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;
    let uptime_ms = unsafe { windows::Win32::System::SystemInformation::GetTickCount64() };
    let boot_epoch_five_minute_bucket = now_ms.saturating_sub(uptime_ms) / 300_000;
    (1u64 << 63) | boot_epoch_five_minute_bucket
}

fn driver_change_budget_path() -> Result<std::path::PathBuf> {
    let app_data = std::env::var_os("APPDATA").context("APPDATA is not available")?;
    let directory = std::path::PathBuf::from(app_data).join("VirtualScreens");
    std::fs::create_dir_all(&directory)
        .context("Could not create the EvertyDisplay settings directory")?;
    Ok(directory.join("driver-change-budget.json"))
}

fn reserve_driver_change(recovery: bool) -> Result<()> {
    let boot_id = current_windows_boot_id();
    let now_ms = unix_time_ms();
    let path = driver_change_budget_path()?;
    let saved = std::fs::read(&path)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<DriverChangeBudget>(&bytes).ok());
    let mut budget = normalize_driver_change_budget(saved, boot_id, now_ms);
    let limit = if recovery {
        MAX_RECOVERY_DRIVER_CHANGES_PER_WINDOW
    } else {
        MAX_NORMAL_DRIVER_CHANGES_PER_WINDOW
    };
    if budget.changes >= limit {
        let retry_after_ms = DRIVER_CHANGE_BUDGET_WINDOW
            .as_millis()
            .saturating_sub(now_ms.saturating_sub(budget.window_started_unix_ms) as u128);
        let retry_after_minutes = retry_after_ms.div_ceil(60_000).max(1);
        bail!(
            "Too many virtual-display driver changes in a short time. Try again in about {retry_after_minutes} minute(s); restarting Windows is not required"
        );
    }

    // Reserve before touching the pipe. A service crash after SETDISPLAYCOUNT
    // must not forget an adapter restart and allow the next process to exhaust
    // Windows' finite UMDF restart budget.
    budget.changes += 1;
    let encoded = serde_json::to_vec(&budget).context("Could not encode driver restart budget")?;
    atomic_write(&path, &encoded).context("Could not save driver restart budget")
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VddMonitor {
    pub id: u32,
    pub name: Option<String>,
    pub enabled: bool,
    pub modes: Vec<VddMode>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VddMode {
    pub width: u32,
    pub height: u32,
    pub refresh_rates: Vec<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum VddDriverCommand {
    Notify(Vec<VddMonitor>),
    Remove(Vec<u32>),
    RemoveAll,
}

/// Check if the Virtual Display Driver named pipe is responsive
pub fn is_driver_pipe_ready() -> bool {
    unsafe {
        use windows::core::PCWSTR;
        use windows::Win32::System::Pipes::WaitNamedPipeW;
        let pipe_wide: Vec<u16> = VDD_PIPE_NAME
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();
        WaitNamedPipeW(PCWSTR(pipe_wide.as_ptr()), 50).as_bool()
    }
}

/// Check if the Virtual Display Driver is installed / ready on this machine
pub fn is_driver_service_running() -> bool {
    is_driver_pipe_ready() || std::path::Path::new(VDD_SETTINGS_PATH).exists()
}

/// Read the current monitor count from C:\VirtualDisplayDriver\vdd_settings.xml
pub fn get_xml_monitor_count() -> Result<u32> {
    let path = std::path::Path::new(VDD_SETTINGS_PATH);
    if !path.exists() {
        anyhow::bail!("vdd_settings.xml not found at {}", VDD_SETTINGS_PATH);
    }
    let content = std::fs::read_to_string(path).context("Failed to read vdd_settings.xml")?;

    let start_tag = "<count>";
    let end_tag = "</count>";
    if let Some(start_idx) = content.find(start_tag) {
        let val_start = start_idx + start_tag.len();
        if let Some(end_rel) = content[val_start..].find(end_tag) {
            let val_end = val_start + end_rel;
            let count_str = content[val_start..val_end].trim();
            let count: u32 = count_str.parse().context("Failed to parse <count> value")?;
            return Ok(count);
        }
    }
    anyhow::bail!("Could not parse <count> from vdd_settings.xml");
}

/// Update the monitor count inside C:\VirtualDisplayDriver\vdd_settings.xml
pub fn set_xml_monitor_count(count: u32) -> Result<()> {
    let path = std::path::Path::new(VDD_SETTINGS_PATH);
    if !path.exists() {
        anyhow::bail!("vdd_settings.xml not found at {}", VDD_SETTINGS_PATH);
    }
    let content = std::fs::read_to_string(path).context("Failed to read vdd_settings.xml")?;

    let updated = update_monitor_count_xml(&content, count)?;
    atomic_write(path, updated.as_bytes())
}

fn update_monitor_count_xml(content: &str, count: u32) -> Result<String> {
    if !(1..=5).contains(&count) {
        bail!("MttVDD monitor count must be between 1 and 5, got {count}");
    }
    let start_tag = "<count>";
    let end_tag = "</count>";
    let start_idx = content
        .find(start_tag)
        .context("Could not find <count> tag in vdd_settings.xml")?;
    let val_start = start_idx + start_tag.len();
    let end_rel = content[val_start..]
        .find(end_tag)
        .context("Could not find </count> tag in vdd_settings.xml")?;
    let val_end = val_start + end_rel;
    let mut updated = String::with_capacity(content.len() + 8);
    updated.push_str(&content[..val_start]);
    updated.push_str(&count.to_string());
    updated.push_str(&content[val_end..]);
    Ok(updated)
}

fn atomic_write(path: &std::path::Path, contents: &[u8]) -> Result<()> {
    use std::io::Write;
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let temp_path = path.with_extension(format!("xml.{}.{}.tmp", std::process::id(), nonce));
    let mut temp = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temp_path)
        .context("Failed to create temporary VDD settings file")?;
    if let Err(error) = temp.write_all(contents).and_then(|_| temp.sync_all()) {
        let _ = std::fs::remove_file(&temp_path);
        return Err(error).context("Failed to write temporary VDD settings file");
    }
    drop(temp);

    let source: Vec<u16> = temp_path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let destination: Vec<u16> = path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    use std::os::windows::ffi::OsStrExt;
    use windows::core::PCWSTR;
    use windows::Win32::Storage::FileSystem::{
        MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
    };
    if let Err(error) = unsafe {
        MoveFileExW(
            PCWSTR(source.as_ptr()),
            PCWSTR(destination.as_ptr()),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    } {
        let _ = std::fs::remove_file(&temp_path);
        return Err(error).context("Failed to atomically replace vdd_settings.xml");
    }
    Ok(())
}

/// Send a raw command to MTTVirtualDisplayPipe encoded as null-terminated UTF-16LE
pub async fn send_pipe_command(command: &str) -> Result<()> {
    let client = ClientOptions::new()
        .open(VDD_PIPE_NAME)
        .context("Could not connect to MTTVirtualDisplayPipe")?;

    let (_, mut writer) = tokio::io::split(client);

    // MikeTheTech driver expects wide string (UTF-16LE) null-terminated
    let cmd_clean = command.trim_end_matches('\0');
    let wide_bytes: Vec<u8> = cmd_clean
        .encode_utf16()
        .chain(std::iter::once(0))
        .flat_map(|c| c.to_le_bytes())
        .collect();

    writer.write_all(&wide_bytes).await?;
    writer.flush().await?;
    Ok(())
}

/// Set virtual monitor count in both vdd_settings.xml and the active Virtual Display Driver
pub async fn set_driver_monitor_count(count: u32) -> Result<()> {
    set_driver_monitor_count_inner(count, false).await
}

/// Restore the previously committed count after a failed or abandoned change.
/// One extra UMDF restart is reserved for this path so normal user actions can
/// never consume the recovery slot.
pub async fn restore_driver_monitor_count(count: u32) -> Result<()> {
    set_driver_monitor_count_inner(count, true).await
}

async fn set_driver_monitor_count_inner(count: u32, recovery: bool) -> Result<()> {
    // MttVDD reinitializes its adapter for every count change. Serializing and
    // spacing those operations prevents Windows from exhausting the UMDF restart
    // budget when a user adds or removes several monitors in quick succession.
    let mut last_change = driver_change_gate().lock().await;
    if let Some(previous) = *last_change {
        let elapsed = previous.elapsed();
        if elapsed < DRIVER_CHANGE_COOLDOWN {
            tokio::time::sleep(DRIVER_CHANGE_COOLDOWN - elapsed).await;
        }
    }

    // Never modify the persistent count when the live driver cannot accept the
    // matching operation. Otherwise a failed request changes the next-boot state
    // and a rollback may trigger a second driver restart immediately afterwards.
    if !is_driver_pipe_ready() {
        bail!("MTTVirtualDisplayPipe is not ready; monitor count was not changed");
    }

    reserve_driver_change(recovery)?;

    // The driver owns the live transaction and also writes the XML itself.
    let cmd = format!("SETDISPLAYCOUNT {}", count);
    let mut pipe_ok = false;

    for attempt in 1..=3 {
        match tokio::time::timeout(std::time::Duration::from_secs(1), send_pipe_command(&cmd)).await
        {
            Ok(Ok(_)) => {
                info!(
                    "Sent '{}' to MTTVirtualDisplayPipe (attempt {})",
                    cmd, attempt
                );
                pipe_ok = true;
                *last_change = Some(std::time::Instant::now());
                break;
            }
            Ok(Err(e)) => {
                debug!("Pipe attempt {} for '{}': {}", attempt, cmd, e);
                tokio::time::sleep(std::time::Duration::from_millis(300)).await;
            }
            Err(_) => {
                warn!("Driver pipe timed out on attempt {} for '{}'", attempt, cmd);
            }
        }
    }

    if !pipe_ok {
        bail!("MTTVirtualDisplayPipe stopped responding; monitor count may not have been applied");
    }

    // SETDISPLAYCOUNT owns the XML update inside MttVDD. Writing the same file
    // here races its SHCreateStreamOnFileEx transaction and can falsely turn a
    // successful live change into an error followed by an unnecessary rollback.
    // The service verifies the resulting live display count before committing.

    Ok(())
}

/// Backwards compatibility helper: spawns real virtual monitors
pub async fn notify_virtual_displays(monitors: &[VddMonitor]) -> Result<()> {
    let count = monitors.iter().filter(|m| m.enabled).count() as u32;
    set_driver_monitor_count(count).await
}

/// Backwards compatibility helper: removes virtual displays
pub async fn remove_virtual_displays(ids: &[u32]) -> Result<()> {
    let current = get_xml_monitor_count().unwrap_or(1);
    let new_count = current.saturating_sub(ids.len() as u32);
    set_driver_monitor_count(new_count).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn xml_monitor_count_preserves_other_settings_and_rejects_driver_invalid_zero() {
        let input =
            "<vdd_settings><monitors><count>3</count></monitors><gpu>default</gpu></vdd_settings>";
        let output = update_monitor_count_xml(input, 5).unwrap();
        assert_eq!(
            output,
            "<vdd_settings><monitors><count>5</count></monitors><gpu>default</gpu></vdd_settings>"
        );
        assert!(update_monitor_count_xml(input, 0).is_err());
        assert!(update_monitor_count_xml(input, 6).is_err());
    }

    #[test]
    fn legacy_and_expired_driver_budgets_reset_without_reboot() {
        let legacy = DriverChangeBudget {
            boot_id: 42,
            changes: 5,
            window_started_unix_ms: 0,
        };
        let normalized = normalize_driver_change_budget(Some(legacy), 42, 1_000_000);
        assert_eq!(normalized.changes, 0);
        assert_eq!(normalized.window_started_unix_ms, 1_000_000);

        let expired = DriverChangeBudget {
            boot_id: 42,
            changes: MAX_NORMAL_DRIVER_CHANGES_PER_WINDOW,
            window_started_unix_ms: 1,
        };
        let now = 1 + DRIVER_CHANGE_BUDGET_WINDOW.as_millis() as u64;
        assert_eq!(
            normalize_driver_change_budget(Some(expired), 42, now).changes,
            0
        );
    }

    #[test]
    fn current_driver_budget_is_preserved_inside_the_window() {
        let saved = DriverChangeBudget {
            boot_id: 42,
            changes: 7,
            window_started_unix_ms: 100,
        };
        let normalized = normalize_driver_change_budget(Some(saved), 42, 101);
        assert_eq!(normalized.changes, 7);
        assert_eq!(normalized.window_started_unix_ms, 100);
    }
}
