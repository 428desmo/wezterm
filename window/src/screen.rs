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
