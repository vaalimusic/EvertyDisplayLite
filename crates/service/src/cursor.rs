use crate::topology::TopologyManager;
use multitor_ipc::{EdgeDirection, SwitchReason};
use std::mem::zeroed;
use std::time::{Duration, Instant};
use tracing::info;
use windows::Win32::Foundation::{POINT, RECT};
use windows::Win32::UI::WindowsAndMessaging::{ClipCursor, GetCursorPos, SetCursorPos};

#[derive(Debug, Clone)]
pub struct SwitchResult {
    pub target_id: u32,
    pub dragged_window: Option<String>,
}

pub struct CursorTracker {
    edge_threshold: i32,
    cooldown_duration: Duration,
    last_switch_time: Instant,
    active_edge: Option<(EdgeDirection, Instant)>,
    last_pos: (i32, i32),
    last_pos_time: Instant,
    has_initial_sample: bool,
}

impl CursorTracker {
    pub fn new() -> Self {
        Self {
            edge_threshold: 10, // Within 10 pixels of monitor edge
            cooldown_duration: Duration::from_millis(250),
            last_switch_time: Instant::now() - Duration::from_secs(1),
            active_edge: None,
            last_pos: (0, 0),
            last_pos_time: Instant::now(),
            has_initial_sample: false,
        }
    }

    /// Read Windows current cursor position
    pub fn get_current_cursor_pos() -> Option<(i32, i32)> {
        let mut pt: POINT = unsafe { zeroed() };
        if unsafe { GetCursorPos(&mut pt) }.is_ok() {
            Some((pt.x, pt.y))
        } else {
            None
        }
    }

    /// Teleport cursor to target position
    pub fn teleport_cursor(x: i32, y: i32) -> bool {
        unsafe { SetCursorPos(x, y).is_ok() }
    }

    /// Confine cursor to a rectangle or release confinement
    pub fn clip_cursor_to(rect: Option<RECT>) {
        unsafe {
            match rect {
                Some(r) => {
                    let _ = ClipCursor(Some(&r));
                }
                None => {
                    let _ = ClipCursor(None);
                }
            }
        }
    }

    /// Check cursor and execute edge switch if conditions met.
    /// Returns Some(SwitchResult) if a switch occurred.
    pub fn update(
        &mut self,
        topology: &mut TopologyManager,
        runtime_pause: bool,
    ) -> Option<SwitchResult> {
        let (cur_x, cur_y) = Self::get_current_cursor_pos()?;

        // Pause disables synthetic edge jumps, but it must never freeze the monitor identity.
        // Native Windows movement can still place the cursor on another attached display
        // (notably while a browser/video is fullscreen). Keep Viewport state synchronized so
        // its fullscreen mirror is removed from MAIN as soon as the cursor really arrives there.
        if topology.config().pause_switching || runtime_pause {
            Self::clip_cursor_to(None);
            let actual_id = topology
                .config()
                .monitors
                .iter()
                .find(|monitor| monitor.is_enabled && monitor.bounds.contains_point(cur_x, cur_y))
                .map(|monitor| monitor.id);
            let previous_id = topology.config().active_monitor_id;
            self.last_pos = (cur_x, cur_y);
            self.last_pos_time = Instant::now();
            self.active_edge = None;

            if let Some(actual_id) = actual_id {
                if previous_id != Some(actual_id) {
                    topology.set_active_monitor(actual_id, SwitchReason::Cursor);
                    return Some(SwitchResult {
                        target_id: actual_id,
                        dragged_window: None,
                    });
                }
            }
            return None;
        }

        let now = Instant::now();

        let active_mon = match topology.active_monitor() {
            Some(m) => m.clone(),
            None => {
                Self::clip_cursor_to(None);
                return None;
            }
        };

        // The persisted active monitor is only a UI preference. On service start,
        // the real cursor position is authoritative; otherwise a stale virtual
        // monitor selection can make the cursor appear permanently trapped.
        if !self.has_initial_sample {
            self.has_initial_sample = true;
            self.last_pos = (cur_x, cur_y);
            self.last_pos_time = now;
            self.active_edge = None;
            Self::clip_cursor_to(None);
            if let Some(actual_id) = topology
                .config()
                .monitors
                .iter()
                .find(|m| m.is_enabled && m.bounds.contains_point(cur_x, cur_y))
                .map(|m| m.id)
            {
                topology.set_active_monitor(actual_id, SwitchReason::Cursor);
            }
            return None;
        }

        if self.last_switch_time.elapsed() < self.cooldown_duration {
            // Cooldown only suppresses another synthetic edge jump. It must not
            // constrain normal Windows cursor or window-drag behaviour.
            Self::clip_cursor_to(None);
            self.last_pos = (cur_x, cur_y);
            self.last_pos_time = now;
            self.active_edge = None;
            return None;
        }

        // Never constrain the cursor during normal use or window dragging.
        // The canvas layout is applied to Windows, so DWM owns these interactions.
        Self::clip_cursor_to(None);

        // If cursor is outside the active monitor entirely, locate which monitor it's on
        if !active_mon.bounds.contains_point(cur_x, cur_y) {
            let actual_mon_opt = topology
                .config()
                .monitors
                .iter()
                .find(|m| m.bounds.contains_point(cur_x, cur_y));
            if let Some(actual_mon) = actual_mon_opt {
                let actual_id = actual_mon.id;

                // If the previous sample was not on the configured active monitor,
                // this was an external warp/stale state, not an edge crossing.
                // Re-synchronise instead of fighting Windows and trapping the user.
                if !active_mon
                    .bounds
                    .contains_point(self.last_pos.0, self.last_pos.1)
                {
                    Self::clip_cursor_to(None);
                    topology.set_active_monitor(actual_id, SwitchReason::Cursor);
                    self.last_pos = (cur_x, cur_y);
                    self.last_pos_time = now;
                    self.active_edge = None;
                    return None;
                }

                topology.set_active_monitor(actual_id, SwitchReason::Cursor);
                self.last_switch_time = now;
                self.last_pos = (cur_x, cur_y);
                self.last_pos_time = now;
                self.active_edge = None;
                return Some(SwitchResult {
                    target_id: actual_id,
                    dragged_window: None,
                });
            } else {
                // Windows may briefly report a point in a topology gap while a
                // layout is changing. Observe it; never yank the cursor away.
                self.last_pos = (cur_x, cur_y);
                self.last_pos_time = now;
                return None;
            }
        }

        // Detect if cursor is against an edge
        let bounds = active_mon.bounds;
        let detected_edge = if cur_x >= bounds.right() - self.edge_threshold {
            Some(EdgeDirection::Right)
        } else if cur_x <= bounds.x + self.edge_threshold {
            Some(EdgeDirection::Left)
        } else if cur_y >= bounds.bottom() - self.edge_threshold {
            Some(EdgeDirection::Bottom)
        } else if cur_y <= bounds.y + self.edge_threshold {
            Some(EdgeDirection::Top)
        } else {
            None
        };

        let edge_delay = Duration::from_millis(if active_mon.edge_activation_delay_ms > 0 {
            active_mon.edge_activation_delay_ms
        } else if topology.config().edge_delay_ms > 0 {
            topology.config().edge_delay_ms
        } else {
            15
        });

        if let Some(edge) = detected_edge {
            if let Some(target_id) = topology.find_neighbor_on_edge(active_mon.id, edge) {
                // If target monitor is natively adjacent in Windows OS and both are physical:
                // Let Windows handle native cursor movement across screens! Do NOT teleport!
                if topology.is_os_adjacent(active_mon.id, target_id, edge) {
                    self.active_edge = None;
                    self.last_pos = (cur_x, cur_y);
                    self.last_pos_time = now;
                    return None;
                }

                match self.active_edge {
                    Some((prev_edge, enter_time)) if prev_edge == edge => {
                        if enter_time.elapsed() >= edge_delay {
                            // Edge activation threshold reached!
                            if let Some((target_x, target_y)) = topology
                                .calculate_target_cursor_pos(
                                    active_mon.id,
                                    target_id,
                                    edge,
                                    cur_x,
                                    cur_y,
                                )
                            {
                                info!(
                                    "Edge {:?} triggered on monitor {} -> switching to {} and teleporting to ({}, {})",
                                    edge, active_mon.id, target_id, target_x, target_y
                                );
                                Self::clip_cursor_to(None);

                                Self::teleport_cursor(target_x, target_y);
                                topology.set_active_monitor(target_id, SwitchReason::Cursor);
                                self.last_switch_time = Instant::now();
                                self.last_pos = (target_x, target_y);
                                self.last_pos_time = Instant::now();
                                self.active_edge = None;
                                return Some(SwitchResult {
                                    target_id,
                                    dragged_window: None,
                                });
                            }
                        }
                    }
                    _ => {
                        // Just touched edge
                        if self.active_edge.as_ref().map(|(e, _)| *e) != Some(edge) {
                            info!(
                                "Cursor entered {:?} edge on monitor {} (cur_x={}, cur_y={}, bounds=[{}, {}])",
                                edge, active_mon.id, cur_x, cur_y, bounds.x, bounds.right()
                            );
                        }
                        self.active_edge = Some((edge, now));
                    }
                }
            } else {
                self.active_edge = None;
            }
        } else {
            // Not on any edge
            self.active_edge = None;
        }

        self.last_pos = (cur_x, cur_y);
        self.last_pos_time = now;

        None
    }
}

impl Drop for CursorTracker {
    fn drop(&mut self) {
        Self::clip_cursor_to(None);
    }
}
