use anyhow::{Context, Result};
use multitor_ipc::{DisplayInfo, MonitorConfig, Neighbors, TopologyConfig};
use std::fs;
use std::path::PathBuf;
use tracing::{info, warn};

/// Reconcile persisted UI preferences with the displays that actually exist in
/// the current Windows desktop session. Device names are the identity; numeric
/// IDs are only a convenient presentation/API key.
pub(crate) fn reconcile_detected(config: &mut TopologyConfig, detected_displays: &[DisplayInfo]) {
    let previous_active_id = config.active_monitor_id;
    let active_device = config
        .active_monitor_id
        .and_then(|id| config.monitors.iter().find(|m| m.id == id))
        .map(|m| m.device_name.clone());

    let previous = std::mem::take(&mut config.monitors);
    let mut reconciled = Vec::with_capacity(detected_displays.len());
    let mut used_previous = std::collections::HashSet::new();
    let mut migrated_active_id = None;
    let mut next_id = previous
        .iter()
        .map(|monitor| monitor.id)
        .max()
        .unwrap_or(0)
        .saturating_add(1);

    for display in detected_displays {
        let find_match = |predicate: &dyn Fn(&MonitorConfig) -> bool| {
            previous
                .iter()
                .enumerate()
                .find(|(index, monitor)| !used_previous.contains(index) && predicate(monitor))
                .map(|(index, _)| index)
        };
        let existing_index = find_match(&|m| {
            !m.device_name.is_empty() && m.device_name.eq_ignore_ascii_case(&display.device_name)
        })
        .or_else(|| find_match(&|m| m.device_name.is_empty() && m.id == display.id))
        .or_else(|| {
            find_match(&|m| {
                m.device_name.is_empty()
                    && m.is_virtual == display.is_virtual
                    && m.bounds == display.bounds
            })
        })
        .or_else(|| {
            let candidates: Vec<usize> = previous
                .iter()
                .enumerate()
                .filter(|(index, m)| {
                    !used_previous.contains(index)
                        && m.device_name.is_empty()
                        && m.is_virtual == display.is_virtual
                })
                .map(|(index, _)| index)
                .collect();
            (candidates.len() == 1).then(|| candidates[0])
        })
        .or_else(|| {
            // VDD device ordinals can all change after a driver reinstall
            // (DISPLAY394/395 -> DISPLAY396/397). Migrate the next unmatched virtual
            // identity in logical order instead of appending every renamed output and
            // then asking the driver to create that doubled count on the next startup.
            if !display.is_virtual {
                return None;
            }
            previous
                .iter()
                .enumerate()
                .filter(|(index, monitor)| !used_previous.contains(index) && monitor.is_virtual)
                .min_by_key(|(_, monitor)| monitor.id)
                .map(|(index, _)| index)
        });
        let existing = existing_index.map(|index| {
            used_previous.insert(index);
            &previous[index]
        });

        let assigned_id = existing.map(|monitor| monitor.id).unwrap_or_else(|| {
            let id = next_id;
            next_id = next_id.saturating_add(1);
            id
        });

        if existing
            .and_then(|m| previous_active_id.filter(|id| *id == m.id))
            .is_some()
        {
            migrated_active_id = Some(assigned_id);
        }

        let mut monitor = existing.cloned().unwrap_or_else(|| MonitorConfig {
            id: assigned_id,
            device_name: display.device_name.clone(),
            name: if display.is_primary {
                "MAIN".to_string()
            } else if display.is_virtual {
                format!("Virtual {}", assigned_id)
            } else {
                format!("Screen {}", assigned_id)
            },
            bounds: display.bounds,
            refresh_rate: display.refresh_rate,
            neighbors: Neighbors::default(),
            edge_activation_delay_ms: config.edge_delay_ms,
            is_enabled: true,
            is_virtual: display.is_virtual,
            layout_x: None,
            layout_y: None,
        });

        monitor.id = assigned_id;
        monitor.device_name = display.device_name.clone();
        monitor.bounds = display.bounds;
        monitor.refresh_rate = display.refresh_rate;
        monitor.is_virtual = display.is_virtual;
        monitor.is_enabled = true;
        reconciled.push(monitor);
    }

    // Keep missing monitors as persistent identities. A VDD can briefly disappear
    // while restarting; a physical monitor can be unplugged and later reconnected.
    // Physical outputs are marked offline so they cannot receive cursor transitions,
    // while configured virtual outputs remain desired/enabled for driver restoration.
    for (index, monitor) in previous.iter().enumerate() {
        if !used_previous.contains(&index) {
            let mut missing = monitor.clone();
            if !missing.is_virtual {
                missing.is_enabled = false;
            }
            reconciled.push(missing);
        }
    }

    config.monitors = reconciled;
    normalize_layout_coordinates(config, detected_displays);
    config.active_monitor_id = active_device
        .as_deref()
        .and_then(|device| {
            config
                .monitors
                .iter()
                .find(|m| m.device_name.eq_ignore_ascii_case(device))
                .filter(|monitor| monitor.is_enabled)
                .map(|m| m.id)
        })
        .or(migrated_active_id)
        .or_else(|| {
            active_device
                .as_deref()
                .filter(|device| device.is_empty())
                .and(config.active_monitor_id)
                .filter(|id| config.monitors.iter().any(|m| m.id == *id))
        })
        .or_else(|| {
            detected_displays
                .iter()
                .find(|display| display.is_primary)
                .and_then(|display| {
                    config
                        .monitors
                        .iter()
                        .find(|monitor| {
                            monitor
                                .device_name
                                .eq_ignore_ascii_case(&display.device_name)
                        })
                        .map(|monitor| monitor.id)
                })
        })
        .or_else(|| config.monitors.first().map(|m| m.id));

    compact_monitor_ids(config);
    multitor_ipc::recompute_neighbors(&mut config.monitors, config.wrap_around);
}

/// Applies an edition limit to persisted virtual monitors without deleting their
/// identities. This makes Pro -> Lite reversible: the previous full config is
/// retained by ConfigManager as config.json.bak and surplus entries stay disabled.
pub(crate) fn enforce_virtual_display_limit(
    config: &mut TopologyConfig,
    max_virtual_displays: u32,
) -> bool {
    let mut enabled_virtual_seen = 0_u32;
    let mut changed = false;
    config.monitors.sort_by_key(|monitor| monitor.id);

    for monitor in &mut config.monitors {
        if monitor.is_virtual && monitor.is_enabled {
            enabled_virtual_seen = enabled_virtual_seen.saturating_add(1);
            if enabled_virtual_seen > max_virtual_displays {
                monitor.is_enabled = false;
                changed = true;
            }
        }
    }

    if changed {
        if config.active_monitor_id.is_some_and(|active_id| {
            config
                .monitors
                .iter()
                .find(|monitor| monitor.id == active_id)
                .is_some_and(|monitor| !monitor.is_enabled)
        }) {
            config.active_monitor_id = config
                .monitors
                .iter()
                .find(|monitor| monitor.is_enabled)
                .map(|monitor| monitor.id);
        }
        multitor_ipc::recompute_neighbors(&mut config.monitors, config.wrap_around);
    }

    changed
}

/// Device names are the stable identity; UI IDs are deliberately compact and
/// human-readable. Sorting by the previous logical ID preserves connection/addition
/// order while closing gaps left by removals (1, 2, 4, 6 -> 1, 2, 3, 4).
fn compact_monitor_ids(config: &mut TopologyConfig) {
    config.monitors.sort_by_key(|monitor| monitor.id);
    let previous_active = config.active_monitor_id;
    let mut remapped_active = None;

    for (index, monitor) in config.monitors.iter_mut().enumerate() {
        let old_id = monitor.id;
        let new_id = index as u32 + 1;
        monitor.id = new_id;
        if previous_active == Some(old_id) {
            remapped_active = Some(new_id);
        }
    }

    config.active_monitor_id = remapped_active.or_else(|| {
        config
            .monitors
            .iter()
            .find(|monitor| monitor.is_enabled)
            .map(|monitor| monitor.id)
    });
}

/// Once any monitor has a custom canvas position, every monitor must use the same
/// coordinate space. Mixing saved layout coordinates with live Windows coordinates
/// makes a newly added virtual display drift farther away on every service restart.
fn normalize_layout_coordinates(config: &mut TopologyConfig, detected_displays: &[DisplayInfo]) {
    if !config
        .monitors
        .iter()
        .any(|monitor| monitor.layout_x.is_some() || monitor.layout_y.is_some())
    {
        return;
    }

    let Some(primary_display) = detected_displays
        .iter()
        .find(|display| display.is_primary)
        .or_else(|| detected_displays.first())
    else {
        return;
    };
    let Some(anchor) = config.monitors.iter().find(|monitor| {
        monitor
            .device_name
            .eq_ignore_ascii_case(&primary_display.device_name)
    }) else {
        return;
    };

    let anchor_x = anchor.layout_x.unwrap_or(primary_display.bounds.x);
    let anchor_y = anchor.layout_y.unwrap_or(primary_display.bounds.y);
    let primary_x = primary_display.bounds.x;
    let primary_y = primary_display.bounds.y;

    for monitor in &mut config.monitors {
        monitor.layout_x.get_or_insert_with(|| {
            anchor_x.saturating_add(monitor.bounds.x.saturating_sub(primary_x))
        });
        monitor.layout_y.get_or_insert_with(|| {
            anchor_y.saturating_add(monitor.bounds.y.saturating_sub(primary_y))
        });
    }
}

pub struct ConfigManager {
    config_path: PathBuf,
}

impl ConfigManager {
    pub fn new() -> Self {
        let app_data = std::env::var("APPDATA").unwrap_or_else(|_| ".".to_string());
        let dir = PathBuf::from(app_data).join("VirtualScreens");
        let _ = fs::create_dir_all(&dir);
        let config_path = dir.join("config.json");

        Self { config_path }
    }

    pub fn load_or_create(&self, detected_displays: &[DisplayInfo]) -> TopologyConfig {
        if let Some(mut config) = self.load_saved() {
            info!("Loaded configuration from {:?}", self.config_path);

            if config.pip_scale_percent > 50 || config.pip_scale_percent < 10 {
                config.pip_scale_percent = 15;
            }

            reconcile_detected(&mut config, detected_displays);

            let _ = self.save(&config);
            return config;
        }

        // Generate initial default topology from detected displays
        info!(
            "Generating default topology for {} detected display(s)",
            detected_displays.len()
        );
        let mut monitors = Vec::new();

        for (i, d) in detected_displays.iter().enumerate() {
            let left = if i > 0 {
                Some(detected_displays[i - 1].id)
            } else {
                None
            };
            let right = if i + 1 < detected_displays.len() {
                Some(detected_displays[i + 1].id)
            } else {
                None
            };

            monitors.push(MonitorConfig {
                id: d.id,
                device_name: d.device_name.clone(),
                name: if d.is_primary {
                    "MAIN".to_string()
                } else {
                    format!("Screen {}", d.id)
                },
                bounds: d.bounds,
                refresh_rate: d.refresh_rate,
                neighbors: Neighbors {
                    left,
                    right,
                    top: None,
                    bottom: None,
                },
                edge_activation_delay_ms: 15,
                is_enabled: true,
                is_virtual: d.is_virtual,
                layout_x: None,
                layout_y: None,
            });
        }

        let mut default_config = TopologyConfig {
            monitors,
            active_monitor_id: detected_displays
                .iter()
                .find(|d| d.is_primary)
                .or_else(|| detected_displays.first())
                .map(|d| d.id),
            wrap_around: true,
            pause_switching: false,
            viewport_enabled: true,
            edge_delay_ms: 15,
            cursor_velocity_threshold: 0.0,
            ..Default::default()
        };

        compact_monitor_ids(&mut default_config);
        multitor_ipc::recompute_neighbors(&mut default_config.monitors, default_config.wrap_around);
        let _ = self.save(&default_config);
        default_config
    }

    /// Load the persisted topology without reconciling it against a potentially
    /// incomplete startup display snapshot.
    pub fn load_saved(&self) -> Option<TopologyConfig> {
        let backup_path = self.config_path.with_extension("json.bak");
        for path in [&self.config_path, &backup_path] {
            let Ok(content) = fs::read_to_string(path) else {
                continue;
            };
            match serde_json::from_str(&content) {
                Ok(config) => {
                    if path == &backup_path {
                        warn!("Recovered topology from backup {:?}", backup_path);
                    }
                    return Some(config);
                }
                Err(error) => warn!("Ignoring invalid topology file {:?}: {}", path, error),
            }
        }
        None
    }

    pub fn save(&self, config: &TopologyConfig) -> Result<()> {
        use std::io::Write;
        use std::os::windows::ffi::OsStrExt;
        use windows::core::PCWSTR;
        use windows::Win32::Storage::FileSystem::{
            MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
        };

        let json = serde_json::to_string_pretty(config)?;
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let temporary =
            self.config_path
                .with_extension(format!("json.{}.{}.tmp", std::process::id(), nonce));
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .context("Failed to create temporary config file")?;
        if let Err(error) = file
            .write_all(json.as_bytes())
            .and_then(|_| file.sync_all())
        {
            let _ = fs::remove_file(&temporary);
            return Err(error).context("Failed to write temporary config file");
        }
        drop(file);

        // Preserve only a known-good previous file. A truncated file must never
        // overwrite the recovery copy.
        if let Ok(previous) = fs::read_to_string(&self.config_path) {
            if serde_json::from_str::<TopologyConfig>(&previous).is_ok() {
                fs::write(self.config_path.with_extension("json.bak"), previous)
                    .context("Failed to update config backup")?;
            }
        }

        let source: Vec<u16> = temporary
            .as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        let destination: Vec<u16> = self
            .config_path
            .as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        if let Err(error) = unsafe {
            MoveFileExW(
                PCWSTR(source.as_ptr()),
                PCWSTR(destination.as_ptr()),
                MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
            )
        } {
            let _ = fs::remove_file(&temporary);
            return Err(error).context("Failed to atomically replace config.json");
        }
        info!("Saved configuration to {:?}", self.config_path);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use multitor_ipc::DisplayBounds;

    #[test]
    fn invalid_primary_config_recovers_from_backup() {
        let unique = format!(
            "evertydisplay-config-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        );
        let dir = std::env::temp_dir().join(unique);
        std::fs::create_dir_all(&dir).unwrap();
        let config_path = dir.join("config.json");
        let manager = ConfigManager {
            config_path: config_path.clone(),
        };
        let expected = TopologyConfig::default();
        std::fs::write(&config_path, "{truncated").unwrap();
        std::fs::write(
            config_path.with_extension("json.bak"),
            serde_json::to_vec(&expected).unwrap(),
        )
        .unwrap();

        assert_eq!(manager.load_saved(), Some(expected));

        let _ = std::fs::remove_file(config_path.with_extension("json.bak"));
        let _ = std::fs::remove_file(config_path);
        let _ = std::fs::remove_dir(dir);
    }

    fn display(id: u32, device: &str, primary: bool, virtual_display: bool) -> DisplayInfo {
        DisplayInfo {
            id,
            device_name: device.to_string(),
            friendly_name: device.to_string(),
            bounds: DisplayBounds::new((id as i32 - 1) * 1920, 0, 1920, 1080),
            refresh_rate: 60,
            is_primary: primary,
            is_virtual: virtual_display,
        }
    }

    #[test]
    fn reconciliation_removes_disconnected_and_preserves_identity() {
        let mut config = TopologyConfig {
            monitors: vec![
                MonitorConfig {
                    id: 1,
                    device_name: r"\\.\DISPLAY1".into(),
                    name: "My main".into(),
                    bounds: DisplayBounds::new(0, 0, 800, 600),
                    refresh_rate: 30,
                    neighbors: Neighbors::default(),
                    edge_activation_delay_ms: 25,
                    is_enabled: true,
                    is_virtual: false,
                    layout_x: Some(100),
                    layout_y: Some(200),
                },
                MonitorConfig {
                    id: 2,
                    device_name: r"\\.\DISPLAY2".into(),
                    name: "Gone".into(),
                    bounds: DisplayBounds::new(800, 0, 800, 600),
                    refresh_rate: 30,
                    neighbors: Neighbors::default(),
                    edge_activation_delay_ms: 15,
                    is_enabled: true,
                    is_virtual: true,
                    layout_x: None,
                    layout_y: None,
                },
            ],
            active_monitor_id: Some(2),
            ..Default::default()
        };

        let detected = vec![
            display(7, r"\\.\DISPLAY1", true, false),
            display(3, r"\\.\DISPLAY3", false, true),
        ];
        reconcile_detected(&mut config, &detected);

        assert_eq!(config.monitors.len(), 2);
        assert_eq!(config.monitors[0].id, 1);
        assert_eq!(config.monitors[0].name, "My main");
        assert_eq!(config.monitors[0].layout_x, Some(100));
        assert_eq!(config.monitors[1].id, 2);
        assert_eq!(config.monitors[1].device_name, r"\\.\DISPLAY3");
        assert_eq!(config.monitors[1].name, "Gone");
        assert_eq!(config.active_monitor_id, Some(2));
    }

    #[test]
    fn multiple_renumbered_virtual_outputs_do_not_duplicate_saved_monitors() {
        let virtual_monitor = |id, device: &str, name: &str| MonitorConfig {
            id,
            device_name: device.into(),
            name: name.into(),
            bounds: DisplayBounds::new((id as i32 - 2) * 1920, 0, 1920, 1080),
            refresh_rate: 60,
            neighbors: Neighbors::default(),
            edge_activation_delay_ms: 15,
            is_enabled: true,
            is_virtual: true,
            layout_x: Some((id as i32 - 2) * 1920),
            layout_y: Some(0),
        };
        let mut config = TopologyConfig {
            monitors: vec![
                virtual_monitor(2, r"\\.\DISPLAY394", "Work A"),
                virtual_monitor(3, r"\\.\DISPLAY395", "Work B"),
            ],
            active_monitor_id: Some(2),
            ..Default::default()
        };
        let detected = vec![
            display(8, r"\\.\DISPLAY410", false, true),
            display(9, r"\\.\DISPLAY411", false, true),
        ];

        reconcile_detected(&mut config, &detected);

        assert_eq!(config.monitors.len(), 2);
        assert_eq!(config.monitors[0].name, "Work A");
        assert_eq!(config.monitors[0].device_name, r"\\.\DISPLAY410");
        assert_eq!(config.monitors[1].name, "Work B");
        assert_eq!(config.monitors[1].device_name, r"\\.\DISPLAY411");
    }

    #[test]
    fn temporarily_missing_virtual_monitor_remains_configured() {
        let mut config = TopologyConfig {
            monitors: vec![MonitorConfig {
                id: 4,
                device_name: r"\\.\DISPLAY394".into(),
                name: "Virtual workspace".into(),
                bounds: DisplayBounds::new(-2560, 0, 2560, 1440),
                refresh_rate: 60,
                neighbors: Neighbors::default(),
                edge_activation_delay_ms: 15,
                is_enabled: true,
                is_virtual: true,
                layout_x: Some(0),
                layout_y: Some(0),
            }],
            active_monitor_id: Some(4),
            ..Default::default()
        };
        let detected = vec![display(1, r"\\.\DISPLAY1", true, false)];

        reconcile_detected(&mut config, &detected);

        let virtual_monitor = config
            .monitors
            .iter()
            .find(|monitor| monitor.is_virtual)
            .expect("configured virtual monitor should survive a transient driver restart");
        assert_eq!(virtual_monitor.id, 1);
        assert_eq!(virtual_monitor.layout_x, Some(0));
    }

    #[test]
    fn lite_limit_disables_surplus_virtual_monitors_without_deleting_identity() {
        let monitor = |id, virtual_display| MonitorConfig {
            id,
            device_name: format!(r"\\.\DISPLAY{id}"),
            name: format!("Display {id}"),
            bounds: DisplayBounds::new((id as i32 - 1) * 1920, 0, 1920, 1080),
            refresh_rate: 60,
            neighbors: Neighbors::default(),
            edge_activation_delay_ms: 15,
            is_enabled: true,
            is_virtual: virtual_display,
            layout_x: None,
            layout_y: None,
        };
        let mut config = TopologyConfig {
            monitors: vec![monitor(1, false), monitor(2, true), monitor(3, true)],
            active_monitor_id: Some(3),
            ..Default::default()
        };

        assert!(enforce_virtual_display_limit(&mut config, 1));
        assert_eq!(config.monitors.len(), 3);
        assert!(config.monitors[1].is_enabled);
        assert!(!config.monitors[2].is_enabled);
        assert_eq!(config.active_monitor_id, Some(1));
        assert!(!enforce_virtual_display_limit(&mut config, 1));
    }

    #[test]
    fn reconnected_physical_monitor_restores_saved_identity_and_layout() {
        let mut config = TopologyConfig {
            monitors: vec![MonitorConfig {
                id: 7,
                device_name: r"\\.\DISPLAY7".into(),
                name: "Desk monitor".into(),
                bounds: DisplayBounds::new(1920, 0, 1920, 1080),
                refresh_rate: 60,
                neighbors: Neighbors::default(),
                edge_activation_delay_ms: 42,
                is_enabled: false,
                is_virtual: false,
                layout_x: Some(3000),
                layout_y: Some(120),
            }],
            ..Default::default()
        };
        let detected = vec![DisplayInfo {
            bounds: DisplayBounds::new(2560, 0, 1920, 1080),
            refresh_rate: 75,
            ..display(19, r"\\.\DISPLAY7", false, false)
        }];

        reconcile_detected(&mut config, &detected);

        let monitor = &config.monitors[0];
        assert!(monitor.is_enabled);
        assert_eq!(monitor.id, 1);
        assert_eq!(monitor.name, "Desk monitor");
        assert_eq!(monitor.layout_x, Some(3000));
        assert_eq!(monitor.layout_y, Some(120));
        assert_eq!(monitor.refresh_rate, 75);
    }

    #[test]
    fn logical_monitor_ids_are_compacted_without_losing_active_identity() {
        let detected = vec![
            display(11, r"\\.\DISPLAY1", true, false),
            display(12, r"\\.\DISPLAY2", false, false),
            display(40, r"\\.\DISPLAY40", false, true),
            display(60, r"\\.\DISPLAY60", false, true),
        ];
        let mut config = TopologyConfig {
            monitors: detected
                .iter()
                .zip([1, 2, 4, 6])
                .map(|(display, id)| MonitorConfig {
                    id,
                    device_name: display.device_name.clone(),
                    name: format!("Saved {id}"),
                    bounds: display.bounds,
                    refresh_rate: display.refresh_rate,
                    neighbors: Neighbors::default(),
                    edge_activation_delay_ms: 15,
                    is_enabled: true,
                    is_virtual: display.is_virtual,
                    layout_x: Some(display.bounds.x),
                    layout_y: Some(display.bounds.y),
                })
                .collect(),
            active_monitor_id: Some(6),
            ..Default::default()
        };

        reconcile_detected(&mut config, &detected);

        assert_eq!(
            config
                .monitors
                .iter()
                .map(|monitor| monitor.id)
                .collect::<Vec<_>>(),
            vec![1, 2, 3, 4]
        );
        assert_eq!(config.active_monitor_id, Some(4));
        assert_eq!(config.monitors[3].name, "Saved 6");
    }

    #[test]
    fn legacy_virtual_monitor_is_migrated_to_windows_device_name() {
        let mut config = TopologyConfig {
            monitors: vec![MonitorConfig {
                id: 3,
                device_name: String::new(),
                name: "Workspace".into(),
                bounds: DisplayBounds::new(4480, 0, 2560, 1440),
                refresh_rate: 60,
                neighbors: Neighbors::default(),
                edge_activation_delay_ms: 15,
                is_enabled: true,
                is_virtual: true,
                layout_x: Some(-1684),
                layout_y: Some(2240),
            }],
            active_monitor_id: Some(3),
            ..Default::default()
        };
        let detected = vec![DisplayInfo {
            id: 378,
            device_name: r"\\.\DISPLAY378".into(),
            friendly_name: "MttVDD".into(),
            bounds: DisplayBounds::new(4480, 0, 2560, 1440),
            refresh_rate: 60,
            is_primary: false,
            is_virtual: true,
        }];

        reconcile_detected(&mut config, &detected);

        assert_eq!(config.monitors[0].id, 1);
        assert_eq!(config.monitors[0].device_name, r"\\.\DISPLAY378");
        assert_eq!(config.monitors[0].name, "Workspace");
        assert_eq!(config.monitors[0].layout_x, Some(-1684));
        assert_eq!(config.active_monitor_id, Some(1));
    }

    #[test]
    fn partial_canvas_layout_is_normalized_without_startup_drift() {
        let detected = vec![
            DisplayInfo {
                bounds: DisplayBounds::new(0, 0, 2560, 1440),
                ..display(1, r"\\.\DISPLAY1", true, false)
            },
            DisplayInfo {
                bounds: DisplayBounds::new(-2560, 0, 2560, 1440),
                ..display(388, r"\\.\DISPLAY388", false, true)
            },
        ];
        let mut config = TopologyConfig {
            monitors: vec![
                MonitorConfig {
                    id: 1,
                    device_name: r"\\.\DISPLAY1".into(),
                    name: "MAIN".into(),
                    bounds: detected[0].bounds,
                    refresh_rate: 60,
                    neighbors: Neighbors::default(),
                    edge_activation_delay_ms: 15,
                    is_enabled: true,
                    is_virtual: false,
                    layout_x: Some(2560),
                    layout_y: Some(0),
                },
                MonitorConfig {
                    id: 4,
                    device_name: r"\\.\DISPLAY388".into(),
                    name: "Virtual".into(),
                    bounds: detected[1].bounds,
                    refresh_rate: 60,
                    neighbors: Neighbors::default(),
                    edge_activation_delay_ms: 15,
                    is_enabled: true,
                    is_virtual: true,
                    layout_x: None,
                    layout_y: None,
                },
            ],
            ..Default::default()
        };

        reconcile_detected(&mut config, &detected);
        assert_eq!(config.monitors[1].layout_x, Some(0));
        assert_eq!(config.monitors[1].layout_y, Some(0));

        let once = config.monitors[1].layout_bounds();
        reconcile_detected(&mut config, &detected);
        assert_eq!(config.monitors[1].layout_bounds(), once);
    }
}
