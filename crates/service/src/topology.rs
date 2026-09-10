#[cfg(test)]
use multitor_ipc::DisplayBounds;
use multitor_ipc::{
    ArrangeMode, EdgeDirection, MonitorConfig, Neighbors, SwitchReason, TopologyConfig,
};
use tracing::{info, warn};

pub struct TopologyManager {
    config: TopologyConfig,
}

impl TopologyManager {
    pub fn new(config: TopologyConfig) -> Self {
        Self { config }
    }

    pub fn config(&self) -> &TopologyConfig {
        &self.config
    }

    pub fn config_mut(&mut self) -> &mut TopologyConfig {
        &mut self.config
    }

    pub fn active_monitor(&self) -> Option<&MonitorConfig> {
        let active_id = self.config.active_monitor_id?;
        self.config.monitors.iter().find(|m| m.id == active_id)
    }

    pub fn get_monitor(&self, id: u32) -> Option<&MonitorConfig> {
        self.config.monitors.iter().find(|m| m.id == id)
    }

    /// Add a new virtual display into the topology
    #[cfg(test)]
    pub fn add_virtual_monitor(
        &mut self,
        name: String,
        width: u32,
        height: u32,
        refresh_rate: u32,
    ) -> u32 {
        let max_id = self.config.monitors.iter().map(|m| m.id).max().unwrap_or(0);
        let new_id = max_id + 1;

        let rightmost_x = self
            .config
            .monitors
            .iter()
            .map(|m| m.bounds.right())
            .max()
            .unwrap_or(0);
        let new_bounds = DisplayBounds::new(rightmost_x, 0, width, height);

        let last_id = self.config.monitors.last().map(|m| m.id);

        let new_monitor = MonitorConfig {
            id: new_id,
            device_name: String::new(),
            name: if name.is_empty() {
                format!("Virtual {}", new_id)
            } else {
                name
            },
            bounds: new_bounds,
            refresh_rate,
            neighbors: Neighbors {
                left: last_id,
                right: None,
                top: None,
                bottom: None,
            },
            edge_activation_delay_ms: 15,
            is_enabled: true,
            is_virtual: true,
            layout_x: None,
            layout_y: None,
        };

        if let Some(prev_id) = last_id {
            if let Some(prev) = self.config.monitors.iter_mut().find(|m| m.id == prev_id) {
                prev.neighbors.right = Some(new_id);
            }
        }

        self.config.monitors.push(new_monitor);

        if self.config.active_monitor_id.is_none() {
            self.config.active_monitor_id = Some(new_id);
        }

        self.auto_arrange(ArrangeMode::Horizontal);
        info!("Added virtual monitor ID {} at x={}", new_id, rightmost_x);
        new_id
    }

    /// Move monitor position to the left in the chain
    pub fn move_monitor_left(&mut self, id: u32) -> bool {
        let idx = match self.config.monitors.iter().position(|m| m.id == id) {
            Some(i) => i,
            None => return false,
        };
        if idx > 0 {
            let prev_x = self.config.monitors[idx - 1].layout_bounds().x;
            let cur_w = self.config.monitors[idx].bounds.width as i32;
            self.config.monitors[idx].layout_x = Some(prev_x);
            self.config.monitors[idx - 1].layout_x = Some(prev_x + cur_w);
            self.config.monitors.swap(idx, idx - 1);
            self.auto_arrange(ArrangeMode::Horizontal);
            info!(
                "Moved monitor ID {} to the left (new index {})",
                id,
                idx - 1
            );
            true
        } else {
            false
        }
    }

    /// Move monitor position to the right in the chain
    pub fn move_monitor_right(&mut self, id: u32) -> bool {
        let idx = match self.config.monitors.iter().position(|m| m.id == id) {
            Some(i) => i,
            None => return false,
        };
        if idx + 1 < self.config.monitors.len() {
            let cur_x = self.config.monitors[idx].layout_bounds().x;
            let next_w = self.config.monitors[idx + 1].bounds.width as i32;
            self.config.monitors[idx].layout_x = Some(cur_x + next_w);
            self.config.monitors[idx + 1].layout_x = Some(cur_x);
            self.config.monitors.swap(idx, idx + 1);
            self.auto_arrange(ArrangeMode::Horizontal);
            info!(
                "Moved monitor ID {} to the right (new index {})",
                id,
                idx + 1
            );
            true
        } else {
            false
        }
    }

    /// Set manual neighbors
    pub fn set_neighbors(
        &mut self,
        id: u32,
        left: Option<u32>,
        right: Option<u32>,
        top: Option<u32>,
        bottom: Option<u32>,
    ) {
        if let Some(m) = self.config.monitors.iter_mut().find(|m| m.id == id) {
            m.neighbors = Neighbors {
                left,
                right,
                top,
                bottom,
            };
            info!(
                "Set neighbors for monitor {}: L={:?} R={:?} T={:?} B={:?}",
                id, left, right, top, bottom
            );
        }
    }

    /// Auto-arrange monitors in topology (Horizontal, Vertical, Grid2x2)
    pub fn auto_arrange(&mut self, mode: ArrangeMode) {
        let count = self.config.monitors.len();
        if count == 0 {
            return;
        }

        match mode {
            ArrangeMode::Horizontal => {
                // Sort by spatial X coordinate first
                self.config.monitors.sort_by_key(|m| m.layout_bounds().x);
                let count = self.config.monitors.len();
                for i in 0..count {
                    let left_id = if i > 0 {
                        Some(self.config.monitors[i - 1].id)
                    } else if self.config.wrap_around && count > 1 {
                        Some(self.config.monitors[count - 1].id)
                    } else {
                        None
                    };

                    let right_id = if i + 1 < count {
                        Some(self.config.monitors[i + 1].id)
                    } else if self.config.wrap_around && count > 1 {
                        Some(self.config.monitors[0].id)
                    } else {
                        None
                    };

                    self.config.monitors[i].neighbors = Neighbors {
                        left: left_id,
                        right: right_id,
                        top: None,
                        bottom: None,
                    };
                }
            }
            ArrangeMode::Vertical => {
                // Sort by spatial Y coordinate first
                self.config.monitors.sort_by_key(|m| m.layout_bounds().y);
                let count = self.config.monitors.len();
                for i in 0..count {
                    let top_id = if i > 0 {
                        Some(self.config.monitors[i - 1].id)
                    } else {
                        None
                    };
                    let bottom_id = if i + 1 < count {
                        Some(self.config.monitors[i + 1].id)
                    } else {
                        None
                    };

                    self.config.monitors[i].neighbors = Neighbors {
                        left: None,
                        right: None,
                        top: top_id,
                        bottom: bottom_id,
                    };
                }
            }
            ArrangeMode::Grid2x2 => {
                self.config
                    .monitors
                    .sort_by_key(|m| (m.layout_bounds().y / 500, m.layout_bounds().x));
                let count = self.config.monitors.len();
                let ids: Vec<u32> = self.config.monitors.iter().map(|m| m.id).collect();
                for (i, m) in self.config.monitors.iter_mut().enumerate() {
                    let row = i / 2;
                    let col = i % 2;

                    let left = if col > 0 { Some(ids[i - 1]) } else { None };
                    let right = if col == 0 && i + 1 < count {
                        Some(ids[i + 1])
                    } else {
                        None
                    };
                    let top = if row > 0 { Some(ids[i - 2]) } else { None };
                    let bottom = if row == 0 && i + 2 < count {
                        Some(ids[i + 2])
                    } else {
                        None
                    };

                    m.neighbors = Neighbors {
                        left,
                        right,
                        top,
                        bottom,
                    };
                }
            }
        }
        info!(
            "Auto-arranged {} monitor(s) with mode: {:?}",
            self.config.monitors.len(),
            mode
        );
    }

    /// Set active monitor and return (previous_id, new_id)
    pub fn set_active_monitor(
        &mut self,
        target_id: u32,
        reason: SwitchReason,
    ) -> Option<(Option<u32>, u32)> {
        if !self
            .config
            .monitors
            .iter()
            .any(|m| m.id == target_id && m.is_enabled)
        {
            warn!("Target monitor ID {} not found in topology", target_id);
            return None;
        }

        let prev_id = self.config.active_monitor_id;
        if prev_id == Some(target_id) {
            return None;
        }

        self.config.active_monitor_id = Some(target_id);
        info!(
            "Switched active monitor: {:?} -> {} (reason: {:?})",
            prev_id, target_id, reason
        );
        Some((prev_id, target_id))
    }

    /// Set wrap-around mode and update arrangements
    #[allow(dead_code)]
    pub fn set_wrap_around(&mut self, wrap: bool) {
        self.config.wrap_around = wrap;
        self.auto_arrange(ArrangeMode::Horizontal);
    }

    /// Determine neighbor monitor for a given edge transition
    pub fn find_neighbor_on_edge(&self, from_id: u32, edge: EdgeDirection) -> Option<u32> {
        let monitor = self.get_monitor(from_id)?;
        match edge {
            EdgeDirection::Left => monitor.neighbors.left,
            EdgeDirection::Right => monitor.neighbors.right,
            EdgeDirection::Top => monitor.neighbors.top,
            EdgeDirection::Bottom => monitor.neighbors.bottom,
        }
    }

    /// Checks if two monitors are natively adjacent in Windows OS desktop coordinates
    /// along the specified edge direction. When this is true, Windows DWM already connects
    /// the monitors natively, so artificial edge teleportation and clipping must be skipped.
    pub fn is_os_adjacent(&self, from_id: u32, to_id: u32, edge: EdgeDirection) -> bool {
        let from = match self.get_monitor(from_id) {
            Some(m) => m,
            None => return false,
        };
        let to = match self.get_monitor(to_id) {
            Some(m) => m,
            None => return false,
        };

        let f = &from.bounds;
        let t = &to.bounds;

        match edge {
            EdgeDirection::Right => f.right() == t.x && f.y < t.bottom() && f.bottom() > t.y,
            EdgeDirection::Left => f.x == t.right() && f.y < t.bottom() && f.bottom() > t.y,
            EdgeDirection::Bottom => f.bottom() == t.y && f.x < t.right() && f.right() > t.x,
            EdgeDirection::Top => f.y == t.bottom() && f.x < t.right() && f.right() > t.x,
        }
    }

    /// Calculate new cursor position on target monitor after edge jump
    pub fn calculate_target_cursor_pos(
        &self,
        from_id: u32,
        to_id: u32,
        edge: EdgeDirection,
        cur_x: i32,
        cur_y: i32,
    ) -> Option<(i32, i32)> {
        let from_mon = self.get_monitor(from_id)?;
        let to_mon = self.get_monitor(to_id)?;

        let margin = 45;

        match edge {
            EdgeDirection::Right => {
                let new_x = to_mon.bounds.x + margin;
                let rel_y =
                    (cur_y - from_mon.bounds.y) as f32 / from_mon.bounds.height.max(1) as f32;
                let new_y = to_mon.bounds.y + (rel_y * to_mon.bounds.height as f32).round() as i32;
                let min_y = to_mon.bounds.y + 10;
                let max_y = (to_mon.bounds.bottom() - 10).max(min_y);
                Some((new_x, new_y.clamp(min_y, max_y)))
            }
            EdgeDirection::Left => {
                let new_x = to_mon.bounds.right() - margin;
                let rel_y =
                    (cur_y - from_mon.bounds.y) as f32 / from_mon.bounds.height.max(1) as f32;
                let new_y = to_mon.bounds.y + (rel_y * to_mon.bounds.height as f32).round() as i32;
                let min_y = to_mon.bounds.y + 10;
                let max_y = (to_mon.bounds.bottom() - 10).max(min_y);
                Some((new_x, new_y.clamp(min_y, max_y)))
            }
            EdgeDirection::Bottom => {
                let new_y = to_mon.bounds.y + margin;
                let rel_x =
                    (cur_x - from_mon.bounds.x) as f32 / from_mon.bounds.width.max(1) as f32;
                let new_x = to_mon.bounds.x + (rel_x * to_mon.bounds.width as f32).round() as i32;
                let min_x = to_mon.bounds.x + 10;
                let max_x = (to_mon.bounds.right() - 10).max(min_x);
                Some((new_x.clamp(min_x, max_x), new_y))
            }
            EdgeDirection::Top => {
                let new_y = to_mon.bounds.bottom() - margin;
                let rel_x =
                    (cur_x - from_mon.bounds.x) as f32 / from_mon.bounds.width.max(1) as f32;
                let new_x = to_mon.bounds.x + (rel_x * to_mon.bounds.width as f32).round() as i32;
                let min_x = to_mon.bounds.x + 10;
                let max_x = (to_mon.bounds.right() - 10).max(min_x);
                Some((new_x.clamp(min_x, max_x), new_y))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_move_left_right() {
        let mut mgr = TopologyManager::new(TopologyConfig::default());
        let id1 = mgr.add_virtual_monitor("Main".to_string(), 1920, 1080, 60);
        let id2 = mgr.add_virtual_monitor("Virtual".to_string(), 1920, 1080, 60);

        // id1 is at index 0, id2 is at index 1
        assert_eq!(mgr.config().monitors[0].id, id1);
        assert_eq!(mgr.config().monitors[1].id, id2);

        // Move id2 left -> id2 is now at index 0 (LEFT of Main!)
        assert!(mgr.move_monitor_left(id2));
        assert_eq!(mgr.config().monitors[0].id, id2);
        assert_eq!(mgr.config().monitors[1].id, id1);

        // Virtual (id2) is on the left, so id1's left neighbor is id2!
        assert_eq!(
            mgr.find_neighbor_on_edge(id1, EdgeDirection::Left),
            Some(id2)
        );
        // id2's right neighbor is id1!
        assert_eq!(
            mgr.find_neighbor_on_edge(id2, EdgeDirection::Right),
            Some(id1)
        );
    }

    #[test]
    fn test_wrap_around_disabled() {
        let mut mgr = TopologyManager::new(TopologyConfig::default());
        let id1 = mgr.add_virtual_monitor("Monitor 1".to_string(), 1920, 1080, 60);
        let id2 = mgr.add_virtual_monitor("Monitor 2".to_string(), 1920, 1080, 60);

        // Initially wrap_around is true
        mgr.set_wrap_around(true);
        assert_eq!(
            mgr.find_neighbor_on_edge(id1, EdgeDirection::Left),
            Some(id2)
        );
        assert_eq!(
            mgr.find_neighbor_on_edge(id2, EdgeDirection::Right),
            Some(id1)
        );

        // Disable wrap_around
        mgr.set_wrap_around(false);
        // Going left from leftmost monitor must NOT wrap!
        assert_eq!(mgr.find_neighbor_on_edge(id1, EdgeDirection::Left), None);
        // Going right from rightmost monitor must NOT wrap!
        assert_eq!(mgr.find_neighbor_on_edge(id2, EdgeDirection::Right), None);
        // Normal transition between neighbors still works!
        assert_eq!(
            mgr.find_neighbor_on_edge(id1, EdgeDirection::Right),
            Some(id2)
        );
        assert_eq!(
            mgr.find_neighbor_on_edge(id2, EdgeDirection::Left),
            Some(id1)
        );
    }

    #[test]
    fn test_spatial_canvas_left_virtual_monitor() {
        let mut cfg = TopologyConfig {
            wrap_around: false,
            ..Default::default()
        };

        // User setup:
        // Mon 1: MAIN (x: 0, layout_x: 876)
        // Mon 2: Screen 2 (x: 2560, layout_x: 3436)
        // Mon 3: Virtual (x: 4480, layout_x: -1684)
        let m1 = MonitorConfig {
            id: 1,
            device_name: r"\\.\DISPLAY1".to_string(),
            name: "MAIN".to_string(),
            bounds: DisplayBounds::new(0, 0, 2560, 1440),
            refresh_rate: 180,
            neighbors: Neighbors::default(),
            edge_activation_delay_ms: 15,
            is_enabled: true,
            is_virtual: false,
            layout_x: Some(876),
            layout_y: Some(2240),
        };
        let m2 = MonitorConfig {
            id: 2,
            device_name: r"\\.\DISPLAY2".to_string(),
            name: "Screen 2".to_string(),
            bounds: DisplayBounds::new(2560, 0, 1920, 1080),
            refresh_rate: 75,
            neighbors: Neighbors::default(),
            edge_activation_delay_ms: 15,
            is_enabled: true,
            is_virtual: false,
            layout_x: Some(3436),
            layout_y: Some(2240),
        };
        let m3 = MonitorConfig {
            id: 3,
            device_name: r"\\.\DISPLAY3".to_string(),
            name: "Экран 3".to_string(),
            bounds: DisplayBounds::new(4480, 0, 2560, 1440),
            refresh_rate: 60,
            neighbors: Neighbors::default(),
            edge_activation_delay_ms: 15,
            is_enabled: true,
            is_virtual: true,
            layout_x: Some(-1684),
            layout_y: Some(2240),
        };

        let mut monitors = vec![m1.clone(), m2.clone(), m3.clone()];
        multitor_ipc::recompute_neighbors(&mut monitors, false);
        cfg.monitors = monitors;
        cfg.active_monitor_id = Some(1);

        let mut mgr = TopologyManager::new(cfg);

        // Moving left from Monitor 1 (MAIN) must target Monitor 3!
        assert_eq!(mgr.find_neighbor_on_edge(1, EdgeDirection::Left), Some(3));
        // Moving right from Monitor 3 must target Monitor 1!
        assert_eq!(mgr.find_neighbor_on_edge(3, EdgeDirection::Right), Some(1));
        // Moving right from Monitor 1 must target Monitor 2!
        assert_eq!(mgr.find_neighbor_on_edge(1, EdgeDirection::Right), Some(2));
        // Moving left from Monitor 2 must target Monitor 1!
        assert_eq!(mgr.find_neighbor_on_edge(2, EdgeDirection::Left), Some(1));
        // Moving left from Monitor 3 has no neighbor (wrap_around is false)!
        assert_eq!(mgr.find_neighbor_on_edge(3, EdgeDirection::Left), None);

        // Target cursor pos for jump to Monitor 3 (Left edge of Mon 1 -> Right edge of Mon 3)
        let (tx, ty) = mgr
            .calculate_target_cursor_pos(1, 3, EdgeDirection::Left, 0, 500)
            .unwrap();
        // Mon 3 right is 4480 + 2560 = 7040. Landing margin is 45 -> 7040 - 45 = 6995
        assert_eq!(tx, 6995);
        assert_eq!(ty, 500);

        // Verify Windows OS native adjacency:
        // Physical Mon 1 and Physical Mon 2 ARE natively adjacent in Windows (right/left)
        assert!(mgr.is_os_adjacent(1, 2, EdgeDirection::Right));
        assert!(mgr.is_os_adjacent(2, 1, EdgeDirection::Left));
        // Native adjacency is based on Windows coordinates, regardless of
        // whether an output is physical or virtual.
        assert!(!mgr.is_os_adjacent(1, 3, EdgeDirection::Left));
        assert!(!mgr.is_os_adjacent(3, 1, EdgeDirection::Right));
        assert!(mgr.is_os_adjacent(2, 3, EdgeDirection::Right));

        // After applying the canvas layout to Windows, the virtual output is
        // physically adjacent to MAIN on its left and DWM can handle dragging.
        mgr.config_mut()
            .monitors
            .iter_mut()
            .find(|m| m.id == 3)
            .unwrap()
            .bounds
            .x = -2560;
        assert!(mgr.is_os_adjacent(1, 3, EdgeDirection::Left));
        assert!(mgr.is_os_adjacent(3, 1, EdgeDirection::Right));
    }
}
