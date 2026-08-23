use std::sync::Mutex;
use std::time::{Duration, Instant};

use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, WindowEvent};

const WIDGET_LABEL: &str = "widget";
const WIDGET_LOGICAL_WIDTH: f64 = 448.0;
const WIDGET_LOGICAL_HEIGHT: f64 = 880.0;
const WIDGET_LOGICAL_MARGIN: f64 = 12.0;
const TOGGLE_DEBOUNCE: Duration = Duration::from_millis(180);
const FOCUS_GUARD: Duration = Duration::from_millis(280);

#[derive(Debug, Default)]
struct WidgetRuntime {
    last_toggle_at: Option<Instant>,
    suppress_focus_loss_until: Option<Instant>,
}

static WIDGET_RUNTIME: Mutex<WidgetRuntime> = Mutex::new(WidgetRuntime {
    last_toggle_at: None,
    suppress_focus_loss_until: None,
});

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct WorkArea {
    x: i32,
    y: i32,
    width: u32,
    height: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct WidgetGeometry {
    position: PhysicalPosition<i32>,
    size: PhysicalSize<u32>,
}

pub fn prepare_windows(app: &AppHandle) {
    if let Some(widget) = app.get_webview_window(WIDGET_LABEL) {
        #[cfg(target_os = "windows")]
        {
            // Acrylic is best-effort: systems with transparency disabled retain the
            // opaque CSS fallback instead of failing app startup.
            let _ = window_vibrancy::apply_acrylic(&widget, Some((18, 14, 28, 196)));
        }

        let app_for_events = app.clone();
        widget.on_window_event(move |event| match event {
            WindowEvent::Focused(false) => hide_after_focus_loss(&app_for_events),
            WindowEvent::CloseRequested { api, .. } => {
                api.prevent_close();
                hide_widget(&app_for_events);
            }
            _ => {}
        });
    }

    if let Some(settings) = app.get_webview_window("main") {
        let settings_for_events = settings.clone();
        settings.on_window_event(move |event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = settings_for_events.hide();
            }
        });
    }
}

pub fn toggle_widget(app: &AppHandle, click_position: PhysicalPosition<f64>) {
    let now = Instant::now();
    {
        let Ok(mut runtime) = WIDGET_RUNTIME.lock() else {
            return;
        };
        if runtime
            .last_toggle_at
            .is_some_and(|last_toggle| now.duration_since(last_toggle) < TOGGLE_DEBOUNCE)
        {
            return;
        }
        runtime.last_toggle_at = Some(now);
    }

    let Some(window) = app.get_webview_window(WIDGET_LABEL) else {
        return;
    };
    if window.is_visible().unwrap_or(false) {
        hide_widget(app);
    } else {
        show_widget(app, Some(click_position));
    }
}

pub fn show_widget(app: &AppHandle, click_position: Option<PhysicalPosition<f64>>) {
    let Some(window) = app.get_webview_window(WIDGET_LABEL) else {
        return;
    };

    let monitor = click_position
        .and_then(|position| {
            app.monitor_from_point(position.x, position.y)
                .ok()
                .flatten()
        })
        .or_else(|| app.primary_monitor().ok().flatten());
    let Some(monitor) = monitor else {
        return;
    };
    let work_area = monitor.work_area();
    let geometry = calculate_widget_geometry(
        WorkArea {
            x: work_area.position.x,
            y: work_area.position.y,
            width: work_area.size.width,
            height: work_area.size.height,
        },
        monitor.scale_factor(),
    );

    let focus_guard_until = Instant::now() + FOCUS_GUARD;
    if let Ok(mut runtime) = WIDGET_RUNTIME.lock() {
        runtime.suppress_focus_loss_until = Some(focus_guard_until);
    }

    let _ = window.set_size(geometry.size);
    let _ = window.set_position(geometry.position);
    let _ = window.set_skip_taskbar(true);
    let _ = window.set_always_on_top(true);
    if window.show().is_err() {
        if let Ok(mut runtime) = WIDGET_RUNTIME.lock() {
            runtime.suppress_focus_loss_until = None;
        }
        let _ = window.set_always_on_top(false);
        return;
    }
    let _ = window.set_focus();
    schedule_focus_guard_check(app.clone(), focus_guard_until);
    let _ = app.emit_to(WIDGET_LABEL, "widget-shown", ());

    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let state = app
            .state::<crate::app_state::SharedAppState>()
            .inner()
            .clone();
        let _ = crate::dashboard::emit_dashboard_state(&app, &state).await;
    });
}

pub fn hide_widget(app: &AppHandle) {
    if let Ok(mut runtime) = WIDGET_RUNTIME.lock() {
        runtime.suppress_focus_loss_until = None;
    }
    if let Some(window) = app.get_webview_window(WIDGET_LABEL) {
        let _ = window.hide();
        let _ = window.set_always_on_top(false);
    }
}

fn hide_after_focus_loss(app: &AppHandle) {
    let should_hide = WIDGET_RUNTIME
        .lock()
        .map(|runtime| {
            !runtime
                .suppress_focus_loss_until
                .is_some_and(|until| Instant::now() < until)
        })
        .unwrap_or(true);
    if should_hide {
        hide_widget(app);
    }
}

fn schedule_focus_guard_check(app: AppHandle, guarded_until: Instant) {
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(guarded_until.saturating_duration_since(Instant::now())).await;
        let guard_is_current = WIDGET_RUNTIME
            .lock()
            .map(|mut runtime| {
                if runtime.suppress_focus_loss_until == Some(guarded_until) {
                    runtime.suppress_focus_loss_until = None;
                    true
                } else {
                    false
                }
            })
            .unwrap_or(false);
        if !guard_is_current {
            return;
        }

        let Some(window) = app.get_webview_window(WIDGET_LABEL) else {
            return;
        };
        if window.is_visible().unwrap_or(false) && !window.is_focused().unwrap_or(false) {
            hide_widget(&app);
        }
    });
}

fn calculate_widget_geometry(work_area: WorkArea, scale_factor: f64) -> WidgetGeometry {
    let scale_factor = if scale_factor.is_finite() && scale_factor > 0.0 {
        scale_factor
    } else {
        1.0
    };
    let margin = (WIDGET_LOGICAL_MARGIN * scale_factor).round() as u32;
    let horizontal_margin = margin.saturating_mul(2);
    let vertical_margin = margin.saturating_mul(2);
    let available_width = work_area.width.saturating_sub(horizontal_margin).max(1);
    let available_height = work_area.height.saturating_sub(vertical_margin).max(1);
    let width = ((WIDGET_LOGICAL_WIDTH * scale_factor).round() as u32).min(available_width);
    let height = ((WIDGET_LOGICAL_HEIGHT * scale_factor).round() as u32).min(available_height);
    let x =
        i64::from(work_area.x) + i64::from(work_area.width) - i64::from(width) - i64::from(margin);
    let y = i64::from(work_area.y) + i64::from(margin);

    WidgetGeometry {
        position: PhysicalPosition::new(clamp_i64_to_i32(x), clamp_i64_to_i32(y)),
        size: PhysicalSize::new(width, height),
    }
}

fn clamp_i64_to_i32(value: i64) -> i32 {
    value.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn anchors_to_the_right_edge_of_the_work_area() {
        let geometry = calculate_widget_geometry(
            WorkArea {
                x: 0,
                y: 0,
                width: 1920,
                height: 1040,
            },
            1.0,
        );

        assert_eq!(geometry.position, PhysicalPosition::new(1460, 12));
        assert_eq!(geometry.size, PhysicalSize::new(448, 880));
    }

    #[test]
    fn supports_negative_coordinates_and_high_dpi_monitors() {
        let geometry = calculate_widget_geometry(
            WorkArea {
                x: -2560,
                y: 40,
                width: 2560,
                height: 1400,
            },
            1.5,
        );

        assert_eq!(geometry.position, PhysicalPosition::new(-690, 58));
        assert_eq!(geometry.size, PhysicalSize::new(672, 1320));
    }

    #[test]
    fn constrains_height_to_small_work_areas_without_overlapping_the_taskbar() {
        let geometry = calculate_widget_geometry(
            WorkArea {
                x: 0,
                y: 0,
                width: 1280,
                height: 680,
            },
            1.25,
        );

        assert_eq!(geometry.position, PhysicalPosition::new(705, 15));
        assert_eq!(geometry.size, PhysicalSize::new(560, 650));
    }
}
