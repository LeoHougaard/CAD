//! Local application preferences. Document geometry and shared projects are separate.
use serde::{Deserialize, Serialize};
use std::{
    fs::{File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

type Result<T> = std::result::Result<T, String>;
const VERSION: u32 = 1;
const MAX_BYTES: u64 = 64 * 1024;
static TEMPORARY_ID: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Appearance {
    pub background: [u8; 3],
    pub panel: [u8; 3],
    pub turquoise: [u8; 3],
    pub purple: [u8; 3],
}
impl Default for Appearance {
    fn default() -> Self {
        Self {
            background: [7, 7, 10],
            panel: [12, 12, 15],
            turquoise: [112, 184, 190],
            purple: [170, 145, 196],
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct SpaceMouseSettings {
    pub enabled: bool,
    pub speed: f32,
    pub reverse_zoom: bool,
    pub reverse_rotation: bool,
}
impl Default for SpaceMouseSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            speed: 1.,
            reverse_zoom: false,
            reverse_rotation: false,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Settings {
    pub version: u32,
    pub appearance: Appearance,
    pub mouse_rotation_speed: f32,
    pub mouse_pan_speed: f32,
    pub mouse_zoom_speed: f32,
    pub space_mouse: SpaceMouseSettings,
    pub shaded_edges: bool,
    pub tree_open: bool,
    pub compact_toolbar: bool,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            version: VERSION,
            appearance: Appearance::default(),
            mouse_rotation_speed: 1.,
            mouse_pan_speed: 1.,
            mouse_zoom_speed: 1.,
            space_mouse: SpaceMouseSettings::default(),
            shaded_edges: true,
            tree_open: true,
            compact_toolbar: true,
        }
    }
}
impl Settings {
    pub fn load() -> Result<Self> {
        Self::load_from(&default_path()?)
    }
    pub fn save(&self) -> Result<()> {
        self.save_to(&default_path()?)
    }

    pub fn validate(&self) -> Result<()> {
        if self.version != VERSION {
            return Err(format!("Unsupported settings version {}", self.version));
        }
        Ok(())
    }
    pub fn sanitize(&mut self) {
        for speed in [
            &mut self.mouse_rotation_speed,
            &mut self.mouse_pan_speed,
            &mut self.mouse_zoom_speed,
            &mut self.space_mouse.speed,
        ] {
            *speed = if speed.is_finite() {
                speed.clamp(0.1, 5.)
            } else {
                1.
            };
        }
        // Appearance remains dark even if an edited configuration contains bright backgrounds.
        self.appearance
            .background
            .iter_mut()
            .for_each(|v| *v = (*v).min(32));
        self.appearance
            .panel
            .iter_mut()
            .for_each(|v| *v = (*v).min(40));
    }
    pub fn load_from(path: &Path) -> Result<Self> {
        let file = match File::open(path) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Self::default());
            }
            Err(error) => return Err(error.to_string()),
        };
        let mut bytes = Vec::new();
        file.take(MAX_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| e.to_string())?;
        if bytes.len() as u64 > MAX_BYTES {
            return Err("Settings file exceeds 64 KiB".into());
        }
        let mut settings: Self =
            serde_json::from_slice(&bytes).map_err(|e| format!("Invalid settings: {e}"))?;
        settings.validate()?;
        settings.sanitize();
        Ok(settings)
    }
    pub fn save_to(&self, path: &Path) -> Result<()> {
        self.validate()?;
        let mut settings = self.clone();
        settings.sanitize();
        let bytes = serde_json::to_vec_pretty(&settings).map_err(|e| e.to_string())?;
        write_atomic(path, |file| {
            file.write_all(&bytes).map_err(|e| e.to_string())
        })
    }
}

/// Uses only the current user's application configuration directory.
pub fn default_path() -> Result<PathBuf> {
    let nonempty = |name| {
        std::env::var_os(name)
            .filter(|s| !s.is_empty())
            .map(PathBuf::from)
    };
    #[cfg(windows)]
    {
        nonempty("LOCALAPPDATA")
            .map(|p| p.join("LeoHougaard").join("CAD").join("settings.json"))
            .ok_or_else(|| "LOCALAPPDATA is unavailable".into())
    }
    #[cfg(not(windows))]
    {
        nonempty("XDG_CONFIG_HOME")
            .filter(|p| p.is_absolute())
            .or_else(|| nonempty("HOME").map(|p| p.join(".config")))
            .map(|p| p.join("leo-hougaard-cad").join("settings.json"))
            .ok_or_else(|| "The user configuration directory is unavailable".into())
    }
}

pub(crate) fn write_atomic(path: &Path, write: impl FnOnce(&mut File) -> Result<()>) -> Result<()> {
    let name = path
        .file_name()
        .ok_or("Choose a settings file name")?
        .to_string_lossy();
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    let mut temporary = None;
    for _ in 0..16 {
        let serial = TEMPORARY_ID.fetch_add(1, Ordering::Relaxed);
        let candidate = parent.join(format!(".{name}.{}.{}.tmp", std::process::id(), serial));
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&candidate)
        {
            Ok(file) => {
                temporary = Some((candidate, file));
                break;
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e.to_string()),
        }
    }
    let (temporary, mut file) = temporary.ok_or("Unable to create temporary settings file")?;
    let result = (|| {
        write(&mut file)?;
        file.sync_all().map_err(|e| e.to_string())?;
        drop(file);
        std::fs::rename(&temporary, path).map_err(|e| e.to_string())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Directory(PathBuf);
    impl Directory {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "leocad-settings-{}-{}",
                std::process::id(),
                TEMPORARY_ID.fetch_add(1, Ordering::Relaxed)
            ));
            std::fs::create_dir_all(&path).unwrap();
            Self(path)
        }
        fn file(&self) -> PathBuf {
            self.0.join("settings.json")
        }
    }
    impl Drop for Directory {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    #[test]
    fn missing_preferences_default_and_saved_preferences_replace_atomically() {
        let dir = Directory::new();
        assert_eq!(
            Settings::load_from(&dir.file()).unwrap(),
            Settings::default()
        );
        assert!(!dir.file().exists());
        let mut settings = Settings::default();
        settings.save_to(&dir.file()).unwrap();
        settings.mouse_rotation_speed = 2.5;
        settings.space_mouse.reverse_zoom = true;
        settings.tree_open = false;
        settings.save_to(&dir.file()).unwrap();
        assert_eq!(Settings::load_from(&dir.file()).unwrap(), settings);
        assert_eq!(std::fs::read_dir(&dir.0).unwrap().count(), 1);
    }
    #[test]
    fn sanitization_bounds_navigation_and_keeps_background_dark() {
        let dir = Directory::new();
        std::fs::write(dir.file(), br#"{"mouse_rotation_speed":-20,"mouse_zoom_speed":100,"appearance":{"background":[255,255,255]}}"#).unwrap();
        let mut settings = Settings::load_from(&dir.file()).unwrap();
        assert_eq!(settings.mouse_rotation_speed, 0.1);
        assert_eq!(settings.mouse_zoom_speed, 5.);
        assert_eq!(settings.appearance.background, [32; 3]);
        settings.space_mouse.speed = f32::NAN;
        settings.sanitize();
        assert_eq!(settings.space_mouse.speed, 1.);
        assert_eq!(settings.appearance.purple, Appearance::default().purple);
    }
    #[test]
    fn invalid_or_future_settings_are_reported_without_overwriting_them() {
        let dir = Directory::new();
        for text in ["not json", r#"{"version":2}"#] {
            std::fs::write(dir.file(), text).unwrap();
            assert!(Settings::load_from(&dir.file()).is_err());
            assert_eq!(std::fs::read_to_string(dir.file()).unwrap(), text);
        }
        let original = std::fs::read(dir.file()).unwrap();
        let settings = Settings {
            version: 2,
            ..Settings::default()
        };
        assert!(settings.save_to(&dir.file()).is_err());
        assert_eq!(std::fs::read(dir.file()).unwrap(), original);
    }
    #[test]
    fn failed_partial_write_preserves_existing_preferences_and_cleans_temporary_file() {
        let dir = Directory::new();
        Settings::default().save_to(&dir.file()).unwrap();
        let original = std::fs::read(dir.file()).unwrap();
        assert!(
            write_atomic(&dir.file(), |file| {
                file.write_all(b"partial").unwrap();
                Err("Simulated interrupted write".into())
            })
            .is_err()
        );
        assert_eq!(std::fs::read(dir.file()).unwrap(), original);
        assert_eq!(std::fs::read_dir(&dir.0).unwrap().count(), 1);
    }
    #[test]
    fn failed_atomic_replace_preserves_existing_target() {
        let dir = Directory::new();
        let target = dir.0.join("existing-directory");
        std::fs::create_dir(&target).unwrap();
        std::fs::write(target.join("keep"), b"preserve").unwrap();
        assert!(Settings::default().save_to(&target).is_err());
        assert_eq!(std::fs::read(target.join("keep")).unwrap(), b"preserve");
        assert_eq!(std::fs::read_dir(&dir.0).unwrap().count(), 1);
    }
}
