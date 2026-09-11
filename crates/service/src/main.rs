#![windows_subsystem = "windows"]

mod autostart;
mod config;
mod cursor;
mod foreground_watcher;
mod hotkey;
mod ipc_server;
mod localization;
mod osd;
mod topology;
mod tray;
mod window_manager;

use anyhow::Result;
use config::ConfigManager;
use cursor::CursorTracker;
use foreground_watcher::{ForegroundEvent, ForegroundWatcher};
use hotkey::{HotkeyAction, HotkeyManager};
use ipc_server::{IpcServer, ServiceCommand};
use multitor_ipc::{
    DisplayBounds, DisplayInfo, EdgeDirection, PipPlacementConfig, SwitchReason, TopologyConfig,
    VirtualWindowActivationAction,
};
use multitor_renderer::{PipAction, ViewportRenderer, ViewportWindow};
use osd::{OsdKind, OsdNotifier};
use std::collections::HashMap;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use tracing::{error, info, warn};
use tray::{TrayEvent, TrayManager};
use window_manager::WindowManager;

const PRODUCT_CAPABILITIES: multitor_ipc::ProductCapabilities =
    everty_product_policy::ACTIVE_CAPABILITIES;
const DEFAULT_VIRTUAL_REFRESH_RATE: u32 = 60;

struct PendingMonitorConfirmation {
    id: u32,
    previous_virtual_count: u32,
    previous_modes: Vec<VirtualModePreference>,
    expires_at: Instant,
}

struct PendingForegroundActivation {
    previous_window: isize,
    target_window: isize,
    target_monitor_id: u32,
    target_is_virtual: bool,
    require_foreground: bool,
    ready_at: Instant,
}

#[derive(Clone)]
struct VirtualModePreference {
    width: u32,
    height: u32,
    refresh_rate: u32,
}

fn virtual_modes_in_driver_order(
    topology: &TopologyConfig,
    displays: &[DisplayInfo],
) -> Vec<VirtualModePreference> {
    displays
        .iter()
        .filter(|display| display.is_virtual)
        .filter_map(|display| {
            topology
                .monitors
                .iter()
                .find(|monitor| {
                    monitor
                        .device_name
                        .eq_ignore_ascii_case(&display.device_name)
                })
                .map(|monitor| VirtualModePreference {
                    width: monitor.bounds.width,
                    height: monitor.bounds.height,
                    refresh_rate: monitor.refresh_rate,
                })
        })
        .collect()
}

fn restore_virtual_modes(displays: &[DisplayInfo], modes: &[VirtualModePreference]) {
    for (output, mode) in displays
        .iter()
        .filter(|display| display.is_virtual)
        .zip(modes)
    {
        if let Err(exact_error) = multitor_driver_manager::set_display_mode(
            &output.device_name,
            mode.width,
            mode.height,
            mode.refresh_rate,
        ) {
            match multitor_driver_manager::set_display_mode(
                &output.device_name,
                mode.width,
                mode.height,
                0,
            ) {
                Ok(()) => info!(
                    "Restored {}x{} for {} using its highest available refresh rate ({} Hz was unavailable)",
                    mode.width, mode.height, output.device_name, mode.refresh_rate
                ),
                Err(fallback_error) => warn!(
                    "Could not restore mode {}x{} @ {} Hz for {}: {exact_error:#}; resolution fallback also failed: {fallback_error:#}",
                    mode.width, mode.height, mode.refresh_rate, output.device_name
                ),
            }
        }
    }
}

/// Match managed virtual outputs to the primary physical display while preserving
/// each output's refresh rate. Pixel dimensions remove capture resampling and blur;
/// copying a high-refresh MAIN mode would needlessly make new virtual outputs start
/// at 144/240+ Hz.
fn synchronized_virtual_mode(
    primary: &DisplayInfo,
    output: &DisplayInfo,
) -> Option<(u32, u32, u32)> {
    (output.bounds.width != primary.bounds.width || output.bounds.height != primary.bounds.height)
        .then_some((
            primary.bounds.width,
            primary.bounds.height,
            output.refresh_rate,
        ))
}

fn synchronize_virtual_modes_to_primary(displays: &[DisplayInfo]) -> bool {
    let Some(primary) = displays
        .iter()
        .find(|display| display.is_primary && !display.is_virtual)
        .or_else(|| displays.iter().find(|display| !display.is_virtual))
    else {
        warn!("Cannot synchronize virtual modes: no physical display is active");
        return false;
    };

    let mut changed = false;
    for output in displays.iter().filter(|display| display.is_virtual) {
        let Some((target_width, target_height, target_refresh_rate)) =
            synchronized_virtual_mode(primary, output)
        else {
            continue;
        };

        let exact = multitor_driver_manager::set_display_mode(
            &output.device_name,
            target_width,
            target_height,
            target_refresh_rate,
        );
        match exact {
            Ok(()) => {
                changed = true;
                info!(
                    "Matched {} to MAIN resolution {}x{} while preserving {} Hz",
                    output.device_name, target_width, target_height, target_refresh_rate
                );
            }
            Err(exact_error) => {
                match multitor_driver_manager::set_display_mode(
                    &output.device_name,
                    target_width,
                    target_height,
                    DEFAULT_VIRTUAL_REFRESH_RATE,
                ) {
                    Ok(()) => {
                        changed = true;
                        info!(
                            "Matched {} to MAIN resolution {}x{} using the safe 60 Hz fallback",
                            output.device_name, target_width, target_height
                        );
                    }
                    Err(fallback_error) => warn!(
                        "Could not match {} to MAIN {}x{} while preserving {} Hz: {exact_error:#}; 60 Hz fallback failed: {fallback_error:#}",
                        output.device_name,
                        target_width,
                        target_height,
                        target_refresh_rate
                    ),
                }
            }
        }
    }
    changed
}

fn default_pip_bounds(
    host: &DisplayInfo,
    source: &DisplayInfo,
    scale_percent: u32,
) -> DisplayBounds {
    let scale = scale_percent.clamp(10, 50) as f32 / 100.0;
    let max_width = (host.bounds.width as f32 * scale).max(240.0);
    let max_height = (host.bounds.height as f32 * scale).max(135.0);
    let source_aspect = source.bounds.width.max(1) as f32 / source.bounds.height.max(1) as f32;
    let mut width = max_width;
    let mut height = width / source_aspect;
    if height > max_height {
        height = max_height;
        width = height * source_aspect;
    }
    let width = width.round().max(1.0) as u32;
    let height = height.round().max(1.0) as u32;
    DisplayBounds::new(
        host.bounds.right() - width as i32 - 24,
        host.bounds.bottom() - height as i32 - 48,
        width,
        height,
    )
}

fn clamp_pip_bounds(mut bounds: DisplayBounds, displays: &[DisplayInfo]) -> DisplayBounds {
    let physical: Vec<&DisplayInfo> = displays
        .iter()
        .filter(|display| !display.is_virtual)
        .collect();
    if physical.is_empty() {
        return bounds;
    }

    let center_x = i64::from(bounds.x) + i64::from(bounds.width) / 2;
    let center_y = i64::from(bounds.y) + i64::from(bounds.height) / 2;
    let host = physical
        .iter()
        .copied()
        .find(|display| {
            center_x >= i64::from(display.bounds.x)
                && center_x < i64::from(display.bounds.right())
                && center_y >= i64::from(display.bounds.y)
                && center_y < i64::from(display.bounds.bottom())
        })
        .or_else(|| physical.iter().copied().find(|display| display.is_primary))
        .unwrap_or(physical[0]);

    let max_width = host.bounds.width.saturating_sub(32).max(1);
    let max_height = host.bounds.height.saturating_sub(64).max(1);
    let scale = (max_width as f32 / bounds.width.max(1) as f32)
        .min(max_height as f32 / bounds.height.max(1) as f32)
        .min(1.0);
    bounds.width = (bounds.width.max(1) as f32 * scale).round().max(1.0) as u32;
    bounds.height = (bounds.height.max(1) as f32 * scale).round().max(1.0) as u32;
    bounds.x = bounds.x.clamp(
        host.bounds.x,
        host.bounds.right().saturating_sub(bounds.width as i32),
    );
    bounds.y = bounds.y.clamp(
        host.bounds.y,
        host.bounds.bottom().saturating_sub(bounds.height as i32),
    );
    bounds
}

fn remember_pip_bounds(config: &mut TopologyConfig, source_device: &str, bounds: DisplayBounds) {
    let placement = PipPlacementConfig {
        source_device_name: source_device.to_string(),
        x: bounds.x,
        y: bounds.y,
        width: bounds.width,
        height: bounds.height,
    };
    if let Some(saved) = config
        .pip_placements
        .iter_mut()
        .find(|saved| saved.source_device_name.eq_ignore_ascii_case(source_device))
    {
        *saved = placement;
    } else {
        config.pip_placements.push(placement);
    }
}

/// Capture the actual window rectangle immediately before hiding PiP. Periodic
/// geometry polling is useful during dragging, but it must not be the only
/// persistence path because a user can move and hide the window within 250 ms.
fn remember_current_pip_window(
    renderer: Option<&ViewportRenderer>,
    source_id: Option<u32>,
    topology: &mut topology::TopologyManager,
) -> bool {
    let Some(renderer) = renderer.filter(|renderer| renderer.is_pip()) else {
        return false;
    };
    let Some(bounds) = renderer.window().bounds() else {
        return false;
    };
    let Some(source_device) = source_id.and_then(|id| {
        topology
            .get_monitor(id)
            .map(|monitor| monitor.device_name.clone())
    }) else {
        return false;
    };
    remember_pip_bounds(topology.config_mut(), &source_device, bounds);
    true
}

fn saved_pip_bounds(
    config: &TopologyConfig,
    source_device: &str,
    displays: &[DisplayInfo],
) -> Option<DisplayBounds> {
    config
        .pip_placements
        .iter()
        .find(|saved| saved.source_device_name.eq_ignore_ascii_case(source_device))
        .map(|saved| {
            clamp_pip_bounds(
                DisplayBounds::new(saved.x, saved.y, saved.width, saved.height),
                displays,
            )
        })
}

#[cfg(test)]
mod pip_state_tests {
    use super::*;

    fn display(name: &str, x: i32, width: u32, primary: bool) -> DisplayInfo {
        DisplayInfo {
            id: 1,
            device_name: name.into(),
            friendly_name: name.into(),
            bounds: DisplayBounds::new(x, 0, width, 1080),
            refresh_rate: 60,
            is_primary: primary,
            is_virtual: false,
        }
    }

    #[test]
    fn pip_geometry_is_kept_independently_for_each_virtual_source() {
        let mut config = TopologyConfig::default();
        remember_pip_bounds(
            &mut config,
            r"\\.\DISPLAY10",
            DisplayBounds::new(120, 80, 640, 360),
        );
        remember_pip_bounds(
            &mut config,
            r"\\.\DISPLAY11",
            DisplayBounds::new(2100, 100, 480, 270),
        );
        remember_pip_bounds(
            &mut config,
            r"\\.\display10",
            DisplayBounds::new(160, 90, 800, 450),
        );

        assert_eq!(config.pip_placements.len(), 2);
        let first = config
            .pip_placements
            .iter()
            .find(|saved| {
                saved
                    .source_device_name
                    .eq_ignore_ascii_case(r"\\.\DISPLAY10")
            })
            .unwrap();
        assert_eq!(
            (first.x, first.y, first.width, first.height),
            (160, 90, 800, 450)
        );
    }

    #[test]
    fn stale_pip_position_is_clamped_to_an_available_physical_display() {
        let displays = vec![display(r"\\.\DISPLAY1", 0, 1920, true)];
        let restored = clamp_pip_bounds(DisplayBounds::new(-4200, 1700, 2560, 1440), &displays);

        assert!(restored.x >= 0);
        assert!(restored.y >= 0);
        assert!(restored.right() <= 1920);
        assert!(restored.bottom() <= 1080);
        let aspect = restored.width as f32 / restored.height as f32;
        assert!((aspect - 16.0 / 9.0).abs() < 0.01);
    }
}

fn native_layout_positions(
    topology: &TopologyConfig,
    displays: &[DisplayInfo],
) -> std::result::Result<Vec<(String, i32, i32)>, String> {
    if topology.monitors.is_empty()
        || !topology
            .monitors
            .iter()
            .any(|monitor| monitor.layout_x.is_some() || monitor.layout_y.is_some())
    {
        return Ok(Vec::new());
    }

    let primary = displays
        .iter()
        .find(|display| display.is_primary)
        .or_else(|| displays.first())
        .ok_or_else(|| "Windows did not report an active display".to_string())?;
    let anchor = topology
        .monitors
        .iter()
        .find(|monitor| {
            monitor
                .device_name
                .eq_ignore_ascii_case(&primary.device_name)
        })
        .ok_or_else(|| "Primary display is missing from the topology".to_string())?;
    let anchor_layout = anchor.layout_bounds();

    let mut positions = Vec::with_capacity(topology.monitors.len());
    let mut rectangles = Vec::with_capacity(topology.monitors.len());
    let mut needs_change = false;
    for monitor in topology
        .monitors
        .iter()
        .filter(|monitor| monitor.is_enabled)
    {
        let Some(display) = displays.iter().find(|display| {
            display
                .device_name
                .eq_ignore_ascii_case(&monitor.device_name)
        }) else {
            continue;
        };
        let layout = monitor.layout_bounds();
        let x = i64::from(primary.bounds.x) + i64::from(layout.x) - i64::from(anchor_layout.x);
        let y = i64::from(primary.bounds.y) + i64::from(layout.y) - i64::from(anchor_layout.y);
        let x = i32::try_from(x).map_err(|_| "Display X coordinate is out of range".to_string())?;
        let y = i32::try_from(y).map_err(|_| "Display Y coordinate is out of range".to_string())?;
        needs_change |= display.bounds.x != x || display.bounds.y != y;
        positions.push((monitor.device_name.clone(), x, y));
        rectangles.push((
            monitor.id,
            x,
            y,
            monitor.bounds.width,
            monitor.bounds.height,
        ));
    }

    for (index, first) in rectangles.iter().enumerate() {
        for second in rectangles.iter().skip(index + 1) {
            let overlaps = first.1 < second.1 + second.3 as i32
                && first.1 + first.3 as i32 > second.1
                && first.2 < second.2 + second.4 as i32
                && first.2 + first.4 as i32 > second.2;
            if overlaps {
                return Err(format!(
                    "Monitors {} and {} overlap; separate them before applying the layout",
                    first.0, second.0
                ));
            }
        }
    }
    if needs_change {
        Ok(positions)
    } else {
        Ok(Vec::new())
    }
}

fn apply_native_layout(
    topology: &TopologyConfig,
    displays: &[DisplayInfo],
) -> std::result::Result<bool, String> {
    let positions = native_layout_positions(topology, displays)?;
    if positions.is_empty() {
        return Ok(false);
    }
    multitor_driver_manager::set_display_layout(&positions).map_err(|error| error.to_string())?;
    Ok(true)
}

fn apply_current_topology_layout(
    topology: &mut topology::TopologyManager,
    detected_displays: &mut Vec<DisplayInfo>,
    primary_display: &mut DisplayInfo,
) -> std::result::Result<bool, String> {
    let changed = apply_native_layout(topology.config(), detected_displays)?;
    if changed {
        *detected_displays =
            multitor_driver_manager::enumerate_displays().map_err(|error| error.to_string())?;
        config::reconcile_detected(topology.config_mut(), detected_displays);
        if let Some(primary) = detected_displays.iter().find(|display| display.is_primary) {
            *primary_display = primary.clone();
        }
    }
    Ok(changed)
}

fn renderer_result_is_fatal(result: anyhow::Result<bool>, consecutive_errors: &mut u8) -> bool {
    match result {
        Ok(_) => {
            *consecutive_errors = 0;
            false
        }
        Err(error) => {
            *consecutive_errors = consecutive_errors.saturating_add(1);
            warn!(
                "Viewport renderer error ({}/5): {error:?}",
                *consecutive_errors
            );
            *consecutive_errors >= 5
        }
    }
}

fn should_show_monitor_switch_osd(
    config: &TopologyConfig,
    from_id: Option<u32>,
    to_id: u32,
) -> bool {
    if !config.osd_hide_physical_to_physical {
        return true;
    }
    let from_is_physical = from_id
        .and_then(|id| config.monitors.iter().find(|monitor| monitor.id == id))
        .map(|monitor| !monitor.is_virtual)
        .unwrap_or(false);
    let to_is_physical = config
        .monitors
        .iter()
        .find(|monitor| monitor.id == to_id)
        .map(|monitor| !monitor.is_virtual)
        .unwrap_or(false);
    !(from_is_physical && to_is_physical)
}

fn should_focus_after_monitor_switch(
    smart_focus_enabled: bool,
    target_is_virtual: bool,
    needs_virtual_handoff: bool,
) -> bool {
    needs_virtual_handoff || (smart_focus_enabled && target_is_virtual)
}

fn should_follow_activated_window(
    active_is_virtual: Option<bool>,
    target_is_virtual: bool,
    virtual_action: VirtualWindowActivationAction,
    follow_physical: bool,
    caused_by_minimize: bool,
) -> bool {
    if caused_by_minimize {
        return false;
    }
    match active_is_virtual {
        Some(false) => target_is_virtual && virtual_action != VirtualWindowActivationAction::None,
        Some(true) => !target_is_virtual && follow_physical,
        None => false,
    }
}

#[cfg(test)]
mod native_layout_tests {
    use super::*;
    use multitor_ipc::{DisplayBounds, MonitorConfig, Neighbors};

    fn monitor(id: u32, device: &str, bounds: DisplayBounds, layout_x: i32) -> MonitorConfig {
        MonitorConfig {
            id,
            device_name: device.into(),
            name: format!("Monitor {id}"),
            bounds,
            refresh_rate: 60,
            neighbors: Neighbors::default(),
            edge_activation_delay_ms: 15,
            is_enabled: true,
            is_virtual: id == 3,
            layout_x: Some(layout_x),
            layout_y: Some(2240),
        }
    }

    #[test]
    fn main_resolution_sync_never_copies_a_high_refresh_rate() {
        let primary = DisplayInfo {
            id: 1,
            device_name: r"\\.\DISPLAY1".into(),
            friendly_name: "Main".into(),
            bounds: DisplayBounds::new(0, 0, 2560, 1440),
            refresh_rate: 244,
            is_primary: true,
            is_virtual: false,
        };
        let mut virtual_display = DisplayInfo {
            id: 2,
            device_name: r"\\.\DISPLAY2".into(),
            friendly_name: "Virtual".into(),
            bounds: DisplayBounds::new(2560, 0, 1920, 1080),
            refresh_rate: 60,
            is_primary: false,
            is_virtual: true,
        };

        assert_eq!(
            synchronized_virtual_mode(&primary, &virtual_display),
            Some((2560, 1440, 60))
        );

        virtual_display.bounds.width = 2560;
        virtual_display.bounds.height = 1440;
        virtual_display.refresh_rate = 144;
        assert_eq!(synchronized_virtual_mode(&primary, &virtual_display), None);
    }

    #[test]
    fn canvas_layout_is_anchored_to_primary_windows_position() {
        let displays = vec![
            DisplayInfo {
                id: 1,
                device_name: r"\\.\DISPLAY1".into(),
                friendly_name: "Main".into(),
                bounds: DisplayBounds::new(0, 0, 2560, 1440),
                refresh_rate: 60,
                is_primary: true,
                is_virtual: false,
            },
            DisplayInfo {
                id: 2,
                device_name: r"\\.\DISPLAY2".into(),
                friendly_name: "Second".into(),
                bounds: DisplayBounds::new(2560, 0, 1920, 1080),
                refresh_rate: 60,
                is_primary: false,
                is_virtual: false,
            },
            DisplayInfo {
                id: 3,
                device_name: r"\\.\DISPLAY378".into(),
                friendly_name: "Virtual".into(),
                bounds: DisplayBounds::new(4480, 0, 2560, 1440),
                refresh_rate: 60,
                is_primary: false,
                is_virtual: true,
            },
        ];
        let topology = TopologyConfig {
            monitors: vec![
                monitor(1, r"\\.\DISPLAY1", displays[0].bounds, 876),
                monitor(2, r"\\.\DISPLAY2", displays[1].bounds, 3436),
                monitor(3, r"\\.\DISPLAY378", displays[2].bounds, -1684),
            ],
            ..Default::default()
        };

        let positions = native_layout_positions(&topology, &displays).unwrap();
        assert_eq!(positions[0], (r"\\.\DISPLAY1".into(), 0, 0));
        assert_eq!(positions[1], (r"\\.\DISPLAY2".into(), 2560, 0));
        assert_eq!(positions[2], (r"\\.\DISPLAY378".into(), -2560, 0));
    }

    #[test]
    fn physical_only_osd_filter_preserves_virtual_transitions() {
        let mut config = TopologyConfig {
            monitors: vec![
                monitor(1, r"\\.\DISPLAY1", DisplayBounds::new(0, 0, 1920, 1080), 0),
                monitor(
                    2,
                    r"\\.\DISPLAY2",
                    DisplayBounds::new(1920, 0, 1920, 1080),
                    1920,
                ),
                monitor(
                    3,
                    r"\\.\DISPLAY3",
                    DisplayBounds::new(3840, 0, 1920, 1080),
                    3840,
                ),
            ],
            osd_hide_physical_to_physical: true,
            ..Default::default()
        };
        config.monitors[2].is_virtual = true;

        assert!(!should_show_monitor_switch_osd(&config, Some(1), 2));
        assert!(should_show_monitor_switch_osd(&config, Some(1), 3));
        assert!(should_show_monitor_switch_osd(&config, Some(3), 1));
        assert!(should_show_monitor_switch_osd(&config, None, 1));

        config.osd_hide_physical_to_physical = false;
        assert!(should_show_monitor_switch_osd(&config, Some(1), 2));
    }

    #[test]
    fn smart_focus_never_steals_focus_between_physical_displays() {
        assert!(!should_focus_after_monitor_switch(true, false, false));
        assert!(should_focus_after_monitor_switch(true, true, false));
        assert!(should_focus_after_monitor_switch(false, false, true));
    }

    #[test]
    fn window_activation_following_is_directional_and_ignores_minimize() {
        assert!(should_follow_activated_window(
            Some(true),
            false,
            VirtualWindowActivationAction::None,
            true,
            false,
        ));
        assert!(!should_follow_activated_window(
            Some(true),
            false,
            VirtualWindowActivationAction::None,
            false,
            false,
        ));
        assert!(should_follow_activated_window(
            Some(false),
            true,
            VirtualWindowActivationAction::SwitchToVirtual,
            true,
            false,
        ));
        assert!(!should_follow_activated_window(
            Some(false),
            false,
            VirtualWindowActivationAction::SwitchToVirtual,
            true,
            false,
        ));
        assert!(!should_follow_activated_window(
            Some(true),
            false,
            VirtualWindowActivationAction::None,
            true,
            true,
        ));
    }
}

fn switch_and_teleport_monitor(
    topology: &mut topology::TopologyManager,
    target_id: u32,
    reason: SwitchReason,
    osd: &OsdNotifier,
) -> Result<(), String> {
    if let Some(target_mon) = topology
        .get_monitor(target_id)
        .filter(|m| m.is_enabled)
        .cloned()
    {
        let previous_id = topology.config().active_monitor_id;
        let cx = target_mon.bounds.x + target_mon.bounds.width as i32 / 2;
        let cy = target_mon.bounds.y + target_mon.bounds.height as i32 / 2;
        CursorTracker::clip_cursor_to(None);
        if !CursorTracker::teleport_cursor(cx, cy) {
            return Err(format!(
                "Windows rejected cursor move to monitor {}",
                target_id
            ));
        }
        topology.set_active_monitor(target_id, reason);
        WindowManager::focus_window_at_point(cx, cy);
        if should_show_monitor_switch_osd(topology.config(), previous_id, target_id) {
            osd.show_monitor_switch(
                format!("🖥 Экран #{}: {}", target_id, target_mon.name),
                format!(
                    "{}x{} @ {} Гц",
                    target_mon.bounds.width, target_mon.bounds.height, target_mon.refresh_rate
                ),
                &topology.config().monitors,
                target_id,
                topology.config().osd_show_layout,
            );
        }
        Ok(())
    } else {
        Err(format!(
            "Monitor {} is not connected or is disabled",
            target_id
        ))
    }
}

fn apply_virtual_window_activation(
    action: VirtualWindowActivationAction,
    topology: &mut topology::TopologyManager,
    osd: &OsdNotifier,
) {
    match action {
        VirtualWindowActivationAction::None => {}
        VirtualWindowActivationAction::MoveToPhysical => {
            let _ = WindowManager::bring_foreground_virtual_window_to_active_physical(topology);
        }
        VirtualWindowActivationAction::SwitchToVirtual => {
            let previous_id = topology.config().active_monitor_id;
            let user_is_on_physical = topology
                .active_monitor()
                .map(|monitor| monitor.is_enabled && !monitor.is_virtual)
                .unwrap_or(false);
            if !user_is_on_physical {
                return;
            }

            let Some(target) = WindowManager::foreground_virtual_window(topology) else {
                return;
            };
            let target_monitor = topology.get_monitor(target.monitor_id).cloned();
            CursorTracker::clip_cursor_to(None);
            if !CursorTracker::teleport_cursor(target.cursor_x, target.cursor_y) {
                warn!(
                    "Windows rejected cursor move to virtual monitor {} for activated window",
                    target.monitor_id
                );
                return;
            }

            topology.set_active_monitor(target.monitor_id, SwitchReason::Ui);
            info!(
                "Switched to virtual monitor {} for activated window '{}'",
                target.monitor_id, target.title
            );
            if let Some(monitor) = target_monitor {
                if should_show_monitor_switch_osd(topology.config(), previous_id, target.monitor_id)
                {
                    osd.show_monitor_switch(
                        format!("🖥 Экран #{}: {}", monitor.id, monitor.name),
                        format!(
                            "{}x{} @ {} Гц",
                            monitor.bounds.width, monitor.bounds.height, monitor.refresh_rate
                        ),
                        &topology.config().monitors,
                        monitor.id,
                        topology.config().osd_show_layout,
                    );
                }
            }
        }
    }
}

fn apply_physical_window_activation(
    topology: &mut topology::TopologyManager,
    target_window: isize,
    expected_monitor_id: u32,
    activate_target: bool,
    osd: &OsdNotifier,
) -> bool {
    let user_is_on_virtual = topology
        .active_monitor()
        .map(|monitor| monitor.is_enabled && monitor.is_virtual)
        .unwrap_or(false);
    if !user_is_on_virtual {
        return false;
    }

    let Some(target) = WindowManager::window_location(target_window, topology)
        .filter(|window| !window.is_virtual && window.monitor_id == expected_monitor_id)
    else {
        return false;
    };
    let Some(target_monitor) = topology.get_monitor(target.monitor_id).cloned() else {
        return false;
    };
    let previous_id = topology.config().active_monitor_id;
    if activate_target && !WindowManager::activate_window(target_window) {
        warn!(
            "Could not explicitly activate browser window on physical monitor {}; continuing display handoff",
            target.monitor_id
        );
    }
    CursorTracker::clip_cursor_to(None);
    if !CursorTracker::teleport_cursor(target.cursor_x, target.cursor_y) {
        warn!(
            "Windows rejected cursor move to physical monitor {} for activated window",
            target.monitor_id
        );
        return false;
    }

    topology.set_active_monitor(target.monitor_id, SwitchReason::Ui);
    info!(
        "Followed activated window '{}' from virtual workspace to physical monitor {}",
        target.title, target.monitor_id
    );
    if should_show_monitor_switch_osd(topology.config(), previous_id, target.monitor_id) {
        osd.show_monitor_switch(
            format!("🖥 Экран #{}: {}", target_monitor.id, target_monitor.name),
            format!(
                "{}x{} @ {} Гц",
                target_monitor.bounds.width,
                target_monitor.bounds.height,
                target_monitor.refresh_rate
            ),
            &topology.config().monitors,
            target_monitor.id,
            topology.config().osd_show_layout,
        );
    }
    true
}

#[tokio::main]
async fn main() -> Result<()> {
    #[cfg(windows)]
    unsafe {
        let _ = windows::Win32::System::Console::SetConsoleOutputCP(65001);
        let _ = windows::Win32::System::Console::SetConsoleCP(65001);
        let _ = windows::Win32::UI::HiDpi::SetProcessDpiAwarenessContext(
            windows::Win32::UI::HiDpi::DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
        );
    }

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(tracing::Level::INFO.into()),
        )
        .init();

    // A second service instance used to create a second tray icon even though
    // its IPC pipe could not start. The named mutex closes that race before any
    // worker, tray window or renderer is created.
    let _instance_mutex = unsafe {
        use windows::core::w;
        use windows::Win32::Foundation::{GetLastError, ERROR_ALREADY_EXISTS};
        use windows::Win32::System::Threading::CreateMutexW;

        let handle = CreateMutexW(None, false, w!("Local\\EvertyDisplay.Service.Singleton"))?;
        if GetLastError() == ERROR_ALREADY_EXISTS {
            info!("EvertyDisplay service is already running; exiting duplicate instance");
            return Ok(());
        }
        handle
    };

    info!("=== Multitor Virtual Screens Service Starting ===");

    // 1. Enumerate connected physical & virtual displays
    let mut detected_displays = multitor_driver_manager::enumerate_displays()?;
    if detected_displays.is_empty() {
        anyhow::bail!("Windows did not report any active displays");
    }
    info!(
        "Detected {} physical/virtual display(s)",
        detected_displays.len()
    );

    // 2. Load or create topology configuration
    let config_mgr = ConfigManager::new();
    if let Some(saved) = config_mgr.load_saved() {
        let configured_virtual = saved
            .monitors
            .iter()
            .filter(|monitor| monitor.is_virtual && monitor.is_enabled)
            .count() as u32;
        let expected_virtual = configured_virtual.min(PRODUCT_CAPABILITIES.max_virtual_displays);
        let detected_virtual = detected_displays
            .iter()
            .filter(|display| display.is_virtual)
            .count() as u32;

        if configured_virtual > expected_virtual {
            warn!(
                "{} edition limits startup restoration from {} to {} virtual display(s)",
                PRODUCT_CAPABILITIES.edition.as_str(),
                configured_virtual,
                expected_virtual
            );
        }

        if expected_virtual > detected_virtual
            && multitor_driver_manager::is_driver_service_running()
        {
            info!(
                "Waiting for {} configured virtual display(s) before topology reconciliation",
                expected_virtual
            );
            if let Err(error) =
                multitor_driver_manager::set_driver_monitor_count(expected_virtual).await
            {
                warn!("Could not restore configured virtual displays: {error:?}");
            } else {
                for _ in 0..30 {
                    tokio::time::sleep(Duration::from_millis(200)).await;
                    if let Ok(displays) = multitor_driver_manager::enumerate_displays() {
                        let ready_count =
                            displays.iter().filter(|display| display.is_virtual).count() as u32;
                        detected_displays = displays;
                        if ready_count >= expected_virtual {
                            break;
                        }
                    }
                }
            }
        } else if detected_virtual > expected_virtual
            && multitor_driver_manager::is_driver_pipe_ready()
        {
            // A previous service/UI crash may have happened while an added
            // monitor was awaiting confirmation. The saved topology is the
            // committed state, so discard only surplus MttVDD outputs.
            warn!(
                "Removing {} uncommitted virtual display(s) left by an interrupted operation",
                detected_virtual - expected_virtual
            );
            if multitor_driver_manager::restore_driver_monitor_count(expected_virtual)
                .await
                .is_ok()
            {
                for _ in 0..30 {
                    tokio::time::sleep(Duration::from_millis(200)).await;
                    if let Ok(displays) = multitor_driver_manager::enumerate_displays() {
                        let ready_count =
                            displays.iter().filter(|display| display.is_virtual).count() as u32;
                        detected_displays = displays;
                        if ready_count <= expected_virtual {
                            break;
                        }
                    }
                }
            }
        }
    }
    let mut initial_config = config_mgr.load_or_create(&detected_displays);
    if config::enforce_virtual_display_limit(
        &mut initial_config,
        PRODUCT_CAPABILITIES.max_virtual_displays,
    ) {
        warn!(
            "Disabled surplus virtual displays in the saved topology for {} edition",
            PRODUCT_CAPABILITIES.edition.as_str()
        );
        let _ = config_mgr.save(&initial_config);
    }

    if initial_config.match_virtual_mode_to_primary
        && synchronize_virtual_modes_to_primary(&detected_displays)
    {
        if let Ok(displays) = multitor_driver_manager::enumerate_displays() {
            detected_displays = displays;
            config::reconcile_detected(&mut initial_config, &detected_displays);
            let _ = config_mgr.save(&initial_config);
        }
    }

    // Make the saved canvas layout the real Windows desktop layout. Once both
    // agree, cursor and window dragging are handled natively by DWM.
    match apply_native_layout(&initial_config, &detected_displays) {
        Err(error) => warn!("Could not restore native Windows display layout: {}", error),
        Ok(true) => {
            if let Ok(displays) = multitor_driver_manager::enumerate_displays() {
                detected_displays = displays;
                config::reconcile_detected(&mut initial_config, &detected_displays);
                let _ = config_mgr.save(&initial_config);
            }
        }
        Ok(false) => {}
    }

    let mut topology = topology::TopologyManager::new(initial_config);

    let mut cursor_tracker = CursorTracker::new();
    let osd = OsdNotifier::new();
    let tray_mgr = TrayManager::new();

    // 3. Register global hotkeys
    let mut hotkey_mgr = HotkeyManager::new();
    if let Err(e) = hotkey_mgr.register_all() {
        warn!("Failed to register some hotkeys: {:?}", e);
    }

    // 4. Start IPC server in background
    let (cmd_tx, mut cmd_rx) = mpsc::channel::<ServiceCommand>(32);
    let ipc_server = IpcServer::new(cmd_tx);
    tokio::spawn(async move {
        if let Err(e) = ipc_server.run().await {
            error!("IPC server error: {:?}", e);
        }
    });

    let mut primary_display = detected_displays
        .iter()
        .find(|d| d.is_primary)
        .cloned()
        .unwrap_or_else(|| detected_displays[0].clone());

    let mut renderer: Option<ViewportRenderer> = None;
    let mut current_source_id = None;
    let mut last_save_time = Instant::now();
    let mut last_display_check = Instant::now();
    let mut last_game_check = Instant::now();
    let mut auto_game_paused = false;
    let mut pending_monitor_confirmation: Option<PendingMonitorConfirmation> = None;
    let mut pip_enabled = topology.config().pip_enabled;
    let mut last_pip_scale: Option<u32> = None;
    let mut pip_source_id = topology
        .config()
        .monitors
        .iter()
        .find(|monitor| {
            monitor.is_virtual
                && monitor
                    .device_name
                    .eq_ignore_ascii_case(&topology.config().pip_source_device_name)
        })
        .map(|monitor| monitor.id);
    let mut last_pip_geometry_check = Instant::now();
    let mut renderer_error_streak = 0u8;
    let foreground_watcher = ForegroundWatcher::new();
    let mut event_foreground_window =
        WindowManager::root_window_token(WindowManager::foreground_window_token());
    let mut minimizing_windows: HashMap<isize, Instant> = HashMap::new();
    let mut pending_foreground_activation: Option<PendingForegroundActivation> = None;

    tray::set_tray_pip_state(pip_enabled);
    tray::set_tray_paused_state(topology.config().pause_switching);
    osd::set_osd_enabled(topology.config().osd_enabled);
    osd::set_osd_duration(topology.config().osd_duration_ms);

    info!("Service running in background. Named Pipe ready.");

    loop {
        // Handle Tray Events
        while let Some(evt) = tray_mgr.poll_event() {
            match evt {
                TrayEvent::OpenUi => {
                    info!("Tray: Open UI requested");
                    tray::launch_ui_app();
                }
                TrayEvent::TogglePause => {
                    let paused = !topology.config().pause_switching;
                    topology.config_mut().pause_switching = paused;
                    tray::set_tray_paused_state(paused || auto_game_paused);
                    if paused {
                        osd.show(
                            "⏸ Пауза переходов: ВКЛ",
                            "Переключение мыши временно отключено",
                            OsdKind::Info,
                        );
                    } else {
                        osd.show(
                            "▶ Переходы мыши: АКТИВНЫ",
                            "Переключение мыши возобновлено",
                            OsdKind::Info,
                        );
                    }
                }
                TrayEvent::ToggleAutostart => {
                    let is_auto = autostart::is_autostart_enabled();
                    let new_auto = !is_auto;
                    let _ = autostart::set_autostart(new_auto);
                    if new_auto {
                        osd.show(
                            "🚀 Автозагрузка: ВКЛ",
                            "Multitor будет запускаться вместе с Windows",
                            OsdKind::Info,
                        );
                    } else {
                        osd.show(
                            "🚀 Автозагрузка: ВЫКЛ",
                            "Multitor удален из автозагрузки",
                            OsdKind::Info,
                        );
                    }
                }
                TrayEvent::TogglePip => {
                    if pip_enabled {
                        remember_current_pip_window(
                            renderer.as_ref(),
                            current_source_id.or(pip_source_id),
                            &mut topology,
                        );
                    }
                    pip_enabled = !pip_enabled;
                    topology.config_mut().pip_enabled = pip_enabled;
                    let _ = config_mgr.save(topology.config());
                    tray::set_tray_pip_state(pip_enabled);
                    last_pip_scale = None;
                    if !pip_enabled {
                        if let Some(ref mut rend) = renderer {
                            rend.hide();
                        }
                    }
                    if pip_enabled {
                        osd.show(
                            "📺 Режим PiP: ВКЛ",
                            "Превью виртуального экрана (Win+Alt+V)",
                            OsdKind::Info,
                        );
                    } else {
                        osd.show("📺 Режим PiP: ВЫКЛ", "Превью скрыто", OsdKind::Info);
                    }
                }
                TrayEvent::Exit => {
                    info!("Tray: Exit requested. Closing UI and EvertyDisplay service.");
                    if let (Some(rend), Some(source_id)) = (renderer.as_ref(), current_source_id) {
                        if rend.is_pip() {
                            if let (Some(bounds), Some(source_device)) = (
                                rend.window().bounds(),
                                topology
                                    .get_monitor(source_id)
                                    .map(|monitor| monitor.device_name.clone()),
                            ) {
                                remember_pip_bounds(topology.config_mut(), &source_device, bounds);
                                let _ = config_mgr.save(topology.config());
                            }
                        }
                    }
                    // Release desktop duplication, cursor clipping and the viewport before
                    // shutting down the process. The display adapter itself must stay running:
                    // Exit closes EvertyDisplay, it does not remove virtual monitors.
                    CursorTracker::clip_cursor_to(None);
                    if let Some(mut rend) = renderer.take() {
                        rend.hide();
                        drop(rend);
                    }
                    let _ = config_mgr.save(topology.config());
                    tray::close_ui_app();
                    return Ok(());
                }
            }
        }

        // A. Handle incoming IPC commands
        while let Ok(cmd) = cmd_rx.try_recv() {
            match cmd {
                ServiceCommand::GetTopology(tx) => {
                    let _ = tx.send(topology.config().clone());
                }
                ServiceCommand::SetActiveMonitor { id, reply } => {
                    let result =
                        switch_and_teleport_monitor(&mut topology, id, SwitchReason::Ui, &osd);
                    if result.is_ok() {
                        let _ = config_mgr.save(topology.config());
                    }
                    let _ = reply.send(result);
                }
                ServiceCommand::UpdateTopology {
                    topology: new_top,
                    reply,
                } => {
                    let result = (|| -> std::result::Result<(), String> {
                        let requested_virtual_count = new_top
                            .monitors
                            .iter()
                            .filter(|monitor| monitor.is_virtual && monitor.is_enabled)
                            .count() as u32;
                        everty_product_policy::validate_virtual_display_count(
                            requested_virtual_count,
                        )?;
                        if topology.config().pip_enabled && !new_top.pip_enabled {
                            remember_current_pip_window(
                                renderer.as_ref(),
                                current_source_id.or(pip_source_id),
                                &mut topology,
                            );
                        }
                        let autostart_changed =
                            topology.config().autostart_enabled != new_top.autostart_enabled;
                        let match_mode_just_enabled =
                            !topology.config().match_virtual_mode_to_primary
                                && new_top.match_virtual_mode_to_primary;
                        let mut validated_top = new_top;
                        // PiP placement is owned by the service because the window can be
                        // dragged while Settings holds an older topology snapshot.
                        validated_top.pip_placements = topology.config().pip_placements.clone();
                        validated_top.pip_source_device_name =
                            topology.config().pip_source_device_name.clone();
                        let mode_refreshed = if match_mode_just_enabled
                            && synchronize_virtual_modes_to_primary(&detected_displays)
                        {
                            multitor_driver_manager::enumerate_displays()
                                .map_err(|error| error.to_string())?
                        } else {
                            detected_displays.clone()
                        };
                        config::reconcile_detected(&mut validated_top, &mode_refreshed);

                        let layout_changed = apply_native_layout(&validated_top, &mode_refreshed)?;
                        let refreshed = if layout_changed {
                            multitor_driver_manager::enumerate_displays()
                                .map_err(|error| error.to_string())?
                        } else {
                            mode_refreshed
                        };
                        config::reconcile_detected(&mut validated_top, &refreshed);

                        pip_enabled = validated_top.pip_enabled;
                        tray::set_tray_pip_state(pip_enabled);
                        tray::set_tray_paused_state(validated_top.pause_switching);
                        osd::set_osd_enabled(validated_top.osd_enabled);
                        osd::set_osd_duration(validated_top.osd_duration_ms);
                        if autostart_changed {
                            autostart::set_autostart(validated_top.autostart_enabled)
                                .map_err(|error| error.to_string())?;
                        }

                        detected_displays = refreshed;
                        if let Some(primary) = detected_displays.iter().find(|d| d.is_primary) {
                            primary_display = primary.clone();
                        }
                        if layout_changed {
                            renderer = None;
                            current_source_id = None;
                        }
                        *topology.config_mut() = validated_top;
                        config_mgr
                            .save(topology.config())
                            .map_err(|error| error.to_string())?;
                        Ok(())
                    })();
                    let _ = reply.send(result);
                }
                ServiceCommand::SetPauseSwitching(pause) => {
                    topology.config_mut().pause_switching = pause;
                    tray::set_tray_paused_state(pause || auto_game_paused);
                    info!("Pause switching set to: {}", pause);
                }
                ServiceCommand::SetViewportEnabled(enabled) => {
                    if !enabled {
                        remember_current_pip_window(
                            renderer.as_ref(),
                            current_source_id.or(pip_source_id),
                            &mut topology,
                        );
                    }
                    topology.config_mut().viewport_enabled = enabled;
                    let _ = config_mgr.save(topology.config());
                    info!("Viewport mode set to: {}", enabled);
                    if !enabled {
                        if let Some(ref mut rend) = renderer {
                            rend.hide();
                        }
                    }
                }
                ServiceCommand::AddMonitor {
                    name,
                    width,
                    height,
                    refresh_rate,
                    reply,
                } => {
                    if pending_monitor_confirmation.is_some() {
                        let _ =
                            reply
                                .send(Err("Confirm or revert the previously added monitor first"
                                    .to_string()));
                        continue;
                    }
                    let (width, height, refresh_rate) =
                        if topology.config().match_virtual_mode_to_primary {
                            detected_displays
                                .iter()
                                .find(|display| display.is_primary && !display.is_virtual)
                                .or_else(|| {
                                    detected_displays.iter().find(|display| !display.is_virtual)
                                })
                                .map(|display| {
                                    (
                                        display.bounds.width,
                                        display.bounds.height,
                                        DEFAULT_VIRTUAL_REFRESH_RATE,
                                    )
                                })
                                .unwrap_or((width, height, refresh_rate))
                        } else {
                            (width, height, refresh_rate)
                        };
                    if !(640..=16384).contains(&width)
                        || !(480..=8640).contains(&height)
                        || !(24..=500).contains(&refresh_rate)
                    {
                        let _ = reply.send(Err(format!(
                            "Unsupported monitor mode: {width}x{height} @ {refresh_rate} Hz"
                        )));
                        continue;
                    }
                    let previous_virtual_count =
                        detected_displays.iter().filter(|d| d.is_virtual).count() as u32;
                    if previous_virtual_count >= PRODUCT_CAPABILITIES.max_virtual_displays {
                        let _ = reply.send(Err(format!(
                            "{} edition supports at most {} active virtual display(s)",
                            PRODUCT_CAPABILITIES.edition.as_str(),
                            PRODUCT_CAPABILITIES.max_virtual_displays
                        )));
                        continue;
                    }
                    let desired_count = previous_virtual_count.saturating_add(1);
                    let mut desired_modes =
                        virtual_modes_in_driver_order(topology.config(), &detected_displays);
                    let previous_modes = desired_modes.clone();
                    desired_modes.push(VirtualModePreference {
                        width,
                        height,
                        refresh_rate,
                    });

                    let result = async {
                        multitor_driver_manager::set_driver_monitor_count(desired_count)
                            .await
                            .map_err(|e| format!("Driver rejected monitor creation: {e}"))?;

                        let mut discovered = None;
                        for _attempt in 1..=60 {
                            tokio::time::sleep(Duration::from_millis(250)).await;
                            if let Ok(disps) = multitor_driver_manager::enumerate_displays() {
                                // MttVDD may renumber every DISPLAY device during its
                                // reinitialization, so a device-name set difference cannot
                                // identify the new connector. Driver connectors are enumerated
                                // in their stable slot order and SETDISPLAYCOUNT appends one.
                                let virtual_count =
                                    disps.iter().filter(|display| display.is_virtual).count() as u32;
                                if virtual_count >= desired_count {
                                    let new_display = disps
                                        .iter()
                                        .rfind(|display| display.is_virtual)
                                        .cloned()
                                        .ok_or_else(|| {
                                            "The driver reported the new count without a virtual output"
                                                .to_string()
                                        })?;
                                    discovered = Some((new_display, disps));
                                    break;
                                }
                            }
                        }

                        let (new_display, mut disps) = discovered.ok_or_else(|| {
                            "Windows did not activate the new virtual monitor within 15 seconds"
                                .to_string()
                        })?;

                        restore_virtual_modes(&disps, &desired_modes);
                        tokio::time::sleep(Duration::from_millis(500)).await;
                        if let Ok(refreshed) = multitor_driver_manager::enumerate_displays() {
                            disps = refreshed;
                        }

                        let new_display = disps
                            .iter()
                            .rfind(|display| display.is_virtual)
                            .cloned()
                            .unwrap_or(new_display);

                        detected_displays = disps;
                        config::reconcile_detected(topology.config_mut(), &detected_displays);
                        let new_id = if let Some(monitor) =
                            topology.config_mut().monitors.iter_mut().find(|m| {
                                m.device_name.eq_ignore_ascii_case(&new_display.device_name)
                            }) {
                            monitor.name = if name.trim().is_empty() {
                                format!("Virtual {}", monitor.id)
                            } else {
                                name.trim().to_string()
                            };
                            monitor.id
                        } else {
                            return Err(
                                "The new display disappeared during configuration".to_string()
                            );
                        };
                        Ok(new_id)
                    }
                    .await;

                    if result.is_err() {
                        let _ = multitor_driver_manager::restore_driver_monitor_count(
                            previous_virtual_count,
                        )
                        .await;
                    } else if let Ok(new_id) = result.as_ref() {
                        pending_monitor_confirmation = Some(PendingMonitorConfirmation {
                            id: *new_id,
                            previous_virtual_count,
                            previous_modes,
                            // The UI counts down from 15 seconds. The service owns
                            // the authoritative watchdog and leaves a small IPC grace period.
                            expires_at: Instant::now() + Duration::from_secs(20),
                        });
                    }
                    let _ = reply.send(result);
                }
                ServiceCommand::ConfirmMonitor { id, reply } => {
                    let result = match pending_monitor_confirmation.as_ref() {
                        Some(pending) if pending.id == id => {
                            pending_monitor_confirmation = None;
                            config_mgr
                                .save(topology.config())
                                .map_err(|error| error.to_string())
                        }
                        Some(_) => Err("A different monitor is awaiting confirmation".to_string()),
                        None if topology.get_monitor(id).is_some() => Ok(()),
                        None => Err(format!("Monitor {id} is not connected")),
                    };
                    let _ = reply.send(result);
                }
                ServiceCommand::RemoveMonitor { id, reply } => {
                    let target = topology.get_monitor(id).cloned();
                    let result = async {
                        let target = target
                            .ok_or_else(|| format!("Экран {id} не подключён или уже удалён"))?;
                        if !target.is_virtual {
                            return Err("Физические мониторы нельзя удалить через EvertyDisplay".to_string());
                        }

                        // SETDISPLAYCOUNT can only remove the driver's last virtual output.
                        // Refuse to pretend that an arbitrary selected device was removed.
                        let removable_display =
                            detected_displays.iter().rfind(|display| display.is_virtual);
                        if !removable_display
                            .map(|display| display.device_name.eq_ignore_ascii_case(&target.device_name))
                            .unwrap_or(false)
                        {
                            let removable_id = multitor_ipc::removable_virtual_monitor_id(
                                topology.config(),
                                &detected_displays,
                            );
                            return Err(format!(
                                "Драйвер удаляет виртуальные экраны в обратном порядке добавления. Сначала удалите экран {}",
                                removable_id.unwrap_or(id)
                            ));
                        }

                        let current_count = detected_displays.iter().filter(|d| d.is_virtual).count() as u32;
                        if current_count <= 1 {
                            return Err(
                                "Последний виртуальный экран нельзя удалить отдельно от активного адаптера. Для полного удаления используйте «Удалить видеодрайвер» в настройках."
                                    .to_string(),
                            );
                        }
                        let remaining_count = current_count.saturating_sub(1);
                        let remaining_modes = virtual_modes_in_driver_order(
                            topology.config(),
                            &detected_displays,
                        );
                        multitor_driver_manager::restore_driver_monitor_count(remaining_count)
                            .await
                            .map_err(|e| format!("драйвер отклонил удаление: {e}"))?;

                        for _attempt in 1..=60 {
                            tokio::time::sleep(Duration::from_millis(250)).await;
                            if let Ok(disps) = multitor_driver_manager::enumerate_displays() {
                                if disps.iter().filter(|d| d.is_virtual).count() as u32 <= remaining_count {
                                    restore_virtual_modes(
                                        &disps,
                                        &remaining_modes,
                                    );
                                    tokio::time::sleep(Duration::from_millis(500)).await;
                                    detected_displays = multitor_driver_manager::enumerate_displays()
                                        .unwrap_or(disps);
                                    topology.config_mut().monitors.retain(|monitor| monitor.id != id);
                                    if pending_monitor_confirmation
                                        .as_ref()
                                        .map(|pending| pending.id == id)
                                        .unwrap_or(false)
                                    {
                                        pending_monitor_confirmation = None;
                                    }
                                    config::reconcile_detected(topology.config_mut(), &detected_displays);
                                    config_mgr.save(topology.config()).map_err(|e| e.to_string())?;
                                    return Ok(());
                                }
                            }
                        }
                        let rollback = multitor_driver_manager::restore_driver_monitor_count(current_count).await;
                        if let Err(error) = rollback {
                            Err(format!(
                                "Windows не убрала виртуальный экран за 15 секунд, восстановить прежнее количество тоже не удалось: {error}"
                            ))
                        } else {
                            Err("Windows не убрала виртуальный экран за 15 секунд; прежнее количество экранов восстановлено".to_string())
                        }
                    }.await;
                    let _ = reply.send(result);
                }
                ServiceCommand::MoveMonitorLeft(id) => {
                    if topology.move_monitor_left(id) {
                        match apply_current_topology_layout(
                            &mut topology,
                            &mut detected_displays,
                            &mut primary_display,
                        ) {
                            Ok(true) => {
                                renderer = None;
                                current_source_id = None;
                            }
                            Ok(false) => {}
                            Err(error) => warn!("Could not apply left-move layout: {error}"),
                        }
                    }
                    let _ = config_mgr.save(topology.config());
                }
                ServiceCommand::MoveMonitorRight(id) => {
                    if topology.move_monitor_right(id) {
                        match apply_current_topology_layout(
                            &mut topology,
                            &mut detected_displays,
                            &mut primary_display,
                        ) {
                            Ok(true) => {
                                renderer = None;
                                current_source_id = None;
                            }
                            Ok(false) => {}
                            Err(error) => warn!("Could not apply right-move layout: {error}"),
                        }
                    }
                    let _ = config_mgr.save(topology.config());
                }
                ServiceCommand::SetNeighbors {
                    monitor_id,
                    left,
                    right,
                    top,
                    bottom,
                } => {
                    topology.set_neighbors(monitor_id, left, right, top, bottom);
                    let _ = config_mgr.save(topology.config());
                }
                ServiceCommand::AutoArrange(mode) => {
                    topology.auto_arrange(mode);
                    match apply_current_topology_layout(
                        &mut topology,
                        &mut detected_displays,
                        &mut primary_display,
                    ) {
                        Ok(true) => {
                            renderer = None;
                            current_source_id = None;
                        }
                        Ok(false) => {}
                        Err(error) => warn!("Could not apply automatic layout: {error}"),
                    }
                    let _ = config_mgr.save(topology.config());
                }
            }
        }

        // B. Handle Hotkeys
        if let Some(action) = hotkey_mgr.poll_hotkey() {
            match action {
                HotkeyAction::TogglePause => {
                    let paused = !topology.config().pause_switching;
                    topology.config_mut().pause_switching = paused;
                    tray::set_tray_paused_state(paused || auto_game_paused);
                    info!("HotKey: Toggle pause switching -> {}", paused);
                    if paused {
                        osd.show(
                            "⏸ Пауза переходов: ВКЛ",
                            "Переключение мыши временно отключено",
                            OsdKind::Info,
                        );
                    } else {
                        osd.show(
                            "▶ Переходы мыши: АКТИВНЫ",
                            "Переключение мыши возобновлено",
                            OsdKind::Info,
                        );
                    }
                }
                HotkeyAction::NavigateLeft => {
                    if let Some(cur_id) = topology.config().active_monitor_id {
                        if let Some(target) =
                            topology.find_neighbor_on_edge(cur_id, EdgeDirection::Left)
                        {
                            let _ = switch_and_teleport_monitor(
                                &mut topology,
                                target,
                                SwitchReason::Hotkey,
                                &osd,
                            );
                        }
                    }
                }
                HotkeyAction::NavigateRight => {
                    if let Some(cur_id) = topology.config().active_monitor_id {
                        if let Some(target) =
                            topology.find_neighbor_on_edge(cur_id, EdgeDirection::Right)
                        {
                            let _ = switch_and_teleport_monitor(
                                &mut topology,
                                target,
                                SwitchReason::Hotkey,
                                &osd,
                            );
                        }
                    }
                }
                HotkeyAction::NavigateUp => {
                    if let Some(cur_id) = topology.config().active_monitor_id {
                        if let Some(target) =
                            topology.find_neighbor_on_edge(cur_id, EdgeDirection::Top)
                        {
                            let _ = switch_and_teleport_monitor(
                                &mut topology,
                                target,
                                SwitchReason::Hotkey,
                                &osd,
                            );
                        }
                    }
                }
                HotkeyAction::NavigateDown => {
                    if let Some(cur_id) = topology.config().active_monitor_id {
                        if let Some(target) =
                            topology.find_neighbor_on_edge(cur_id, EdgeDirection::Bottom)
                        {
                            let _ = switch_and_teleport_monitor(
                                &mut topology,
                                target,
                                SwitchReason::Hotkey,
                                &osd,
                            );
                        }
                    }
                }
                HotkeyAction::SelectMonitor(id) => {
                    let _ =
                        switch_and_teleport_monitor(&mut topology, id, SwitchReason::Hotkey, &osd);
                }
                HotkeyAction::MoveWindowLeft => {
                    info!("HotKey: Move foreground window Left");
                    if let Some((title, target_id)) =
                        WindowManager::move_foreground_window(&mut topology, EdgeDirection::Left)
                    {
                        let target_name = topology
                            .get_monitor(target_id)
                            .map(|m| m.name.clone())
                            .unwrap_or_else(|| format!("Экран {}", target_id));
                        osd.show(
                            "⚡ Окно перенесено",
                            format!("\"{}\" ➔ {}", title, target_name),
                            OsdKind::WindowTeleport,
                        );
                    }
                }
                HotkeyAction::MoveWindowRight => {
                    info!("HotKey: Move foreground window Right");
                    if let Some((title, target_id)) =
                        WindowManager::move_foreground_window(&mut topology, EdgeDirection::Right)
                    {
                        let target_name = topology
                            .get_monitor(target_id)
                            .map(|m| m.name.clone())
                            .unwrap_or_else(|| format!("Экран {}", target_id));
                        osd.show(
                            "⚡ Окно перенесено",
                            format!("\"{}\" ➔ {}", title, target_name),
                            OsdKind::WindowTeleport,
                        );
                    }
                }
                HotkeyAction::TogglePip => {
                    if pip_enabled {
                        remember_current_pip_window(
                            renderer.as_ref(),
                            current_source_id.or(pip_source_id),
                            &mut topology,
                        );
                    }
                    pip_enabled = !pip_enabled;
                    topology.config_mut().pip_enabled = pip_enabled;
                    let _ = config_mgr.save(topology.config());
                    tray::set_tray_pip_state(pip_enabled);
                    last_pip_scale = None;
                    if !pip_enabled {
                        if let Some(ref mut rend) = renderer {
                            rend.hide();
                        }
                    }
                    if pip_enabled {
                        osd.show(
                            "📺 Режим PiP: ВКЛ",
                            "Превью виртуального экрана (Win+Alt+V)",
                            OsdKind::Info,
                        );
                    } else {
                        osd.show("📺 Режим PiP: ВЫКЛ", "Превью скрыто", OsdKind::Info);
                    }
                }
            }
        }

        // Consume precise WinEvent notifications instead of inferring intent by
        // polling HWND values. MINIMIZESTART explicitly cancels the foreground
        // event produced by Windows when it exposes an underlying window.
        if let Some(watcher) = foreground_watcher.as_ref() {
            while let Some(event) = watcher.poll_event() {
                match event {
                    ForegroundEvent::MinimizeStarted(window) => {
                        let window = WindowManager::root_window_token(window);
                        minimizing_windows.insert(window, Instant::now());
                        if pending_foreground_activation
                            .as_ref()
                            .map(|pending| pending.previous_window == window)
                            .unwrap_or(false)
                        {
                            pending_foreground_activation = None;
                        }
                    }
                    ForegroundEvent::MinimizeEnded(window) => {
                        let window = WindowManager::root_window_token(window);
                        minimizing_windows.remove(&window);
                    }
                    ForegroundEvent::Focused(window) => {
                        let target_window = WindowManager::root_window_token(window);
                        if topology
                            .active_monitor()
                            .map(|monitor| monitor.is_enabled && monitor.is_virtual)
                            .unwrap_or(false)
                            && topology.config().follow_physical_window_activation
                            && WindowManager::is_browser_window(target_window)
                        {
                            if let Some(target) =
                                WindowManager::window_location(target_window, &topology)
                                    .filter(|target| !target.is_virtual)
                            {
                                let same_transition_is_pending = pending_foreground_activation
                                    .as_ref()
                                    .map(|pending| {
                                        pending.target_window == target_window
                                            && pending.target_monitor_id == target.monitor_id
                                    })
                                    .unwrap_or(false);
                                if !same_transition_is_pending {
                                    pending_foreground_activation =
                                        Some(PendingForegroundActivation {
                                            previous_window: event_foreground_window,
                                            target_window,
                                            target_monitor_id: target.monitor_id,
                                            target_is_virtual: false,
                                            // Chromium can focus a newly selected tab without
                                            // making its existing top-level HWND foreground.
                                            require_foreground: false,
                                            ready_at: Instant::now() + Duration::from_millis(80),
                                        });
                                }
                            }
                        }
                    }
                    ForegroundEvent::Activated(window) => {
                        let window = WindowManager::root_window_token(window);
                        let previous_window = event_foreground_window;
                        event_foreground_window = window;
                        let action = topology.config().virtual_window_activation_action;
                        let caused_by_minimize = minimizing_windows.contains_key(&previous_window);
                        let active_is_virtual = topology
                            .active_monitor()
                            .filter(|monitor| monitor.is_enabled)
                            .map(|monitor| monitor.is_virtual);
                        let candidate = WindowManager::foreground_window_location(&topology)
                            .filter(|target| {
                                should_follow_activated_window(
                                    active_is_virtual,
                                    target.is_virtual,
                                    action,
                                    topology.config().follow_physical_window_activation,
                                    caused_by_minimize,
                                )
                            });
                        pending_foreground_activation = candidate.map(|target| {
                            PendingForegroundActivation {
                                previous_window,
                                target_window: window,
                                target_monitor_id: target.monitor_id,
                                target_is_virtual: target.is_virtual,
                                require_foreground: true,
                                // Covers rare machines where minimize and foreground
                                // callbacks arrive in the opposite order.
                                ready_at: Instant::now() + Duration::from_millis(80),
                            }
                        });
                    }
                }
            }
        }

        minimizing_windows.retain(|_, started| started.elapsed() < Duration::from_secs(2));

        if pending_foreground_activation
            .as_ref()
            .map(|pending| Instant::now() >= pending.ready_at)
            .unwrap_or(false)
        {
            let pending = pending_foreground_activation.take().unwrap();
            let target_is_still_foreground = !pending.require_foreground
                || WindowManager::root_window_token(WindowManager::foreground_window_token())
                    == pending.target_window;
            let target_is_stable = WindowManager::window_location(pending.target_window, &topology)
                .map(|target| {
                    target.monitor_id == pending.target_monitor_id
                        && target.is_virtual == pending.target_is_virtual
                })
                .unwrap_or(false);
            if target_is_still_foreground
                && target_is_stable
                && !minimizing_windows.contains_key(&pending.previous_window)
                && !WindowManager::foreground_window_was_dismissed(pending.previous_window)
            {
                if pending.target_is_virtual {
                    apply_virtual_window_activation(
                        topology.config().virtual_window_activation_action,
                        &mut topology,
                        &osd,
                    );
                } else if apply_physical_window_activation(
                    &mut topology,
                    pending.target_window,
                    pending.target_monitor_id,
                    !pending.require_foreground,
                    &osd,
                ) {
                    if let Some(ref mut rend) = renderer {
                        if !rend.is_pip() {
                            rend.hide();
                            current_source_id = None;
                            last_pip_scale = None;
                        }
                    }
                }
            } else {
                info!(
                    "Ignoring unstable foreground change caused by minimize, close, or another activation"
                );
            }
        }

        // C. Update cursor tracking and edge transitions (including Drag-to-Teleport)
        let previous_active_id = topology.config().active_monitor_id;
        let switched = cursor_tracker.update(&mut topology, auto_game_paused);
        if let Some(res) = switched {
            // Tear down the fullscreen mirror immediately when the real cursor returns
            // to a physical display. This must happen before focus handling so a
            // fullscreen video on the virtual output can never remain over MAIN.
            let target_is_physical = topology
                .get_monitor(res.target_id)
                .map(|monitor| !monitor.is_virtual)
                .unwrap_or(false);
            if target_is_physical {
                if let Some(ref mut rend) = renderer {
                    // A PiP window is a real movable preview and must survive cursor
                    // movement between physical monitors. Only dismiss the fullscreen
                    // virtual-display mirror when returning to a physical screen.
                    if !rend.is_pip() {
                        rend.hide();
                        current_source_id = None;
                        last_pip_scale = None;
                    }
                }
            }

            if let Some(ref win_title) = res.dragged_window {
                let target_name = topology
                    .get_monitor(res.target_id)
                    .map(|m| m.name.clone())
                    .unwrap_or_else(|| format!("Экран {}", res.target_id));
                osd.show(
                    "🪟 Окно перенесено",
                    format!("\"{}\" ➔ {}", win_title, target_name),
                    OsdKind::WindowTeleport,
                );
            } else if should_show_monitor_switch_osd(
                topology.config(),
                previous_active_id,
                res.target_id,
            ) {
                if let Some(mon) = topology.get_monitor(res.target_id) {
                    let title = format!("🖥 Экран #{}: {}", res.target_id, mon.name);
                    let subtitle = format!(
                        "{}x{} @ {} Гц",
                        mon.bounds.width, mon.bounds.height, mon.refresh_rate
                    );
                    osd.show_monitor_switch(
                        title,
                        subtitle,
                        &topology.config().monitors,
                        res.target_id,
                        topology.config().osd_show_layout,
                    );
                }
            }

            // When leaving a virtual display, never leave its off-screen window
            // as the Windows foreground window. Besides being confusing, that
            // makes a second click on the same taskbar button invisible to the
            // foreground-change detector. This focus handoff is required by the
            // activation action even when optional Smart Auto-Focus is disabled.
            let needs_activation_handoff = target_is_physical
                && topology.config().virtual_window_activation_action
                    != VirtualWindowActivationAction::None
                && WindowManager::foreground_virtual_window(&topology).is_some();
            if should_focus_after_monitor_switch(
                topology.config().smart_focus_enabled,
                !target_is_physical,
                needs_activation_handoff,
            ) {
                if let Some((cur_x, cur_y)) = CursorTracker::get_current_cursor_pos() {
                    let focused = WindowManager::focus_window_at_point(cur_x, cur_y);
                    if needs_activation_handoff && !focused {
                        let _ = WindowManager::focus_desktop();
                    }
                }
            }
        }

        // D. Manage Viewport Renderer on the physical screen
        if topology.config().viewport_enabled {
            let mut reset_unhealthy_renderer = false;
            if renderer.is_none() {
                match ViewportWindow::create_borderless(
                    primary_display.bounds.x,
                    primary_display.bounds.y,
                    primary_display.bounds.width,
                    primary_display.bounds.height,
                    "Multitor Viewport",
                ) {
                    Ok(win) => match ViewportRenderer::new(win) {
                        Ok(rend) => {
                            renderer = Some(rend);
                            renderer_error_streak = 0;
                            current_source_id = None;
                        }
                        Err(e) => warn!("Failed to init ViewportRenderer: {:?}", e),
                    },
                    Err(e) => warn!("Failed to create ViewportWindow: {:?}", e),
                }
            }

            if let Some(ref mut rend) = renderer {
                if let Some(active_virtual) = topology
                    .config()
                    .active_monitor_id
                    .and_then(|id| topology.get_monitor(id))
                    .filter(|monitor| monitor.is_virtual && monitor.is_enabled)
                {
                    pip_source_id = Some(active_virtual.id);
                    if !topology
                        .config()
                        .pip_source_device_name
                        .eq_ignore_ascii_case(&active_virtual.device_name)
                    {
                        topology.config_mut().pip_source_device_name =
                            active_virtual.device_name.clone();
                    }
                }

                let virtual_disp = topology
                    .config()
                    .monitors
                    .iter()
                    .filter(|monitor| monitor.is_virtual && monitor.is_enabled)
                    .find(|monitor| Some(monitor.id) == pip_source_id)
                    .or_else(|| {
                        topology
                            .config()
                            .monitors
                            .iter()
                            .filter(|monitor| monitor.is_virtual && monitor.is_enabled)
                            .find(|monitor| {
                                monitor
                                    .device_name
                                    .eq_ignore_ascii_case(&topology.config().pip_source_device_name)
                            })
                    })
                    .or_else(|| {
                        topology
                            .config()
                            .monitors
                            .iter()
                            .find(|monitor| monitor.is_virtual && monitor.is_enabled)
                    })
                    .and_then(|monitor| {
                        detected_displays
                            .iter()
                            .find(|display| {
                                display
                                    .device_name
                                    .eq_ignore_ascii_case(&monitor.device_name)
                            })
                            .cloned()
                            .map(|display| (monitor.id, display))
                    });

                if let Some(active_id) = topology.config().active_monitor_id {
                    let is_active_virtual = topology
                        .config()
                        .monitors
                        .iter()
                        .find(|m| m.id == active_id)
                        .map(|m| m.is_virtual)
                        .unwrap_or(false);

                    if is_active_virtual {
                        // User is on a VIRTUAL display: show fullscreen viewport on primary physical monitor
                        if rend.is_pip() || !rend.is_visible() {
                            if rend.is_pip() {
                                if let (Some(bounds), Some(source_device)) = (
                                    rend.window().bounds(),
                                    current_source_id.and_then(|source_id| {
                                        topology
                                            .get_monitor(source_id)
                                            .map(|monitor| monitor.device_name.clone())
                                    }),
                                ) {
                                    remember_pip_bounds(
                                        topology.config_mut(),
                                        &source_device,
                                        bounds,
                                    );
                                    let _ = config_mgr.save(topology.config());
                                }
                            }
                            rend.set_fullscreen(
                                primary_display.bounds.x,
                                primary_display.bounds.y,
                                primary_display.bounds.width,
                                primary_display.bounds.height,
                            );
                            last_pip_scale = None;
                        }
                        rend.show();

                        if current_source_id != Some(active_id) {
                            let active_device = topology
                                .get_monitor(active_id)
                                .map(|m| m.device_name.as_str());
                            if let Some(target_disp) = detected_displays.iter().find(|d| {
                                active_device
                                    .map(|device| d.device_name.eq_ignore_ascii_case(device))
                                    .unwrap_or(false)
                            }) {
                                if let Err(e) = rend.switch_source(&target_disp.device_name) {
                                    warn!(
                                        "Failed to switch viewport source to {}: {:?}",
                                        target_disp.device_name, e
                                    );
                                } else {
                                    current_source_id = Some(active_id);
                                    reset_unhealthy_renderer |= renderer_result_is_fatal(
                                        rend.render_frame(100),
                                        &mut renderer_error_streak,
                                    );
                                }
                            }
                        }

                        // Render frame from virtual screen
                        reset_unhealthy_renderer |= renderer_result_is_fatal(
                            rend.render_frame(2),
                            &mut renderer_error_streak,
                        );

                        // Update cursor overlay position on physical primary screen
                        if let Some((cur_x, cur_y)) = CursorTracker::get_current_cursor_pos() {
                            let active_device = topology
                                .get_monitor(active_id)
                                .map(|m| m.device_name.as_str());
                            if let Some(target_disp) = detected_displays.iter().find(|d| {
                                active_device
                                    .map(|device| d.device_name.eq_ignore_ascii_case(device))
                                    .unwrap_or(false)
                            }) {
                                let rel_x = cur_x - target_disp.bounds.x;
                                let rel_y = cur_y - target_disp.bounds.y;
                                if let Some((screen_x, screen_y)) =
                                    rend.map_source_cursor_to_window(rel_x, rel_y)
                                {
                                    rend.update_cursor(screen_x, screen_y);
                                }
                            }
                        }
                    } else if pip_enabled && virtual_disp.is_some() {
                        // User is on PHYSICAL display AND PiP is enabled:
                        // Dynamic floating preview in bottom-right corner scaled to user setting (or stretched by mouse)
                        let (target_virtual_id, target_vdisp) = virtual_disp.as_ref().unwrap();
                        let current_scale = topology.config().pip_scale_percent;
                        let active_physical = topology
                            .config()
                            .active_monitor_id
                            .and_then(|id| topology.get_monitor(id))
                            .filter(|monitor| !monitor.is_virtual)
                            .and_then(|monitor| {
                                detected_displays.iter().find(|display| {
                                    display
                                        .device_name
                                        .eq_ignore_ascii_case(&monitor.device_name)
                                })
                            })
                            .unwrap_or(&primary_display);
                        let default_bounds =
                            default_pip_bounds(active_physical, target_vdisp, current_scale);

                        if !rend.is_pip()
                            || !rend.is_visible()
                            || last_pip_scale != Some(current_scale)
                        {
                            let bounds = if last_pip_scale.is_none() {
                                saved_pip_bounds(
                                    topology.config(),
                                    &target_vdisp.device_name,
                                    &detected_displays,
                                )
                                .unwrap_or(default_bounds)
                            } else {
                                // Moving the scale slider is an explicit request to reset the
                                // current source to the newly selected relative size.
                                default_bounds
                            };
                            rend.set_pip(bounds.x, bounds.y, bounds.width, bounds.height);
                            last_pip_scale = Some(current_scale);
                        }
                        rend.show();

                        // Check for right-click context menu actions from the PiP window
                        if let Some(action) = rend.window().pop_pip_action() {
                            match action {
                                PipAction::HidePip => {
                                    remember_current_pip_window(
                                        Some(&*rend),
                                        current_source_id.or(pip_source_id),
                                        &mut topology,
                                    );
                                    pip_enabled = false;
                                    topology.config_mut().pip_enabled = false;
                                    let _ = config_mgr.save(topology.config());
                                    tray::set_tray_pip_state(false);
                                    last_pip_scale = None;
                                    osd.show(
                                        "📺 PiP скрыт",
                                        "Правая кнопка мыши → Показать PiP",
                                        OsdKind::Info,
                                    );
                                }
                                PipAction::GoToMonitor => {
                                    // Teleport cursor to the virtual monitor being previewed and enable viewport
                                    if let Some((vdisp_id, _)) = virtual_disp.as_ref() {
                                        topology.config_mut().viewport_enabled = true;
                                        let _ = config_mgr.save(topology.config());
                                        let _ = switch_and_teleport_monitor(
                                            &mut topology,
                                            *vdisp_id,
                                            SwitchReason::Hotkey,
                                            &osd,
                                        );
                                    }
                                }
                            }
                        }

                        if current_source_id != Some(*target_virtual_id) {
                            if let Err(e) = rend.switch_source(&target_vdisp.device_name) {
                                warn!(
                                    "Failed to switch PiP source to {}: {:?}",
                                    target_vdisp.device_name, e
                                );
                            } else {
                                current_source_id = Some(*target_virtual_id);
                                reset_unhealthy_renderer |= renderer_result_is_fatal(
                                    rend.render_frame(100),
                                    &mut renderer_error_streak,
                                );
                            }
                        }

                        reset_unhealthy_renderer |= renderer_result_is_fatal(
                            rend.render_frame(2),
                            &mut renderer_error_streak,
                        );

                        // Polling at a modest rate is enough to remember interactive drag/resize
                        // without querying and rewriting state on every rendered frame.
                        if last_pip_geometry_check.elapsed() >= Duration::from_millis(250) {
                            if let Some(bounds) = rend.window().bounds() {
                                remember_pip_bounds(
                                    topology.config_mut(),
                                    &target_vdisp.device_name,
                                    bounds,
                                );
                            }
                            last_pip_geometry_check = Instant::now();
                        }
                    } else {
                        // User is on PHYSICAL display with PiP disabled: hide viewport
                        rend.hide();
                        current_source_id = None;
                        last_pip_scale = None;
                    }
                }

                // Pump window messages
                if !rend.window().poll_events() {
                    renderer = None;
                    topology.config_mut().viewport_enabled = false;
                }
            }
            if reset_unhealthy_renderer {
                warn!("Recreating Viewport renderer after repeated failures");
                renderer = None;
                current_source_id = None;
                renderer_error_streak = 0;
            }
        }

        // Periodic check for display changes (resolution / refresh rate / connect / disconnect)
        if last_display_check.elapsed() > Duration::from_secs(2) {
            if let Ok(mut disps) = multitor_driver_manager::enumerate_displays() {
                if disps != detected_displays {
                    info!("Display configuration changed in Windows OS! Updating topologies and renderer.");
                    let primary_mode_changed = disps
                        .iter()
                        .find(|display| display.is_primary && !display.is_virtual)
                        .map(|primary| {
                            primary.bounds.width != primary_display.bounds.width
                                || primary.bounds.height != primary_display.bounds.height
                                || primary.refresh_rate != primary_display.refresh_rate
                        })
                        .unwrap_or(false);
                    if topology.config().match_virtual_mode_to_primary
                        && primary_mode_changed
                        && synchronize_virtual_modes_to_primary(&disps)
                    {
                        disps = multitor_driver_manager::enumerate_displays().unwrap_or(disps);
                    }
                    detected_displays = disps;

                    // Atomically add new displays, remove disconnected ones, and preserve
                    // per-monitor preferences by stable Windows device name.
                    config::reconcile_detected(topology.config_mut(), &detected_displays);

                    if let Some(p) = detected_displays.iter().find(|d| d.is_primary) {
                        if p.bounds != primary_display.bounds {
                            // Primary monitor resolution changed, recreate viewport window!
                            renderer = None;
                        }
                        primary_display = p.clone();
                    } else if let Some(first) = detected_displays.first() {
                        primary_display = first.clone();
                    }
                    if let Some(ref mut rend) = renderer {
                        let _ = rend.force_reinit_source();
                    }
                    last_pip_scale = None;
                    match apply_current_topology_layout(
                        &mut topology,
                        &mut detected_displays,
                        &mut primary_display,
                    ) {
                        Ok(true) => {
                            renderer = None;
                            current_source_id = None;
                        }
                        Ok(false) => {}
                        Err(error) => {
                            warn!("Could not restore reconnected display layout: {error}")
                        }
                    }
                    let _ = config_mgr.save(topology.config());
                }
            }
            last_display_check = Instant::now();
        }

        // Safety does not depend on the settings window staying alive. If the UI
        // closes or crashes during its confirmation dialog, the service restores
        // the previous virtual-display count by itself.
        let confirmation_expired = pending_monitor_confirmation
            .as_ref()
            .map(|pending| Instant::now() >= pending.expires_at)
            .unwrap_or(false);
        if confirmation_expired {
            if let Some(mut pending) = pending_monitor_confirmation.take() {
                match multitor_driver_manager::restore_driver_monitor_count(
                    pending.previous_virtual_count,
                )
                .await
                {
                    Ok(()) => {
                        topology
                            .config_mut()
                            .monitors
                            .retain(|monitor| monitor.id != pending.id);

                        // SETDISPLAYCOUNT returns before Windows finishes removing
                        // the output. Reconciling immediately can add the abandoned
                        // monitor straight back into the saved topology. Wait for
                        // the live count, then restore modes reset by adapter restart.
                        let mut settled_displays = None;
                        for _ in 0..60 {
                            tokio::time::sleep(Duration::from_millis(250)).await;
                            if let Ok(displays) = multitor_driver_manager::enumerate_displays() {
                                let virtual_count =
                                    displays.iter().filter(|display| display.is_virtual).count()
                                        as u32;
                                if virtual_count <= pending.previous_virtual_count {
                                    settled_displays = Some(displays);
                                    break;
                                }
                            }
                        }
                        if let Some(displays) = settled_displays {
                            restore_virtual_modes(&displays, &pending.previous_modes);
                            tokio::time::sleep(Duration::from_millis(500)).await;
                            detected_displays =
                                multitor_driver_manager::enumerate_displays().unwrap_or(displays);
                            config::reconcile_detected(topology.config_mut(), &detected_displays);
                        } else {
                            warn!(
                                "Windows did not settle after reverting unconfirmed monitor {}",
                                pending.id
                            );
                        }
                        let _ = config_mgr.save(topology.config());
                        warn!(
                            "Unconfirmed virtual monitor {} was automatically reverted",
                            pending.id
                        );
                    }
                    Err(error) => {
                        warn!(
                            "Could not revert unconfirmed monitor {}: {error}; retrying",
                            pending.id
                        );
                        pending.expires_at = Instant::now() + Duration::from_secs(5);
                        pending_monitor_confirmation = Some(pending);
                    }
                }
            }
        }

        // Periodic Auto-Gaming Guard check (detect fullscreen games and pause edge mouse switches)
        if topology.config().gaming_guard_enabled
            && last_game_check.elapsed() > Duration::from_millis(500)
        {
            let is_game = WindowManager::is_fullscreen_game_active();
            if is_game && !auto_game_paused {
                info!("Auto-Gaming Guard: Fullscreen game detected! Temporarily pausing edge switching.");
                auto_game_paused = true;
                tray::set_tray_paused_state(true);
                osd.show(
                    "🎮 Игровой режим",
                    "Полноэкранная игра — переходы на паузе",
                    OsdKind::GamingGuard,
                );
            } else if !is_game && auto_game_paused {
                info!(
                    "Auto-Gaming Guard: Fullscreen game exited/minimized. Resuming edge switching."
                );
                auto_game_paused = false;
                tray::set_tray_paused_state(topology.config().pause_switching);
                osd.show(
                    "🎮 Игровой режим выключен",
                    "Переходы мыши снова активны",
                    OsdKind::GamingGuard,
                );
            }
            last_game_check = Instant::now();
        }

        // Periodic config save
        if pending_monitor_confirmation.is_none()
            && last_save_time.elapsed() > Duration::from_secs(30)
        {
            let _ = config_mgr.save(topology.config());
            last_save_time = Instant::now();
        }

        tokio::time::sleep(Duration::from_millis(2)).await;
    }
}
