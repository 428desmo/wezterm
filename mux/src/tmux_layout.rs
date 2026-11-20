//! Local desktop positions for native tmux windows. Keys include the remote
//! host, socket, server pid, session creation time and window ID, so a new
//! server/session does not inherit an unrelated window's position.
use config::{Dimension, GeometryOrigin, GuiPosition};
use lazy_static::lazy_static;
use parking_lot::Mutex;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Default)]
struct Positions {
    values: BTreeMap<String, (isize, isize)>,
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

pub(crate) fn restore(key: &str) -> Option<GuiPosition> {
    let mut positions = POSITIONS.lock();
    let positions = positions.get_or_insert_with(|| Positions::load(&path()));
    positions.values.get(key).map(|&(x, y)| GuiPosition {
        x: Dimension::Pixels(x as f32),
        y: Dimension::Pixels(y as f32),
        origin: GeometryOrigin::ScreenCoordinateSystem,
    })
}

pub(crate) fn record(key: &str, x: isize, y: isize) {
    let revision = {
        let mut positions = POSITIONS.lock();
        let positions = positions.get_or_insert_with(|| Positions::load(&path()));
        if positions.values.get(key) == Some(&(x, y)) {
            return;
        }
        positions.values.insert(key.to_string(), (x, y));
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
    fn positions_survive_restart_and_keep_remote_windows_separate() {
        let directory = tempfile::tempdir().unwrap();
        let file = directory.path().join("positions.json");
        let mut positions = Positions::load(&file);
        positions
            .values
            .insert("host-a:socket:1:100:@1".into(), (-900, 200));
        positions
            .values
            .insert("host-b:socket:1:100:@1".into(), (300, -50));
        positions.revision += 1;
        positions.save(&file).unwrap();
        let mut restored = Positions::load(&file);
        assert_eq!(restored.values["host-a:socket:1:100:@1"], (-900, 200));
        assert_eq!(restored.values["host-b:socket:1:100:@1"], (300, -50));
        assert!(!restored.values.contains_key("host-a:socket:2:101:@1"));
        restored
            .values
            .insert("host-a:socket:1:100:@1".into(), (20, 40));
        restored.revision += 1;
        restored.save(&file).unwrap();
        assert_eq!(
            Positions::load(&file).values["host-a:socket:1:100:@1"],
            (20, 40)
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
