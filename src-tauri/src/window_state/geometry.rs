use serde::{Deserialize, Serialize};
use tauri::{LogicalSize, PhysicalPosition, PhysicalSize};

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub(super) struct Display {
    pub name: Option<String>,
    pub position: PhysicalPosition<i32>,
    pub size: PhysicalSize<u32>,
    pub scale: f64,
}

impl Display {
    pub fn valid(&self) -> bool {
        self.size.width > 0
            && self.size.height > 0
            && self.scale.is_finite()
            && self.scale > 0.0
            && self.scale <= 64.0
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub(super) struct Placement {
    pub position: PhysicalPosition<i32>,
    // Client size and the offset within a monitor are restored at its current DPI.
    pub size: LogicalSize<f64>,
    pub display: Display,
}

impl Placement {
    pub fn relocated(&self, display: Display) -> Self {
        Self {
            position: PhysicalPosition::new(
                f64::from(display.position.x)
                    + (f64::from(self.position.x) - f64::from(self.display.position.x))
                        / self.display.scale
                        * display.scale,
                f64::from(display.position.y)
                    + (f64::from(self.position.y) - f64::from(self.display.position.y))
                        / self.display.scale
                        * display.scale,
            )
            .cast::<i32>(),
            size: self.size,
            display,
        }
    }

    pub fn valid(&self) -> bool {
        self.display.valid()
            && [self.size.width, self.size.height]
                .into_iter()
                .all(|value| value.is_finite() && (1.0..=1_000_000.0).contains(&value))
    }
}

pub(super) struct Restore {
    pub placement: Placement,
    pub minimum: LogicalSize<f64>,
}

/// Displays are ordered with the primary first. A removed monitor falls back to
/// the display under the old window, or to the primary if it is entirely offscreen.
pub(super) fn restore(
    saved: &Placement,
    displays: &[Display],
    frame: LogicalSize<f64>,
    minimum: LogicalSize<f64>,
) -> Option<Restore> {
    if !saved.valid() {
        return None;
    }
    let display = displays
        .iter()
        // max_by_key keeps the last tie; reverse to prefer the primary.
        .rev()
        .filter(|display| display.valid())
        .max_by_key(|display| {
            (
                same_display(&saved.display, display),
                overlap(
                    saved.position,
                    saved.size.to_physical(saved.display.scale),
                    display,
                ),
            )
        })?
        .clone();

    let available = LogicalSize::new(
        (f64::from(display.size.width) / display.scale - frame.width).max(1.0),
        (f64::from(display.size.height) / display.scale - frame.height).max(1.0),
    );
    // Even the configured minimum must fit a small display or high DPI desktop.
    let minimum = LogicalSize::new(
        minimum.width.min(available.width),
        minimum.height.min(available.height),
    );
    let size = LogicalSize::new(
        saved.size.width.clamp(minimum.width, available.width),
        saved.size.height.clamp(minimum.height, available.height),
    );
    let outer = LogicalSize::new(size.width + frame.width, size.height + frame.height)
        .to_physical::<u32>(display.scale);
    let position = if same_display(&saved.display, &display) {
        saved.relocated(display.clone()).position
    } else if overlap(saved.position, outer, &display) > 0 {
        saved.position
    } else {
        PhysicalPosition::new(
            f64::from(display.position.x)
                + f64::from(display.size.width.saturating_sub(outer.width)) / 2.0,
            f64::from(display.position.y)
                + f64::from(display.size.height.saturating_sub(outer.height)) / 2.0,
        )
        .cast::<i32>()
    };
    Some(Restore {
        placement: Placement {
            position: clamp_position(position, outer, &display),
            size,
            display,
        },
        minimum,
    })
}

fn same_display(saved: &Display, current: &Display) -> bool {
    match (&saved.name, &current.name) {
        (Some(saved), Some(current)) => saved == current,
        _ => saved.position == current.position,
    }
}

fn overlap(position: PhysicalPosition<i32>, size: PhysicalSize<u32>, display: &Display) -> i64 {
    let width = (i64::from(position.x) + i64::from(size.width))
        .min(i64::from(display.position.x) + i64::from(display.size.width))
        - i64::from(position.x).max(i64::from(display.position.x));
    let height = (i64::from(position.y) + i64::from(size.height))
        .min(i64::from(display.position.y) + i64::from(display.size.height))
        - i64::from(position.y).max(i64::from(display.position.y));
    width.max(0).saturating_mul(height.max(0))
}

pub(super) fn clamp_position(
    position: PhysicalPosition<i32>,
    outer: PhysicalSize<u32>,
    display: &Display,
) -> PhysicalPosition<i32> {
    let clamp = |value: i32, origin: i32, available: u32, size: u32| {
        let end = i64::from(origin) + i64::from(available.saturating_sub(size));
        i32::try_from(i64::from(value).clamp(i64::from(origin), end)).unwrap_or(i32::MAX)
    };
    PhysicalPosition::new(
        clamp(
            position.x,
            display.position.x,
            display.size.width,
            outer.width,
        ),
        clamp(
            position.y,
            display.position.y,
            display.size.height,
            outer.height,
        ),
    )
}
