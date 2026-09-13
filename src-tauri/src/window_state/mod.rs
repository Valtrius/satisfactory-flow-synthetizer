//! Keep normal window bounds independently of minimized/maximized/fullscreen bounds.
mod geometry;

use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    sync::Mutex,
};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, LogicalSize, Manager, Monitor, WebviewWindow, WindowEvent};

use geometry::{Display, Placement, clamp_position, restore};

const FILE_NAME: &str = "window-state.json";

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
struct SavedState {
    version: u8,
    normal: Placement,
    maximized: bool,
}

enum Sample {
    Normal(Placement),
    Maximized(Display),
    Ignore,
}

struct Tracker {
    path: Option<PathBuf>,
    saved: Option<SavedState>,
    displays: Vec<Display>,
    minimum: LogicalSize<f64>,
    recovery_pending: bool,
}

impl Tracker {
    fn record(&mut self, sample: Sample) {
        match sample {
            Sample::Normal(normal) if normal.valid() && !self.recovery_pending => {
                self.saved = Some(SavedState {
                    version: 1,
                    normal,
                    maximized: false,
                });
            }
            Sample::Maximized(display) if display.valid() => {
                if let Some(saved) = &mut self.saved {
                    saved.maximized = true;
                    // Win+Shift+Arrow can move a maximized window without ever
                    // producing normal bounds on its destination monitor.
                    saved.normal = saved.normal.relocated(display);
                }
            }
            Sample::Normal(_) | Sample::Maximized(_) | Sample::Ignore => {}
        }
    }

    fn update_displays(&mut self, displays: Vec<Display>) {
        // A display change while minimized/maximized must also repair the normal
        // bounds when the user restores the window later.
        if !displays.is_empty() && displays != self.displays {
            self.displays = displays;
            self.recovery_pending = true;
        }
    }

    fn save(&self) {
        if let (Some(path), Some(saved)) = (&self.path, &self.saved)
            && let Err(error) = write_state(path, saved)
        {
            eprintln!("Cannot save desktop window state: {error}");
        }
    }
}

fn read_state(path: &Path) -> Option<SavedState> {
    let saved: SavedState = serde_json::from_slice(&fs::read(path).ok()?).ok()?;
    (saved.version == 1 && saved.normal.valid()).then_some(saved)
}

fn write_state(path: &Path, saved: &SavedState) -> Result<(), Box<dyn std::error::Error>> {
    let parent = path.parent().ok_or("window state path has no parent")?;
    fs::create_dir_all(parent)?;
    let mut file = tempfile::NamedTempFile::new_in(parent)?;
    file.write_all(&serde_json::to_vec(saved)?)?;
    file.as_file().sync_all()?;
    file.persist(path)?;
    Ok(())
}

fn display(monitor: &Monitor) -> Display {
    Display {
        name: monitor.name().cloned(),
        position: monitor.work_area().position,
        size: monitor.work_area().size,
        scale: monitor.scale_factor(),
    }
}

fn displays(window: &WebviewWindow) -> tauri::Result<Vec<Display>> {
    let primary = window.primary_monitor()?.map(|monitor| display(&monitor));
    let mut monitors: Vec<_> = window
        .available_monitors()?
        .iter()
        .map(display)
        .filter(Display::valid)
        .collect();
    monitors.sort_by_key(|monitor| {
        (
            Some(monitor) != primary.as_ref(),
            monitor.position.x,
            monitor.position.y,
            monitor.name.clone(),
        )
    });
    Ok(monitors)
}

fn sample(window: &WebviewWindow) -> tauri::Result<Sample> {
    // Windows can report sentinel positions and zero sizes when minimized.
    // Fullscreen and maximized rectangles must never replace the normal bounds.
    if window.is_minimized()? || window.is_fullscreen()? {
        return Ok(Sample::Ignore);
    }
    let Some(monitor) = window.current_monitor()? else {
        return Ok(Sample::Ignore);
    };
    if window.is_maximized()? {
        return Ok(Sample::Maximized(display(&monitor)));
    }
    Ok(Sample::Normal(Placement {
        position: window.outer_position()?,
        size: window.inner_size()?.to_logical(window.scale_factor()?),
        display: display(&monitor),
    }))
}

fn frame_size(window: &WebviewWindow) -> tauri::Result<LogicalSize<f64>> {
    let outer = window.outer_size()?;
    let inner = window.inner_size()?;
    Ok(tauri::PhysicalSize::new(
        outer.width.saturating_sub(inner.width),
        outer.height.saturating_sub(inner.height),
    )
    .to_logical(window.scale_factor()?))
}

fn apply(window: &WebviewWindow, saved: &Placement, tracker: &Tracker) -> tauri::Result<()> {
    let Some(plan) = restore(
        saved,
        &tracker.displays,
        frame_size(window)?,
        tracker.minimum,
    ) else {
        return Ok(());
    };
    window.set_min_size(Some(plan.minimum))?;
    // Move first so Windows has selected the target monitor's DPI before resizing.
    window.set_position(plan.placement.position)?;
    let Some(plan) = restore(
        saved,
        &tracker.displays,
        frame_size(window)?,
        tracker.minimum,
    ) else {
        return Ok(());
    };
    window.set_min_size(Some(plan.minimum))?;
    window.set_size(
        plan.placement
            .size
            .to_physical::<u32>(plan.placement.display.scale),
    )?;
    window.set_position(clamp_position(
        plan.placement.position,
        window.outer_size()?,
        &plan.placement.display,
    ))?;
    Ok(())
}

pub(crate) fn init(window: &WebviewWindow) {
    let path = window
        .app_handle()
        .path()
        .app_config_dir()
        .ok()
        .map(|dir| dir.join(FILE_NAME));
    let saved = path.as_deref().and_then(read_state);
    let config = window
        .app_handle()
        .config()
        .app
        .windows
        .iter()
        .find(|config| config.label == window.label());
    let mut tracker = Tracker {
        path,
        saved: None,
        displays: displays(window).unwrap_or_default(),
        minimum: LogicalSize::new(
            config.and_then(|config| config.min_width).unwrap_or(1.0),
            config.and_then(|config| config.min_height).unwrap_or(1.0),
        ),
        recovery_pending: false,
    };
    if let Ok(sample) = sample(window) {
        tracker.record(sample);
    }
    if let Some(saved) = saved.or_else(|| tracker.saved.clone()) {
        if let Err(error) = apply(window, &saved.normal, &tracker) {
            eprintln!("Cannot restore desktop window bounds: {error}");
            let _ = window.center();
        }
        // Capture corrected normal bounds before maximizing, including on first launch.
        if let Ok(sample) = sample(window) {
            tracker.record(sample);
        }
        if saved.maximized
            && window.maximize().is_ok()
            && let Some(saved) = &mut tracker.saved
        {
            saved.maximized = true;
        }
    }
    window.app_handle().manage(Mutex::new(tracker));
    let tracked_window = window.clone();
    window.on_window_event(move |event| on_event(&tracked_window, event));
}

fn on_event(window: &WebviewWindow, event: &WindowEvent) {
    if !matches!(
        event,
        WindowEvent::Moved(_)
            | WindowEvent::Resized(_)
            | WindowEvent::ScaleFactorChanged { .. }
            | WindowEvent::Focused(_)
            | WindowEvent::CloseRequested { .. }
            | WindowEvent::Destroyed
    ) {
        return;
    }
    let state = window.app_handle().state::<Mutex<Tracker>>();
    // Applying bounds may synchronously emit more window events.
    let Ok(mut tracker) = state.try_lock() else {
        return;
    };
    if !matches!(event, WindowEvent::Destroyed) {
        if let Ok(displays) = displays(window) {
            tracker.update_displays(displays);
        }
        if tracker.recovery_pending
            && let Ok(Sample::Normal(current)) = sample(window)
        {
            let normal = tracker
                .saved
                .as_ref()
                .map_or(&current, |saved| &saved.normal);
            match apply(window, normal, &tracker) {
                Ok(()) => tracker.recovery_pending = false,
                Err(error) => {
                    eprintln!("Cannot recover desktop window after display change: {error}");
                }
            }
        }
        if let Ok(sample) = sample(window) {
            tracker.record(sample);
        }
    }
    if matches!(
        event,
        WindowEvent::CloseRequested { .. } | WindowEvent::Destroyed
    ) {
        tracker.save();
    }
}

pub(crate) fn save(app: &AppHandle) {
    if let Some(state) = app.try_state::<Mutex<Tracker>>()
        && let Ok(tracker) = state.lock()
    {
        tracker.save();
    }
}

#[cfg(test)]
mod tests;
