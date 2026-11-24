use crate::ScreenRect;
use std::collections::HashMap;

/// Screen identity and bounds in the same top-left coordinate system as
/// WindowOps::set_window_position (unlike native Cocoa screen frames).
#[derive(Debug, Clone)]
pub struct PlacementScreen {
    pub id: String,
    pub rect: ScreenRect,
    pub dpi: f64,
    pub backing_scale: f64,
}

#[derive(Debug, Clone)]
pub struct Screens {
    pub main: ScreenInfo,
    pub active: ScreenInfo,
    pub by_name: HashMap<String, ScreenInfo>,
    pub virtual_rect: ScreenRect,
}

#[derive(Debug, Clone)]
pub struct ScreenInfo {
    pub name: String,
    pub rect: ScreenRect,
    pub scale: f64,
    pub max_fps: Option<usize>,
    pub effective_dpi: Option<f64>,
}

/// Fit an outer window frame into one monitor's usable desktop area.
/// All coordinates and dimensions must use the same native pixel space.
pub fn fit_window_to_work_area(frame: ScreenRect, work: ScreenRect) -> ScreenRect {
    if work.width() <= 0 || work.height() <= 0 {
        return frame;
    }
    let width = frame.width().max(1).min(work.width());
    let height = frame.height().max(1).min(work.height());
    euclid::rect(
        frame.origin.x.clamp(work.min_x(), work.max_x() - width),
        frame.origin.y.clamp(work.min_y(), work.max_y() - height),
        width,
        height,
    )
}

#[cfg(test)]
mod placement_tests {
    use super::*;

    #[test]
    fn fits_edges_and_taskbar_without_changing_a_fitting_size() {
        let work = euclid::rect(0, 0, 1920, 1040);
        assert_eq!(
            fit_window_to_work_area(euclid::rect(1900, 1030, 800, 600), work),
            euclid::rect(1120, 440, 800, 600)
        );
        assert_eq!(
            fit_window_to_work_area(euclid::rect(-50, -40, 800, 600), work),
            euclid::rect(0, 0, 800, 600)
        );
        assert_eq!(
            fit_window_to_work_area(euclid::rect(100, 100, 800, 600), work),
            euclid::rect(100, 100, 800, 600)
        );
    }

    #[test]
    fn supports_negative_monitor_coordinates_and_oversized_windows() {
        let work = euclid::rect(-1920, -900, 1920, 1040);
        assert_eq!(
            fit_window_to_work_area(euclid::rect(-100, -950, 800, 600), work),
            euclid::rect(-800, -900, 800, 600)
        );
        assert_eq!(
            fit_window_to_work_area(euclid::rect(-1800, -800, 3840, 2160), work),
            work
        );
        // Higher DPI dimensions are still physical pixels, without rescaling.
        let work = euclid::rect(1920, 0, 2560, 1400);
        assert_eq!(
            fit_window_to_work_area(euclid::rect(4400, 1300, 1600, 1200), work),
            euclid::rect(2880, 200, 1600, 1200)
        );
    }
}
