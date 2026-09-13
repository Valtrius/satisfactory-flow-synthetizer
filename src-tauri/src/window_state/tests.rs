use super::*;
use tauri::{PhysicalPosition, PhysicalSize};

fn monitor(name: &str, x: i32, y: i32, width: u32, height: u32, scale: f64) -> Display {
    Display {
        name: Some(name.into()),
        position: PhysicalPosition::new(x, y),
        size: PhysicalSize::new(width, height),
        scale,
    }
}

fn primary() -> Display {
    monitor("primary", 0, 0, 1920, 1040, 1.0)
}

fn placement(display: Display, x: i32, y: i32) -> Placement {
    Placement {
        position: PhysicalPosition::new(x, y),
        size: LogicalSize::new(1000.0, 700.0),
        display,
    }
}

fn plan(saved: &Placement, displays: &[Display]) -> geometry::Restore {
    restore(
        saved,
        displays,
        LogicalSize::new(16.0, 40.0),
        LogicalSize::new(720.0, 560.0),
    )
    .unwrap()
}

fn assert_fits(placement: &Placement) {
    let display = &placement.display;
    let outer = LogicalSize::new(placement.size.width + 16.0, placement.size.height + 40.0)
        .to_physical::<u32>(display.scale);
    assert!(placement.position.x >= display.position.x);
    assert!(placement.position.y >= display.position.y);
    assert!(
        i64::from(placement.position.x) + i64::from(outer.width)
            <= i64::from(display.position.x) + i64::from(display.size.width)
    );
    assert!(
        i64::from(placement.position.y) + i64::from(outer.height)
            <= i64::from(display.position.y) + i64::from(display.size.height)
    );
}

#[test]
fn missing_monitor_centers_on_primary() {
    let saved = placement(monitor("removed", 1920, 0, 2560, 1400, 1.0), 2200, 100);
    let restored = plan(&saved, &[primary()]).placement;
    assert_eq!(restored.position, PhysicalPosition::new(452, 150));
    assert_eq!(restored.size, saved.size);
    assert_fits(&restored);
}

#[test]
fn negative_coordinates_are_retained_on_left_and_upper_displays() {
    for (x, y) in [(-1920, 0), (0, -1200)] {
        let display = monitor("secondary", x, y, 1920, 1160, 1.0);
        let saved = placement(display.clone(), x + 100, y + 100);
        let restored = plan(&saved, &[primary(), display]).placement;
        assert_eq!(restored, saved);
        assert_fits(&restored);
    }
}

#[test]
fn rearranged_monitor_retains_relative_position() {
    let saved = placement(monitor("secondary", 1920, 0, 1920, 1040, 1.0), 2200, 100);
    let moved = monitor("secondary", -1920, -200, 1920, 1040, 1.0);
    let restored = plan(&saved, &[primary(), moved]).placement;
    assert_eq!(restored.position, PhysicalPosition::new(-1640, -100));
    assert_fits(&restored);
}

#[test]
fn changed_dpi_preserves_logical_size_and_offset() {
    let saved = placement(primary(), 200, 100);
    let scaled = monitor("primary", 0, 0, 3840, 2080, 2.0);
    let restored = plan(&saved, &[scaled]).placement;
    assert_eq!(restored.position, PhysicalPosition::new(400, 200));
    assert_eq!(restored.size, saved.size);
    assert_fits(&restored);
}

#[test]
fn small_high_dpi_work_area_overrides_configured_minimum() {
    let saved = placement(primary(), 200, 100);
    let restored = plan(&saved, &[monitor("primary", 0, 0, 1000, 700, 2.0)]);
    assert_eq!(restored.minimum, LogicalSize::new(484.0, 310.0));
    assert_eq!(restored.placement.size, restored.minimum);
    assert_fits(&restored.placement);
}

#[test]
fn partial_title_bar_and_taskbar_overlap_are_corrected() {
    let saved = placement(primary(), 1800, -600);
    let restored = plan(&saved, &[monitor("primary", 50, 60, 1870, 980, 1.0)]).placement;
    assert_eq!(restored.position, PhysicalPosition::new(904, 60));
    assert_fits(&restored);
}

#[test]
fn fallback_uses_a_remaining_display_under_the_old_window() {
    let saved = placement(monitor("removed", -1920, 0, 1920, 1040, 1.0), -100, 100);
    let replacement = monitor("replacement", -1920, 0, 1920, 1040, 1.0);
    let restored = plan(&saved, &[primary(), replacement]).placement;
    assert_eq!(restored.display, primary());
    assert_eq!(restored.position, PhysicalPosition::new(0, 100));
    assert_fits(&restored);
}

#[test]
fn no_monitors_and_invalid_saved_geometry_skip_restoration() {
    let saved = placement(primary(), 100, 100);
    assert!(restore(&saved, &[], LogicalSize::default(), LogicalSize::default()).is_none());
    for invalid in [0.0, f64::NAN, f64::INFINITY, -100.0] {
        let mut saved = saved.clone();
        saved.size.width = invalid;
        assert!(
            restore(
                &saved,
                &[primary()],
                LogicalSize::default(),
                LogicalSize::default()
            )
            .is_none()
        );
    }
}

#[test]
fn extreme_coordinates_recover_without_integer_overflow() {
    for position in [i32::MIN, i32::MAX] {
        let saved = placement(primary(), position, position);
        assert_fits(&plan(&saved, &[primary()]).placement);
    }
}

fn tracker() -> Tracker {
    Tracker {
        path: None,
        saved: None,
        displays: vec![primary()],
        minimum: LogicalSize::new(720.0, 560.0),
        recovery_pending: false,
    }
}

#[test]
fn maximized_and_minimized_samples_preserve_normal_restore_bounds() {
    let normal = placement(primary(), 100, 100);
    let mut tracker = tracker();
    tracker.record(Sample::Normal(normal.clone()));
    tracker.record(Sample::Maximized(primary()));
    tracker.record(Sample::Ignore); // Minimized or fullscreen, including sentinel coordinates.
    let saved = tracker.saved.as_ref().unwrap();
    assert!(saved.maximized);
    assert_eq!(saved.normal, normal);

    let resized = placement(primary(), 300, 200);
    tracker.record(Sample::Normal(resized.clone()));
    let saved = tracker.saved.as_ref().unwrap();
    assert!(!saved.maximized);
    assert_eq!(saved.normal, resized);
}

#[test]
fn display_changes_defer_recovery_until_normal_bounds_can_be_applied() {
    let normal = placement(primary(), 100, 100);
    let mut tracker = tracker();
    tracker.record(Sample::Normal(normal.clone()));
    tracker.record(Sample::Maximized(primary()));
    tracker.update_displays(vec![monitor("laptop", 0, 0, 1366, 728, 1.0)]);
    tracker.record(Sample::Ignore);
    assert!(tracker.recovery_pending);
    assert!(tracker.saved.as_ref().unwrap().maximized);

    // A transient empty monitor list and stale OS restore bounds must not erase
    // the usable normal bounds before recovery succeeds.
    tracker.update_displays(vec![]);
    tracker.record(Sample::Normal(placement(primary(), 2200, 100)));
    assert_eq!(tracker.saved.as_ref().unwrap().normal, normal);
    let corrected = plan(&normal, &tracker.displays).placement;
    tracker.recovery_pending = false;
    tracker.record(Sample::Normal(corrected));
    assert!(!tracker.saved.as_ref().unwrap().maximized);
    assert_fits(&tracker.saved.as_ref().unwrap().normal);
}

#[test]
fn moving_a_maximized_window_remembers_the_destination_and_normal_size() {
    let normal = placement(primary(), 100, 100);
    let secondary = monitor("secondary", -3840, 0, 3840, 2080, 2.0);
    let mut tracker = tracker();
    tracker.record(Sample::Normal(normal.clone()));
    tracker.record(Sample::Maximized(secondary.clone()));
    tracker.record(Sample::Ignore);
    let saved = tracker.saved.as_ref().unwrap();
    assert!(saved.maximized);
    assert_eq!(saved.normal.display, secondary);
    assert_eq!(saved.normal.size, normal.size);
    assert_eq!(saved.normal.position, PhysicalPosition::new(-3640, 200));
    assert_fits(&plan(&saved.normal, &[primary(), secondary]).placement);
}

#[test]
fn state_round_trips_and_atomically_replaces_the_previous_file() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("settings").join(FILE_NAME);
    assert!(read_state(&path).is_none());
    let mut saved = SavedState {
        version: 1,
        normal: placement(primary(), 100, 100),
        maximized: false,
    };
    write_state(&path, &saved).unwrap();
    assert_eq!(read_state(&path), Some(saved.clone()));
    saved.maximized = true;
    write_state(&path, &saved).unwrap();
    assert_eq!(read_state(&path), Some(saved));
    assert_eq!(fs::read_dir(path.parent().unwrap()).unwrap().count(), 1);
}

#[test]
fn corrupt_and_unsupported_state_files_fall_back_to_defaults() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join(FILE_NAME);
    fs::write(&path, b"{truncated").unwrap();
    assert!(read_state(&path).is_none());
    let mut saved = SavedState {
        version: 2,
        normal: placement(primary(), 100, 100),
        maximized: false,
    };
    write_state(&path, &saved).unwrap();
    assert!(read_state(&path).is_none());
    saved.version = 1;
    saved.normal.display.scale = 0.0;
    write_state(&path, &saved).unwrap();
    assert!(read_state(&path).is_none());
}
