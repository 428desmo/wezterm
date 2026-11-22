//! Local desktop positions for native tmux windows. Keys include the remote
//! host, socket, server pid, session creation time and window ID, so a new
//! server/session does not inherit an unrelated window's position.
use config::{Dimension, GeometryOrigin, GuiPosition};
use lazy_static::lazy_static;
use parking_lot::Mutex;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct DisplayInfo {
    pub id: String,
    pub x: isize,
    pub y: isize,
    pub width: isize,
    pub height: isize,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(untagged)]
enum SavedPosition {
    WithSize {
        position: Option<Box<SavedPosition>>,
        rows: usize,
        cols: usize,
    },
    Legacy((isize, isize)),
    Display {
        x: isize,
        y: isize,
        display_id: String,
        relative_x: f64,
        relative_y: f64,
    },
}

impl SavedPosition {
    fn size(&self) -> Option<(usize, usize)> {
        match self {
            Self::WithSize { rows, cols, .. } if *rows > 0 && *cols > 0 => Some((*rows, *cols)),
            _ => None,
        }
    }

    fn with_size(previous: Option<&Self>, rows: usize, cols: usize) -> Self {
        let position = match previous {
            Some(Self::WithSize { position, .. }) => position.clone(),
            value => value.cloned().map(Box::new),
        };
        Self::WithSize {
            position,
            rows,
            cols,
        }
    }

    fn with_position(previous: Option<&Self>, value: Self) -> Self {
        match previous {
            Some(Self::WithSize { rows, cols, .. }) => Self::WithSize {
                position: Some(Box::new(value)),
                rows: *rows,
                cols: *cols,
            },
            _ => value,
        }
    }

    fn resolve(&self, displays: &[DisplayInfo]) -> Option<(isize, isize)> {
        match self {
            Self::WithSize { position, .. } => position.as_ref()?.resolve(displays),
            Self::Legacy(position) => Some(*position),
            Self::Display {
                display_id,
                relative_x,
                relative_y,
                ..
            } => {
                let display = displays.iter().find(|display| display.id == *display_id)?;
                if !(0.0..1.0).contains(relative_x) || !(0.0..1.0).contains(relative_y) {
                    return None;
                }
                Some((
                    display.x + (relative_x * display.width as f64).round() as isize,
                    display.y + (relative_y * display.height as f64).round() as isize,
                ))
            }
        }
    }
}

#[derive(Default)]
struct Positions {
    values: BTreeMap<String, SavedPosition>,
    revision: u64,
    saved_revision: u64,
}

impl Positions {
    fn load(path: &Path) -> Self {
        let values = match std::fs::read(path) {
            Ok(data) => match serde_json::from_slice(&data) {
                Ok(values) => values,
                Err(err) => {
                    log::warn!("Ignoring invalid tmux positions {}: {err}", path.display());
                    BTreeMap::new()
                }
            },
            Err(err) => {
                if err.kind() != std::io::ErrorKind::NotFound {
                    log::warn!("Cannot read tmux positions {}: {err}", path.display());
                }
                BTreeMap::new()
            }
        };
        Self {
            values,
            ..Self::default()
        }
    }

    fn save(&mut self, path: &Path) -> anyhow::Result<()> {
        if self.revision == self.saved_revision {
            return Ok(());
        }
        std::fs::create_dir_all(path.parent().unwrap())?;
        let temporary = path.with_extension(format!("{}.tmp", std::process::id()));
        std::fs::write(&temporary, serde_json::to_vec_pretty(&self.values)?)?;
        std::fs::rename(&temporary, path)?;
        self.saved_revision = self.revision;
        Ok(())
    }
}

fn path() -> PathBuf {
    config::DATA_DIR.join("tmux-window-positions.json")
}

lazy_static! {
    static ref POSITIONS: Mutex<Option<Positions>> = Mutex::new(None);
}

pub fn restore(key: &str, displays: &[DisplayInfo]) -> Option<GuiPosition> {
    let mut positions = POSITIONS.lock();
    let positions = positions.get_or_insert_with(|| Positions::load(&path()));
    let (x, y) = positions.values.get(key)?.resolve(displays)?;
    Some(GuiPosition {
        x: Dimension::Pixels(x as f32),
        y: Dimension::Pixels(y as f32),
        origin: GeometryOrigin::ScreenCoordinateSystem,
    })
}

pub fn restore_size(key: &str) -> Option<(usize, usize)> {
    let mut positions = POSITIONS.lock();
    let positions = positions.get_or_insert_with(|| Positions::load(&path()));
    positions.values.get(key)?.size()
}

pub fn record_size(key: &str, rows: usize, cols: usize) {
    if rows == 0 || cols == 0 {
        return;
    }
    update(key, |previous| {
        SavedPosition::with_size(previous, rows, cols)
    });
}

pub(crate) fn record(key: &str, x: isize, y: isize, display: Option<&DisplayInfo>) {
    let value = match display {
        Some(display) if display.width > 0 && display.height > 0 => SavedPosition::Display {
            x,
            y,
            display_id: display.id.clone(),
            relative_x: (x - display.x) as f64 / display.width as f64,
            relative_y: (y - display.y) as f64 / display.height as f64,
        },
        _ => SavedPosition::Legacy((x, y)),
    };
    update(key, |previous| {
        SavedPosition::with_position(previous, value)
    });
}

fn update(key: &str, make_value: impl FnOnce(Option<&SavedPosition>) -> SavedPosition) {
    let revision = {
        let mut positions = POSITIONS.lock();
        let positions = positions.get_or_insert_with(|| Positions::load(&path()));
        let value = make_value(positions.values.get(key));
        if positions.values.get(key) == Some(&value) {
            return;
        }
        positions.values.insert(key.to_string(), value);
        positions.revision += 1;
        positions.revision
    };
    // Coalesce a drag's move notifications. Shutdown also flushes synchronously.
    smol::spawn(async move {
        smol::Timer::after(std::time::Duration::from_millis(250)).await;
        let mut positions = POSITIONS.lock();
        if let Some(positions) = positions.as_mut() {
            if positions.revision == revision {
                if let Err(err) = positions.save(&path()) {
                    log::warn!("Cannot save tmux window positions: {err:#}");
                }
            }
        }
    })
    .detach();
}

pub fn flush() {
    if let Some(positions) = POSITIONS.lock().as_mut() {
        if let Err(err) = positions.save(&path()) {
            log::warn!("Cannot save tmux window positions: {err:#}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn control_size_and_position_survive_independent_updates_and_restart() {
        let directory = tempfile::tempdir().unwrap();
        let file = directory.path().join("positions.json");
        let old = SavedPosition::Legacy((20, 40));
        let resized = SavedPosition::with_size(Some(&old), 31, 103);
        let moved = SavedPosition::with_position(Some(&resized), SavedPosition::Legacy((60, 80)));
        let resized_again = SavedPosition::with_size(Some(&moved), 35, 110);
        let mut positions = Positions::default();
        positions
            .values
            .insert("session:control".into(), resized_again);
        positions.revision += 1;
        positions.save(&file).unwrap();
        let restored = Positions::load(&file);
        let control = &restored.values["session:control"];
        assert_eq!(control.size(), Some((35, 110)));
        assert_eq!(control.resolve(&[]), Some((60, 80)));
        // Size can be restored even when no usable display position exists.
        let size_only = SavedPosition::with_size(None, 31, 103);
        let encoded = serde_json::to_vec(&size_only).unwrap();
        let decoded: SavedPosition = serde_json::from_slice(&encoded).unwrap();
        assert_eq!(decoded.size(), Some((31, 103)));
        assert_eq!(decoded.resolve(&[]), None);
        assert_eq!(SavedPosition::with_size(None, 0, 103).size(), None);
        let old_display: SavedPosition = serde_json::from_str(
            r#"{"x":20,"y":40,"display_id":"a","relative_x":0.1,"relative_y":0.2}"#,
        )
        .unwrap();
        assert_eq!(old_display.size(), None);
    }

    #[test]
    fn display_identity_survives_rearrangement_and_resolution_change() {
        let saved = SavedPosition::Display {
            x: 200,
            y: 150,
            display_id: "physical-monitor-a".into(),
            relative_x: 0.25,
            relative_y: 0.5,
        };
        let displays = vec![
            DisplayInfo {
                id: "physical-monitor-b".into(),
                x: 0,
                y: 0,
                width: 800,
                height: 300,
            },
            DisplayInfo {
                id: "physical-monitor-a".into(),
                x: -1600,
                y: -600,
                width: 1600,
                height: 600,
            },
        ];
        assert_eq!(saved.resolve(&displays), Some((-1200, -300)));
        assert_eq!(saved.resolve(&displays[..1]), None);
        assert_eq!(
            SavedPosition::Legacy((200, 150)).resolve(&displays),
            Some((200, 150))
        );
        let directory = tempfile::tempdir().unwrap();
        let file = directory.path().join("positions.json");
        let mut positions = Positions::default();
        positions.values.insert("remote-window".into(), saved);
        positions.revision += 1;
        positions.save(&file).unwrap();
        assert_eq!(
            Positions::load(&file).values["remote-window"].resolve(&displays),
            Some((-1200, -300))
        );
    }

    #[test]
    fn legacy_position_file_still_loads() {
        let directory = tempfile::tempdir().unwrap();
        let file = directory.path().join("positions.json");
        std::fs::write(&file, br#"{"old-window":[-900,200]}"#).unwrap();
        assert_eq!(
            Positions::load(&file).values["old-window"].resolve(&[]),
            Some((-900, 200))
        );
    }

    #[test]
    fn positions_survive_restart_and_keep_remote_windows_separate() {
        let directory = tempfile::tempdir().unwrap();
        let file = directory.path().join("positions.json");
        let mut positions = Positions::load(&file);
        positions.values.insert(
            "host-a:socket:1:100:@1".into(),
            SavedPosition::Legacy((-900, 200)),
        );
        positions.values.insert(
            "host-b:socket:1:100:@1".into(),
            SavedPosition::Legacy((300, -50)),
        );
        positions.revision += 1;
        positions.save(&file).unwrap();
        let mut restored = Positions::load(&file);
        assert_eq!(
            restored.values["host-a:socket:1:100:@1"],
            SavedPosition::Legacy((-900, 200))
        );
        assert_eq!(
            restored.values["host-b:socket:1:100:@1"],
            SavedPosition::Legacy((300, -50))
        );
        assert!(!restored.values.contains_key("host-a:socket:2:101:@1"));
        restored.values.insert(
            "host-a:socket:1:100:@1".into(),
            SavedPosition::Legacy((20, 40)),
        );
        restored.revision += 1;
        restored.save(&file).unwrap();
        assert_eq!(
            Positions::load(&file).values["host-a:socket:1:100:@1"],
            SavedPosition::Legacy((20, 40))
        );
    }

    #[test]
    fn invalid_file_does_not_prevent_attach() {
        let directory = tempfile::tempdir().unwrap();
        let file = directory.path().join("positions.json");
        std::fs::write(&file, b"invalid json").unwrap();
        assert!(Positions::load(&file).values.is_empty());
    }
}

/// Explicit tab group edits are independent of attach-time window discovery.
/// Do not snapshot during shutdown: transport teardown has already removed tabs.
#[derive(Default, Debug, serde::Serialize, serde::Deserialize)]
struct TabGroups {
    groups: Vec<SavedTabGroup>,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct SavedTabGroup {
    tabs: Vec<String>,
    position_key: String,
}

impl TabGroups {
    fn load(path: &Path) -> Self {
        match std::fs::read(path) {
            Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_else(|err| {
                log::warn!("Ignoring invalid tmux tab groups: {err}");
                Self::default()
            }),
            Err(err) => {
                if err.kind() != std::io::ErrorKind::NotFound {
                    log::warn!("Cannot read tmux tab groups: {err}");
                }
                Self::default()
            }
        }
    }

    fn find(&self, key: &str) -> Option<Vec<String>> {
        self.groups
            .iter()
            .find(|group| group.tabs.iter().any(|member| member == key))
            .map(|group| group.tabs.clone())
    }

    fn update(&mut self, groups: Vec<(String, Vec<String>)>) {
        self.groups.retain(|old| {
            !old.tabs
                .iter()
                .any(|key| groups.iter().any(|(_, tabs)| tabs.contains(key)))
        });
        self.groups.extend(
            groups
                .into_iter()
                .filter(|(_, tabs)| !tabs.is_empty())
                .map(|(position_key, tabs)| SavedTabGroup { tabs, position_key }),
        );
    }

    fn save(&self, path: &Path) -> anyhow::Result<()> {
        use std::io::Write;
        let directory = path
            .parent()
            .ok_or_else(|| anyhow::anyhow!("Missing layout directory"))?;
        std::fs::create_dir_all(directory)?;
        let mut temporary = tempfile::NamedTempFile::new_in(directory)?;
        temporary.write_all(&serde_json::to_vec_pretty(self)?)?;
        temporary.flush()?;
        // persist replaces existing files on Windows as well as Unix.
        temporary.persist(path)?;
        Ok(())
    }
}

fn groups_path() -> PathBuf {
    // Allows disposable GUI tests to exercise restart persistence in isolation.
    std::env::var_os("WEZTERM_TMUX_GROUPS_FILE")
        .map(PathBuf::from)
        .unwrap_or_else(|| config::DATA_DIR.join("tmux-tab-groups.json"))
}

lazy_static! {
    static ref TAB_GROUPS: Mutex<Option<TabGroups>> = Mutex::new(None);
}

pub(crate) fn tab_group(key: &str) -> Option<Vec<String>> {
    TAB_GROUPS
        .lock()
        .get_or_insert_with(|| TabGroups::load(&groups_path()))
        .find(key)
}

pub(crate) fn tab_group_position_key(key: &str) -> Option<String> {
    let mut saved = TAB_GROUPS.lock();
    saved
        .get_or_insert_with(|| TabGroups::load(&groups_path()))
        .groups
        .iter()
        .find(|group| group.tabs.iter().any(|tab| tab == key))
        .map(|group| group.position_key.clone())
}

pub(crate) fn copy_position(from: &str, to: &str) {
    if from == to {
        return;
    }
    let value = {
        let mut saved = POSITIONS.lock();
        saved
            .get_or_insert_with(|| Positions::load(&path()))
            .values
            .get(from)
            .cloned()
    };
    if let Some(value) = value {
        update(to, |_| value);
    }
}

pub(crate) fn record_tab_groups(groups: Vec<(String, Vec<String>)>) {
    if groups.iter().all(|(_, tabs)| tabs.is_empty()) {
        return;
    }
    let mut saved = TAB_GROUPS.lock();
    let saved = saved.get_or_insert_with(|| TabGroups::load(&groups_path()));
    saved.update(groups);
    if let Err(err) = saved.save(&groups_path()) {
        log::warn!("Cannot save tmux tab groups: {err:#}");
    }
}

#[cfg(test)]
mod tab_group_tests {
    use super::*;

    #[test]
    fn group_order_and_tear_off_survive_restart_and_repeated_save() {
        let directory = tempfile::tempdir().unwrap();
        let file = directory.path().join("groups.json");
        let mut saved = TabGroups::default();
        saved.update(vec![
            ("A".into(), vec!["A".into(), "B".into(), "C".into()]),
            ("other-session:X".into(), vec!["other-session:X".into()]),
        ]);
        saved.save(&file).unwrap();
        let mut saved = TabGroups::load(&file);
        assert_eq!(saved.find("B").unwrap(), ["A", "B", "C"]);
        saved.update(vec![
            ("A".into(), vec!["C".into(), "A".into()]),
            ("B".into(), vec!["B".into()]),
        ]);
        saved.save(&file).unwrap();
        let saved = TabGroups::load(&file);
        assert_eq!(saved.find("A").unwrap(), ["C", "A"]);
        assert_eq!(saved.find("B").unwrap(), ["B"]);
        assert_eq!(saved.find("other-session:X").unwrap(), ["other-session:X"]);
        assert!(saved.find("new-session:A").is_none());
    }
}
