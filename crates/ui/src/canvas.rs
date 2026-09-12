use crate::i18n::{self, Language};
use crate::styles::{ThemeMode, PRIMARY};
use crate::Message;
use crate::{ui_font, ui_font_with_weight};
use iced::font::Weight;
use iced::mouse;
use iced::widget::canvas::{event, Event, Frame, Geometry, Path, Program, Stroke, Text};
use iced::{Color, Point, Rectangle, Renderer, Size, Theme};
use multitor_ipc::MonitorConfig;

#[derive(Debug, Clone)]
pub struct CanvasState {
    pub dragging_id: Option<u32>,
    pub drag_start_cursor: Point,
    pub drag_initial_pos: (i32, i32),
    pub current_pos: (i32, i32),
    // Zoom & pan
    pub user_zoom: f32,
    pub pan_offset: (f32, f32),
    pub is_panning: bool,
    pub pan_start_cursor: Point,
    pub pan_start_offset: (f32, f32),
    pub last_reset_tag: u32,
    /// Fixed transform captured at drag start. Re-fitting the whole topology while an
    /// outer monitor moves causes every tile to slide under the pointer.
    pub drag_transform: Option<(f32, f32, f32, i32, i32)>,
}

impl Default for CanvasState {
    fn default() -> Self {
        Self {
            dragging_id: None,
            drag_start_cursor: Point::ORIGIN,
            drag_initial_pos: (0, 0),
            current_pos: (0, 0),
            user_zoom: 1.0,
            pan_offset: (0.0, 0.0),
            is_panning: false,
            pan_start_cursor: Point::ORIGIN,
            pan_start_offset: (0.0, 0.0),
            last_reset_tag: 0,
            drag_transform: None,
        }
    }
}

#[allow(dead_code)]
pub struct SpatialCanvas<'a> {
    pub monitors: &'a [MonitorConfig],
    pub active_id: Option<u32>,
    pub selected_id: Option<u32>,
    pub theme_mode: ThemeMode,
    pub wrap_around: bool,
    pub reset_tag: u32,
}

impl<'a> SpatialCanvas<'a> {
    pub fn new(
        monitors: &'a [MonitorConfig],
        active_id: Option<u32>,
        selected_id: Option<u32>,
        theme_mode: ThemeMode,
        wrap_around: bool,
        reset_tag: u32,
    ) -> Self {
        Self {
            monitors,
            active_id,
            selected_id,
            theme_mode,
            wrap_around,
            reset_tag,
        }
    }

    /// Computes (scale, origin_x, origin_y, min_x, min_y) to fit all monitors into canvas bounds
    pub fn compute_transform(
        &self,
        state: &CanvasState,
        bounds: Rectangle,
    ) -> (f32, f32, f32, i32, i32) {
        if let Some(transform) = state.drag_transform {
            return transform;
        }
        if self.monitors.is_empty() {
            return (
                0.1,
                bounds.x + bounds.width / 2.0,
                bounds.y + bounds.height / 2.0,
                0,
                0,
            );
        }

        let mut min_x = i32::MAX;
        let mut max_x = i32::MIN;
        let mut min_y = i32::MAX;
        let mut max_y = i32::MIN;

        for m in self.monitors {
            let lb = m.layout_bounds();
            let (mx, my) = if state.dragging_id == Some(m.id) {
                state.current_pos
            } else {
                (lb.x, lb.y)
            };

            min_x = min_x.min(mx);
            max_x = max_x.max(mx + lb.width as i32);
            min_y = min_y.min(my);
            max_y = max_y.max(my + lb.height as i32);
        }

        let span_w = (max_x - min_x).max(1280) as f32;
        let span_h = (max_y - min_y).max(720) as f32;

        let pad = 24.0;
        let avail_w = (bounds.width - pad * 2.0).max(100.0);
        let avail_h = (bounds.height - pad * 2.0).max(100.0);

        let base_scale = (avail_w / span_w).min(avail_h / span_h).clamp(0.03, 0.20);
        let scale = (base_scale * state.user_zoom).clamp(0.01, 1.0);

        let total_scaled_w = span_w * scale;
        let total_scaled_h = span_h * scale;

        let origin_x = bounds.x + (bounds.width - total_scaled_w) / 2.0 + state.pan_offset.0;
        let origin_y = bounds.y + (bounds.height - total_scaled_h) / 2.0 + state.pan_offset.1;

        (scale, origin_x, origin_y, min_x, min_y)
    }
}

impl<'a> Program<Message, Theme, Renderer> for SpatialCanvas<'a> {
    type State = CanvasState;

    fn update(
        &self,
        state: &mut Self::State,
        event: Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> (event::Status, Option<Message>) {
        if state.last_reset_tag != self.reset_tag {
            state.last_reset_tag = self.reset_tag;
            state.user_zoom = 1.0;
            state.pan_offset = (0.0, 0.0);
            state.is_panning = false;
            state.dragging_id = None;
            state.drag_transform = None;
        }

        let (scale, origin_x, origin_y, min_x, min_y) = self.compute_transform(state, bounds);

        match event {
            Event::Mouse(mouse::Event::WheelScrolled { delta }) => {
                if cursor.position_in(bounds).is_some() {
                    let scroll_y = match delta {
                        mouse::ScrollDelta::Lines { y, .. } => y,
                        mouse::ScrollDelta::Pixels { y, .. } => y / 40.0,
                    };
                    if scroll_y != 0.0 {
                        let zoom_factor = if scroll_y > 0.0 { 1.15 } else { 0.85 };
                        state.user_zoom = (state.user_zoom * zoom_factor).clamp(0.2, 5.0);
                        return (event::Status::Captured, None);
                    }
                }
            }
            Event::Mouse(mouse::Event::ButtonPressed(
                mouse::Button::Right | mouse::Button::Middle,
            )) => {
                if let Some(cursor_pos) = cursor.position_in(bounds) {
                    let global_pos = Point::new(bounds.x + cursor_pos.x, bounds.y + cursor_pos.y);
                    state.is_panning = true;
                    state.pan_start_cursor = global_pos;
                    state.pan_start_offset = state.pan_offset;
                    return (event::Status::Captured, None);
                }
            }
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                if let Some(cursor_pos) = cursor.position_in(bounds) {
                    let global_pos = Point::new(bounds.x + cursor_pos.x, bounds.y + cursor_pos.y);

                    // Check which monitor was clicked (in reverse to hit top-most)
                    for m in self.monitors.iter().rev() {
                        let lb = m.layout_bounds();
                        let mx = lb.x;
                        let my = lb.y;
                        let card_x = origin_x + (mx - min_x) as f32 * scale;
                        let card_y = origin_y + (my - min_y) as f32 * scale;
                        let card_w = lb.width as f32 * scale;
                        let card_h = lb.height as f32 * scale;

                        let card_rect =
                            Rectangle::new(Point::new(card_x, card_y), Size::new(card_w, card_h));

                        if card_rect.contains(global_pos) {
                            if m.is_enabled {
                                state.drag_transform =
                                    Some((scale, origin_x, origin_y, min_x, min_y));
                                state.dragging_id = Some(m.id);
                                state.drag_start_cursor = global_pos;
                                state.drag_initial_pos = (mx, my);
                                state.current_pos = (mx, my);
                            }

                            return (event::Status::Captured, Some(Message::SelectMonitor(m.id)));
                        }
                    }

                    // Click on empty space: pan the canvas
                    state.is_panning = true;
                    state.pan_start_cursor = global_pos;
                    state.pan_start_offset = state.pan_offset;
                    return (event::Status::Captured, None);
                }
            }
            Event::Mouse(mouse::Event::CursorMoved { position }) => {
                if state.is_panning {
                    let dx = position.x - state.pan_start_cursor.x;
                    let dy = position.y - state.pan_start_cursor.y;
                    state.pan_offset =
                        (state.pan_start_offset.0 + dx, state.pan_start_offset.1 + dy);
                    return (event::Status::Captured, None);
                }
                if let Some(drag_id) = state.dragging_id {
                    let dx = position.x - state.drag_start_cursor.x;
                    let dy = position.y - state.drag_start_cursor.y;

                    let dx_mon = (dx / scale).round() as i32;
                    let dy_mon = (dy / scale).round() as i32;

                    let mut target_x = state.drag_initial_pos.0 + dx_mon;
                    let mut target_y = state.drag_initial_pos.1 + dy_mon;

                    // Magnetic snapping to other monitors
                    if let Some(curr_m) = self.monitors.iter().find(|m| m.id == drag_id) {
                        let snap_dist = (18.0 / scale) as i32; // 18px visual snap threshold
                        let cur_lb = curr_m.layout_bounds();
                        let cur_w = cur_lb.width as i32;
                        let cur_h = cur_lb.height as i32;

                        for other in self.monitors.iter() {
                            if other.id == drag_id {
                                continue;
                            }
                            let ob = other.layout_bounds();
                            let ox = ob.x;
                            let oy = ob.y;
                            let ow = ob.width as i32;
                            let oh = ob.height as i32;

                            // Horizontal edge snaps
                            if (target_x - (ox + ow)).abs() < snap_dist {
                                target_x = ox + ow; // Snap to right of other
                            } else if ((target_x + cur_w) - ox).abs() < snap_dist {
                                target_x = ox - cur_w; // Snap to left of other
                            } else if (target_x - ox).abs() < snap_dist {
                                target_x = ox; // Snap to left align
                            } else if ((target_x + cur_w) - (ox + ow)).abs() < snap_dist {
                                target_x = ox + ow - cur_w; // Snap to right align
                            }

                            // Vertical edge snaps
                            if (target_y - (oy + oh)).abs() < snap_dist {
                                target_y = oy + oh; // Snap to bottom of other
                            } else if ((target_y + cur_h) - oy).abs() < snap_dist {
                                target_y = oy - cur_h; // Snap to top of other
                            } else if (target_y - oy).abs() < snap_dist {
                                target_y = oy; // Snap to top align
                            } else if ((target_y + cur_h) - (oy + oh)).abs() < snap_dist {
                                target_y = oy + oh - cur_h; // Snap to bottom align
                            }
                        }

                        // Anti-overlap collision constraint: prevent monitors from climbing on top of each other
                        for other in self.monitors.iter() {
                            if other.id == drag_id {
                                continue;
                            }
                            let ob = other.layout_bounds();
                            let ox = ob.x;
                            let oy = ob.y;
                            let ow = ob.width as i32;
                            let oh = ob.height as i32;

                            // Check if boxes intersect
                            let x_overlap = target_x < ox + ow && target_x + cur_w > ox;
                            let y_overlap = target_y < oy + oh && target_y + cur_h > oy;

                            if x_overlap && y_overlap {
                                let push_left = (target_x + cur_w) - ox;
                                let push_right = (ox + ow) - target_x;
                                let push_top = (target_y + cur_h) - oy;
                                let push_bottom = (oy + oh) - target_y;

                                let min_push =
                                    push_left.min(push_right).min(push_top).min(push_bottom);
                                if min_push == push_left {
                                    target_x = ox - cur_w;
                                } else if min_push == push_right {
                                    target_x = ox + ow;
                                } else if min_push == push_top {
                                    target_y = oy - cur_h;
                                } else {
                                    target_y = oy + oh;
                                }
                            }
                        }
                    }

                    state.current_pos = (target_x, target_y);
                    return (event::Status::Captured, None);
                }
            }
            Event::Mouse(mouse::Event::ButtonReleased(button)) => {
                state.is_panning = false;
                if matches!(button, mouse::Button::Right | mouse::Button::Middle) {
                    return (event::Status::Captured, None);
                }
                if let Some(drag_id) = state.dragging_id.take() {
                    let (mut final_x, mut final_y) = state.current_pos;

                    // Final anti-overlap pass on drop
                    if let Some(curr_m) = self.monitors.iter().find(|m| m.id == drag_id) {
                        let cur_lb = curr_m.layout_bounds();
                        let cur_w = cur_lb.width as i32;
                        let cur_h = cur_lb.height as i32;

                        for other in self.monitors.iter() {
                            if other.id == drag_id {
                                continue;
                            }
                            let ob = other.layout_bounds();
                            let ox = ob.x;
                            let oy = ob.y;
                            let ow = ob.width as i32;
                            let oh = ob.height as i32;

                            let x_overlap = final_x < ox + ow && final_x + cur_w > ox;
                            let y_overlap = final_y < oy + oh && final_y + cur_h > oy;

                            if x_overlap && y_overlap {
                                let push_left = (final_x + cur_w) - ox;
                                let push_right = (ox + ow) - final_x;
                                let push_top = (final_y + cur_h) - oy;
                                let push_bottom = (oy + oh) - final_y;

                                let min_push =
                                    push_left.min(push_right).min(push_top).min(push_bottom);
                                if min_push == push_left {
                                    final_x = ox - cur_w;
                                } else if min_push == push_right {
                                    final_x = ox + ow;
                                } else if min_push == push_top {
                                    final_y = oy - cur_h;
                                } else {
                                    final_y = oy + oh;
                                }
                            }
                        }
                    }

                    state.drag_transform = None;
                    return (
                        event::Status::Captured,
                        Some(Message::UpdateMonitorPosition {
                            id: drag_id,
                            x: final_x,
                            y: final_y,
                        }),
                    );
                }
            }
            _ => {}
        }

        (event::Status::Ignored, None)
    }

    fn draw(
        &self,
        state: &Self::State,
        _renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<Geometry> {
        let mut frame = Frame::new(_renderer, bounds.size());
        let (scale, origin_x, origin_y, min_x, min_y) = self.compute_transform(state, bounds);

        // Relative frame offset
        let fx = origin_x - bounds.x;
        let fy = origin_y - bounds.y;

        // 1. Draw Blueprint Background Grid
        let grid_bg = match self.theme_mode {
            ThemeMode::Light => Color::from_rgb(0.965, 0.969, 0.984),
            ThemeMode::Dark => Color::from_rgb(0.08, 0.09, 0.13),
        };
        let grid_dot = match self.theme_mode {
            ThemeMode::Light => Color::from_rgba(0.75, 0.78, 0.88, 0.4),
            ThemeMode::Dark => Color::from_rgba(0.25, 0.28, 0.38, 0.4),
        };

        frame.fill_rectangle(Point::ORIGIN, bounds.size(), grid_bg);

        // Subtle grid dots every 24px
        let step = 24.0;
        let cols = (bounds.width / step) as usize;
        let rows = (bounds.height / step) as usize;
        for c in 0..=cols {
            for r in 0..=rows {
                let dot_p = Point::new(c as f32 * step, r as f32 * step);
                frame.fill_rectangle(dot_p, Size::new(1.5, 1.5), grid_dot);
            }
        }

        // 2. Draw Monitor Cards
        for m in self.monitors {
            let is_dragging = state.dragging_id == Some(m.id);
            let is_active = self.active_id == Some(m.id);
            let is_selected = self.selected_id == Some(m.id);

            let lb = m.layout_bounds();
            let (mx, my) = if is_dragging {
                state.current_pos
            } else {
                (lb.x, lb.y)
            };

            let card_x = fx + (mx - min_x) as f32 * scale;
            let card_y = fy + (my - min_y) as f32 * scale;
            let card_w = lb.width as f32 * scale;
            let card_h = lb.height as f32 * scale;

            // Visual separation gap between monitor tiles
            let gap = 3.0;
            let draw_x = card_x + gap;
            let draw_y = card_y + gap;
            let draw_w = (card_w - gap * 2.0).max(50.0);
            let draw_h = (card_h - gap * 2.0).max(40.0);

            let outer_rect = Rectangle::new(Point::new(draw_x, draw_y), Size::new(draw_w, draw_h));

            // Shadow / elevation when dragging
            if is_dragging {
                let shadow_rect = Rectangle::new(
                    Point::new(draw_x + 4.0, draw_y + 6.0),
                    Size::new(draw_w, draw_h),
                );
                frame.fill(
                    &Path::rounded_rectangle(
                        shadow_rect.position(),
                        shadow_rect.size(),
                        10.0.into(),
                    ),
                    Color::from_rgba(0.0, 0.0, 0.0, 0.22),
                );
            }

            // Outer Monitor Chassis / Frame
            let chassis_bg = match self.theme_mode {
                ThemeMode::Light => {
                    if is_active || is_selected {
                        Color::from_rgb(0.93, 0.94, 0.99)
                    } else {
                        Color::from_rgb(0.94, 0.95, 0.97)
                    }
                }
                ThemeMode::Dark => {
                    if is_active || is_selected {
                        Color::from_rgb(0.15, 0.18, 0.28)
                    } else {
                        Color::from_rgb(0.12, 0.14, 0.19)
                    }
                }
            };

            frame.fill(
                &Path::rounded_rectangle(outer_rect.position(), outer_rect.size(), 10.0.into()),
                chassis_bg,
            );

            // Outer Border
            let border_color = if is_active || is_selected {
                PRIMARY
            } else {
                match self.theme_mode {
                    ThemeMode::Light => Color::from_rgb(0.80, 0.83, 0.90),
                    ThemeMode::Dark => Color::from_rgb(0.24, 0.28, 0.38),
                }
            };
            let border_width = if is_active {
                2.5
            } else if is_selected {
                2.0
            } else {
                1.2
            };

            frame.stroke(
                &Path::rounded_rectangle(outer_rect.position(), outer_rect.size(), 10.0.into()),
                Stroke::default()
                    .with_color(border_color)
                    .with_width(border_width),
            );

            // Inner Display Screen
            let screen_inset = 5.0;
            let screen_x = draw_x + screen_inset;
            let screen_y = draw_y + screen_inset;
            let screen_w = (draw_w - screen_inset * 2.0).max(20.0);
            let screen_h = (draw_h - screen_inset * 2.0).max(20.0);
            let screen_rect = Rectangle::new(
                Point::new(screen_x, screen_y),
                Size::new(screen_w, screen_h),
            );

            let screen_bg = match self.theme_mode {
                ThemeMode::Light => {
                    if is_active {
                        Color::from_rgb(0.97, 0.98, 1.0)
                    } else if is_selected {
                        Color::from_rgb(0.98, 0.98, 1.0)
                    } else {
                        Color::WHITE
                    }
                }
                ThemeMode::Dark => {
                    if is_active {
                        Color::from_rgb(0.09, 0.11, 0.18)
                    } else if is_selected {
                        Color::from_rgb(0.08, 0.10, 0.16)
                    } else {
                        Color::from_rgb(0.06, 0.07, 0.11)
                    }
                }
            };

            frame.fill(
                &Path::rounded_rectangle(screen_rect.position(), screen_rect.size(), 6.0.into()),
                screen_bg,
            );

            // Subtle inner screen stroke
            let inner_stroke = match self.theme_mode {
                ThemeMode::Light => Color::from_rgba(0.0, 0.0, 0.0, 0.05),
                ThemeMode::Dark => Color::from_rgba(1.0, 1.0, 1.0, 0.06),
            };
            frame.stroke(
                &Path::rounded_rectangle(screen_rect.position(), screen_rect.size(), 6.0.into()),
                Stroke::default().with_color(inner_stroke).with_width(1.0),
            );

            let text_color = self.theme_mode.text_primary();
            let muted_color = self.theme_mode.text_muted();

            // 1. TOP HEADER IN TILE: Role badge & Active indicator
            let (role_text, badge_bg, badge_fg) = if !m.is_enabled {
                (
                    if i18n::current() == Language::English {
                        "OFFLINE"
                    } else {
                        "ОТКЛЮЧЁН"
                    },
                    Color::from_rgba(0.94, 0.27, 0.27, 0.14),
                    Color::from_rgb(0.94, 0.27, 0.27),
                )
            } else if m.is_virtual {
                (
                    if i18n::current() == Language::English {
                        "VIRTUAL"
                    } else {
                        "ВИРТУАЛЬНЫЙ"
                    },
                    Color::from_rgba(0.55, 0.35, 0.95, 0.15),
                    Color::from_rgb(0.55, 0.35, 0.95),
                )
            } else if m.id == 1 || m.name == "MAIN" {
                (
                    if i18n::current() == Language::English {
                        "PRIMARY"
                    } else {
                        "ОСНОВНОЙ"
                    },
                    Color::from_rgba(0.31, 0.27, 0.90, 0.14),
                    PRIMARY,
                )
            } else {
                (
                    if i18n::current() == Language::English {
                        "PHYSICAL"
                    } else {
                        "ФИЗИЧЕСКИЙ"
                    },
                    Color::from_rgba(0.5, 0.5, 0.6, 0.12),
                    muted_color,
                )
            };

            let badge_w = match role_text {
                "ВИРТУАЛЬНЫЙ" | "VIRTUAL" => 78.0,
                "ОТКЛЮЧЁН" | "OFFLINE" => 68.0,
                "ОСНОВНОЙ" | "PRIMARY" => 60.0,
                _ => 68.0,
            };

            let badge_rect = Rectangle::new(
                Point::new(screen_x + 8.0, screen_y + 6.0),
                Size::new(badge_w, 15.0),
            );
            frame.fill(
                &Path::rounded_rectangle(badge_rect.position(), badge_rect.size(), 4.0.into()),
                badge_bg,
            );

            frame.fill_text(Text {
                content: role_text.to_string(),
                position: Point::new(screen_x + 8.0 + badge_w / 2.0, screen_y + 6.0 + 7.5),
                color: badge_fg,
                size: (9.5).into(),
                font: ui_font_with_weight(Weight::Semibold),
                horizontal_alignment: iced::alignment::Horizontal::Center,
                vertical_alignment: iced::alignment::Vertical::Center,
                ..Default::default()
            });

            if is_active {
                // Glowing green indicator
                let dot_center = Point::new(screen_x + screen_w - 12.0, screen_y + 11.0);
                frame.fill(
                    &Path::circle(dot_center, 4.0),
                    Color::from_rgb(0.13, 0.77, 0.37),
                );
            }

            // 2. LARGE PROMINENT DISPLAY NUMBER IN THE CENTER
            let center_x = screen_x + screen_w / 2.0;
            let center_y = screen_y + screen_h / 2.0;
            let num_size = (screen_h * 0.40).clamp(24.0, 48.0);

            // Draw subtle background circular pill for the number
            let pill_radius = (num_size * 0.75).min(screen_h * 0.38);
            let pill_bg = match self.theme_mode {
                ThemeMode::Light => {
                    if is_active || is_selected {
                        Color::from_rgba(0.38, 0.31, 0.86, 0.12)
                    } else {
                        Color::from_rgba(0.0, 0.0, 0.0, 0.04)
                    }
                }
                ThemeMode::Dark => {
                    if is_active || is_selected {
                        Color::from_rgba(0.48, 0.41, 0.96, 0.20)
                    } else {
                        Color::from_rgba(1.0, 1.0, 1.0, 0.06)
                    }
                }
            };
            frame.fill(
                &Path::circle(Point::new(center_x, center_y), pill_radius),
                pill_bg,
            );

            let num_color = if is_active || is_selected {
                PRIMARY
            } else {
                text_color
            };

            frame.fill_text(Text {
                content: format!("{}", m.id),
                position: Point::new(center_x, center_y),
                color: num_color,
                size: num_size.into(),
                font: ui_font_with_weight(Weight::Semibold),
                horizontal_alignment: iced::alignment::Horizontal::Center,
                vertical_alignment: iced::alignment::Vertical::Center,
                ..Default::default()
            });

            // 3. BOTTOM FOOTER IN TILE: Resolution & Refresh rate
            let res_text = format!(
                "{}×{}  •  {}Hz",
                m.bounds.width, m.bounds.height, m.refresh_rate
            );
            frame.fill_text(Text {
                content: res_text,
                position: Point::new(center_x, screen_y + screen_h - 10.0),
                color: muted_color,
                size: (11.5).into(),
                font: ui_font_with_weight(Weight::Medium),
                horizontal_alignment: iced::alignment::Horizontal::Center,
                vertical_alignment: iced::alignment::Vertical::Bottom,
                ..Default::default()
            });
        }

        // 6. Draw Zoom / Pan status hint in bottom-left
        let zoom_pct = (state.user_zoom * 100.0).round() as u32;
        let zoom_hint = if i18n::current() == Language::English {
            format!(
                "Zoom: {}%  •  Wheel: zoom  •  Drag background: pan",
                zoom_pct
            )
        } else {
            format!(
                "Зум: {}%  •  Колесико: зум  •  Зажмите фон: перемещение",
                zoom_pct
            )
        };
        frame.fill_text(Text {
            content: zoom_hint,
            position: Point::new(12.0, bounds.height - 10.0),
            color: match self.theme_mode {
                ThemeMode::Light => Color::from_rgba(0.45, 0.48, 0.58, 0.75),
                ThemeMode::Dark => Color::from_rgba(0.55, 0.60, 0.72, 0.75),
            },
            size: (12.0).into(),
            font: ui_font(),
            vertical_alignment: iced::alignment::Vertical::Bottom,
            ..Default::default()
        });

        vec![frame.into_geometry()]
    }

    fn mouse_interaction(
        &self,
        state: &Self::State,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> mouse::Interaction {
        if state.dragging_id.is_some() {
            return mouse::Interaction::Move;
        }

        if state.is_panning {
            return mouse::Interaction::Grabbing;
        }

        if let Some(cursor_pos) = cursor.position_in(bounds) {
            let (scale, origin_x, origin_y, min_x, min_y) = self.compute_transform(state, bounds);
            let global_x = bounds.x + cursor_pos.x;
            let global_y = bounds.y + cursor_pos.y;

            for m in self.monitors {
                let lb = m.layout_bounds();
                let mx = lb.x;
                let my = lb.y;
                let card_x = origin_x + (mx - min_x) as f32 * scale;
                let card_y = origin_y + (my - min_y) as f32 * scale;
                let card_w = lb.width as f32 * scale;
                let card_h = lb.height as f32 * scale;

                let card_rect =
                    Rectangle::new(Point::new(card_x, card_y), Size::new(card_w, card_h));
                if card_rect.contains(Point::new(global_x, global_y)) {
                    // Only hover over a tile returns the 4-way move cross cursor
                    return if m.is_enabled {
                        mouse::Interaction::Move
                    } else {
                        mouse::Interaction::Pointer
                    };
                }
            }

            // Hover over empty space on canvas returns Grab cursor
            return mouse::Interaction::Grab;
        }

        // Empty canvas background keeps default normal arrow cursor
        mouse::Interaction::Idle
    }
}
