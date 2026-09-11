use serde::{Deserialize, Serialize};

pub const IPC_PIPE_NAME: &str = r"\\.\pipe\multitor-virtual-screens-ipc";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DisplayBounds {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

impl DisplayBounds {
    pub fn new(x: i32, y: i32, width: u32, height: u32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    pub fn contains_point(&self, px: i32, py: i32) -> bool {
        px >= self.x
            && px < self.x + self.width as i32
            && py >= self.y
            && py < self.y + self.height as i32
    }

    pub fn right(&self) -> i32 {
        self.x + self.width as i32
    }

    pub fn bottom(&self) -> i32 {
        self.y + self.height as i32
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DisplayInfo {
    pub id: u32,
    pub device_name: String,
    pub friendly_name: String,
    pub bounds: DisplayBounds,
    pub refresh_rate: u32,
    pub is_primary: bool,
    pub is_virtual: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Neighbors {
    pub left: Option<u32>,
    pub right: Option<u32>,
    pub top: Option<u32>,
    pub bottom: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MonitorConfig {
    pub id: u32,
    /// Stable Windows display device path (for example `\\\\.\\DISPLAY2`).
    /// Older configuration files do not contain it and are migrated on load.
    #[serde(default)]
    pub device_name: String,
    pub name: String,
    pub bounds: DisplayBounds,
    pub refresh_rate: u32,
    pub neighbors: Neighbors,
    pub edge_activation_delay_ms: u64,
    pub is_enabled: bool,
    pub is_virtual: bool,
    #[serde(default)]
    pub layout_x: Option<i32>,
    #[serde(default)]
    pub layout_y: Option<i32>,
}

impl MonitorConfig {
    /// Returns spatial layout bounds used for 2D canvas positioning and neighbor calculation.
    /// Falls back to physical OS bounds if custom layout positions are not set.
    pub fn layout_bounds(&self) -> DisplayBounds {
        DisplayBounds {
            x: self.layout_x.unwrap_or(self.bounds.x),
            y: self.layout_y.unwrap_or(self.bounds.y),
            width: self.bounds.width,
            height: self.bounds.height,
        }
    }
}

fn default_true() -> bool {
    true
}

fn default_osd_duration() -> u32 {
    1500
}

fn default_pip_scale() -> u32 {
    15
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PipPlacementConfig {
    pub source_device_name: String,
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

/// Defines what EvertyDisplay should do when Windows activates a window whose
/// center is on a virtual display (for example from the taskbar or Alt+Tab).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VirtualWindowActivationAction {
    /// Preserve native Windows behavior.
    #[default]
    None,
    /// Keep the window in place and enter the virtual display that contains it.
    SwitchToVirtual,
    /// Keep the user on the current physical display and bring the window there.
    MoveToPhysical,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TopologyConfig {
    pub monitors: Vec<MonitorConfig>,
    pub active_monitor_id: Option<u32>,
    pub wrap_around: bool,
    pub pause_switching: bool,
    pub viewport_enabled: bool,
    pub edge_delay_ms: u64,
    pub cursor_velocity_threshold: f32,

    #[serde(default = "default_true")]
    pub osd_enabled: bool,
    #[serde(default = "default_osd_duration")]
    pub osd_duration_ms: u32,
    #[serde(default)]
    pub osd_show_layout: bool,
    /// Suppress only physical-to-physical display switch notifications while
    /// preserving OSD for every transition involving a virtual display.
    #[serde(default)]
    pub osd_hide_physical_to_physical: bool,
    #[serde(default = "default_true")]
    pub gaming_guard_enabled: bool,
    #[serde(default = "default_true")]
    pub smart_focus_enabled: bool,
    #[serde(default = "default_true")]
    pub drag_teleport_enabled: bool,
    /// Keep virtual outputs at the primary physical display's native pixel
    /// dimensions without copying its refresh rate.
    #[serde(default = "default_true")]
    pub match_virtual_mode_to_primary: bool,
    /// Follow a newly activated window from a virtual workspace back to the
    /// physical display where Windows opened it (for example, a browser link).
    #[serde(default = "default_true")]
    pub follow_physical_window_activation: bool,
    #[serde(default)]
    pub virtual_window_activation_action: VirtualWindowActivationAction,
    #[serde(default)]
    pub pip_enabled: bool,
    #[serde(default = "default_pip_scale")]
    pub pip_scale_percent: u32,
    /// Last virtual display selected for Live PiP. Device names are more stable
    /// than logical display numbers, which Windows may compact after reconnects.
    #[serde(default)]
    pub pip_source_device_name: String,
    /// User-controlled PiP geometry is remembered independently for every
    /// virtual source so switching through fullscreen never destroys it.
    #[serde(default)]
    pub pip_placements: Vec<PipPlacementConfig>,
    #[serde(default)]
    pub autostart_enabled: bool,
}

/// Returns the logical id of the virtual connector that the driver can remove
/// next. MttVDD removes connectors in reverse driver order; Windows logical ids
/// can be renumbered and therefore must not be used to infer that order.
pub fn removable_virtual_monitor_id(
    topology: &TopologyConfig,
    displays: &[DisplayInfo],
) -> Option<u32> {
    displays
        .iter()
        .rfind(|display| display.is_virtual)
        .and_then(|display| {
            topology
                .monitors
                .iter()
                .find(|monitor| {
                    monitor
                        .device_name
                        .eq_ignore_ascii_case(&display.device_name)
                })
                .map(|monitor| monitor.id)
        })
}

/// Automatically recomputes neighbor links for all monitors based on their 2D bounding boxes.
pub fn recompute_neighbors(monitors: &mut [MonitorConfig], wrap_around: bool) {
    let count = monitors.len();
    let enabled_count = monitors.iter().filter(|monitor| monitor.is_enabled).count();
    if enabled_count <= 1 {
        for m in monitors.iter_mut() {
            m.neighbors = Neighbors::default();
        }
        return;
    }

    // Clear existing
    for m in monitors.iter_mut() {
        m.neighbors = Neighbors::default();
    }

    for i in 0..count {
        let mut best_left: Option<(u32, i32)> = None;
        let mut best_right: Option<(u32, i32)> = None;
        let mut best_top: Option<(u32, i32)> = None;
        let mut best_bottom: Option<(u32, i32)> = None;

        let mi = &monitors[i];
        if !mi.is_enabled {
            continue;
        }
        let mi_bounds = mi.layout_bounds();
        let mi_x = mi_bounds.x;
        let mi_y = mi_bounds.y;
        let mi_r = mi_bounds.right();
        let mi_b = mi_bounds.bottom();

        for (j, mj) in monitors.iter().enumerate() {
            if i == j || !mj.is_enabled {
                continue;
            }
            let mj_id = mj.id;
            let mj_bounds = mj.layout_bounds();
            let mj_x = mj_bounds.x;
            let mj_y = mj_bounds.y;
            let mj_r = mj_bounds.right();
            let mj_b = mj_bounds.bottom();

            // Vertical overlap check
            let v_overlap = mi_y < mj_b && mi_b > mj_y;
            // Horizontal overlap check
            let h_overlap = mi_x < mj_r && mi_r > mj_x;

            // Left neighbor candidate (mj is to the left of mi)
            if v_overlap && mj_r <= mi_x {
                let dist = mi_x - mj_r;
                if best_left.map(|(_, d)| dist < d).unwrap_or(true) {
                    best_left = Some((mj_id, dist));
                }
            }

            // Right neighbor candidate (mj is to the right of mi)
            if v_overlap && mj_x >= mi_r {
                let dist = mj_x - mi_r;
                if best_right.map(|(_, d)| dist < d).unwrap_or(true) {
                    best_right = Some((mj_id, dist));
                }
            }

            // Top neighbor candidate (mj is above mi)
            if h_overlap && mj_b <= mi_y {
                let dist = mi_y - mj_b;
                if best_top.map(|(_, d)| dist < d).unwrap_or(true) {
                    best_top = Some((mj_id, dist));
                }
            }

            // Bottom neighbor candidate (mj is below mi)
            if h_overlap && mj_y >= mi_b {
                let dist = mj_y - mi_b;
                if best_bottom.map(|(_, d)| dist < d).unwrap_or(true) {
                    best_bottom = Some((mj_id, dist));
                }
            }
        }

        monitors[i].neighbors.left = best_left.map(|(id, _)| id);
        monitors[i].neighbors.right = best_right.map(|(id, _)| id);
        monitors[i].neighbors.top = best_top.map(|(id, _)| id);
        monitors[i].neighbors.bottom = best_bottom.map(|(id, _)| id);
    }

    // Optional wrap-around ONLY if explicitly enabled
    if wrap_around && enabled_count > 1 {
        let leftmost_id = monitors
            .iter()
            .filter(|monitor| monitor.is_enabled)
            .min_by_key(|m| m.layout_bounds().x)
            .map(|m| m.id);
        let rightmost_id = monitors
            .iter()
            .filter(|monitor| monitor.is_enabled)
            .max_by_key(|m| m.layout_bounds().right())
            .map(|m| m.id);

        if let (Some(left_id), Some(right_id)) = (leftmost_id, rightmost_id) {
            if left_id != right_id {
                for m in monitors.iter_mut() {
                    if m.id == left_id && m.neighbors.left.is_none() {
                        m.neighbors.left = Some(right_id);
                    }
                    if m.id == right_id && m.neighbors.right.is_none() {
                        m.neighbors.right = Some(left_id);
                    }
                }
            }
        }
    }
}

impl Default for TopologyConfig {
    fn default() -> Self {
        Self {
            monitors: Vec::new(),
            active_monitor_id: None,
            wrap_around: true,
            pause_switching: false,
            viewport_enabled: true, // Enabled by default, auto-hides on primary screen
            edge_delay_ms: 15,
            cursor_velocity_threshold: 0.0,
            osd_enabled: true,
            osd_duration_ms: 1500,
            osd_show_layout: false,
            osd_hide_physical_to_physical: false,
            gaming_guard_enabled: true,
            smart_focus_enabled: true,
            drag_teleport_enabled: true,
            match_virtual_mode_to_primary: true,
            follow_physical_window_activation: true,
            virtual_window_activation_action: VirtualWindowActivationAction::None,
            pip_enabled: false,
            pip_scale_percent: 15,
            pip_source_device_name: String::new(),
            pip_placements: Vec::new(),
            autostart_enabled: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn monitor(id: u32, x: i32, enabled: bool) -> MonitorConfig {
        MonitorConfig {
            id,
            device_name: format!(r"\\.\DISPLAY{id}"),
            name: format!("Display {id}"),
            bounds: DisplayBounds::new(x, 0, 1920, 1080),
            refresh_rate: 60,
            neighbors: Neighbors::default(),
            edge_activation_delay_ms: 15,
            is_enabled: enabled,
            is_virtual: false,
            layout_x: Some(x),
            layout_y: Some(0),
        }
    }

    #[test]
    fn disconnected_monitors_are_excluded_from_navigation() {
        let mut monitors = vec![
            monitor(1, 0, true),
            monitor(2, 1920, false),
            monitor(3, 3840, true),
        ];
        recompute_neighbors(&mut monitors, false);

        assert_eq!(monitors[0].neighbors.right, Some(3));
        assert_eq!(monitors[1].neighbors, Neighbors::default());
        assert_eq!(monitors[2].neighbors.left, Some(1));
    }

    #[test]
    fn removable_virtual_monitor_follows_driver_order_not_largest_id() {
        let mut first = monitor(8, 0, true);
        first.is_virtual = true;
        let mut last = monitor(3, 1920, true);
        last.is_virtual = true;
        let topology = TopologyConfig {
            monitors: vec![first.clone(), last.clone()],
            ..Default::default()
        };
        let displays = vec![
            DisplayInfo {
                id: 8,
                device_name: first.device_name,
                friendly_name: "Virtual A".into(),
                bounds: first.bounds,
                refresh_rate: 60,
                is_primary: false,
                is_virtual: true,
            },
            DisplayInfo {
                id: 3,
                device_name: last.device_name,
                friendly_name: "Virtual B".into(),
                bounds: last.bounds,
                refresh_rate: 60,
                is_primary: false,
                is_virtual: true,
            },
        ];

        assert_eq!(removable_virtual_monitor_id(&topology, &displays), Some(3));
    }

    #[test]
    fn virtual_window_activation_setting_is_backward_compatible() {
        let mut legacy = serde_json::to_value(TopologyConfig::default()).unwrap();
        let legacy_object = legacy.as_object_mut().unwrap();
        legacy_object.remove("virtual_window_activation_action");
        legacy_object.remove("match_virtual_mode_to_primary");
        legacy_object.remove("follow_physical_window_activation");

        let restored: TopologyConfig = serde_json::from_value(legacy).unwrap();
        assert_eq!(
            restored.virtual_window_activation_action,
            VirtualWindowActivationAction::None
        );
        assert!(restored.match_virtual_mode_to_primary);
        assert!(restored.follow_physical_window_activation);

        for action in [
            VirtualWindowActivationAction::None,
            VirtualWindowActivationAction::SwitchToVirtual,
            VirtualWindowActivationAction::MoveToPhysical,
        ] {
            let config = TopologyConfig {
                virtual_window_activation_action: action,
                ..Default::default()
            };
            let json = serde_json::to_string(&config).unwrap();
            let round_trip: TopologyConfig = serde_json::from_str(&json).unwrap();
            assert_eq!(round_trip.virtual_window_activation_action, action);
        }
    }

    #[test]
    fn product_capabilities_round_trip_over_ipc_json() {
        let response = IpcResponse::ProductCapabilities(ProductCapabilities {
            edition: ProductEdition::Lite,
            max_virtual_displays: 1,
            multi_display_layouts: false,
            cloud_features: false,
        });
        let json = serde_json::to_string(&response).unwrap();
        let restored: IpcResponse = serde_json::from_str(&json).unwrap();
        match restored {
            IpcResponse::ProductCapabilities(capabilities) => {
                assert_eq!(capabilities.edition, ProductEdition::Lite);
                assert_eq!(capabilities.max_virtual_displays, 1);
            }
            _ => panic!("unexpected IPC response variant"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SwitchReason {
    Cursor,
    Hotkey,
    Ui,
    Api,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProductEdition {
    Lite,
    Pro,
}

impl ProductEdition {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Lite => "Lite",
            Self::Pro => "Pro",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProductCapabilities {
    pub edition: ProductEdition,
    pub max_virtual_displays: u32,
    pub multi_display_layouts: bool,
    pub cloud_features: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum IpcRequest {
    GetProductCapabilities,
    GetDisplays,
    GetTopology,
    SetActiveMonitor(u32),
    UpdateTopology(TopologyConfig),
    SetPauseSwitching(bool),
    SetViewportEnabled(bool),
    AddMonitor {
        name: String,
        width: u32,
        height: u32,
        refresh_rate: u32,
    },
    ConfirmMonitor(u32),
    RemoveMonitor(u32),
    MoveMonitorLeft(u32),
    MoveMonitorRight(u32),
    SetNeighbors {
        monitor_id: u32,
        left: Option<u32>,
        right: Option<u32>,
        top: Option<u32>,
        bottom: Option<u32>,
    },
    AutoArrange(ArrangeMode),
    Ping,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ArrangeMode {
    Horizontal,
    Vertical,
    Grid2x2,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum IpcResponse {
    ProductCapabilities(ProductCapabilities),
    Displays(Vec<DisplayInfo>),
    Topology(TopologyConfig),
    Pong,
    Success,
    MonitorAdded(u32),
    MonitorRemoved(u32),
    Error(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum IpcEvent {
    MonitorSwitched {
        from: Option<u32>,
        to: u32,
        reason: SwitchReason,
    },
    CursorEdgeTriggered {
        monitor_id: u32,
        edge: EdgeDirection,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EdgeDirection {
    Left,
    Right,
    Top,
    Bottom,
}
