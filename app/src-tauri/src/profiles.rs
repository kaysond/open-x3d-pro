//! Profile store: one JSON file per profile in `%APPDATA%\OpenX3DPro\profiles`, exactly one default.

use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use x3d_core::config::{DeviceConfig, Profile};

#[derive(Debug)]
pub enum StoreError {
    Io(io::Error),
    Json(serde_json::Error),
    Invalid(String),
    NotFound(String),
    DeleteDefault,
}

impl fmt::Display for StoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StoreError::Io(e) => write!(f, "file error: {e}"),
            StoreError::Json(e) => write!(f, "invalid profile JSON: {e}"),
            StoreError::Invalid(e) => f.write_str(e),
            StoreError::NotFound(id) => write!(f, "no profile with id {id}"),
            StoreError::DeleteDefault => f.write_str(
                "the default profile cannot be deleted; make another profile default first",
            ),
        }
    }
}

impl std::error::Error for StoreError {}

impl From<io::Error> for StoreError {
    fn from(e: io::Error) -> Self {
        StoreError::Io(e)
    }
}

impl From<serde_json::Error> for StoreError {
    fn from(e: serde_json::Error) -> Self {
        StoreError::Json(e)
    }
}

pub struct ProfileStore {
    dir: PathBuf,
    profiles: Vec<Profile>,
}

/// Ids double as file names.
fn valid_id(id: &str) -> bool {
    (1..=64).contains(&id.len())
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

fn new_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

/// Write-then-rename so a crash never leaves a truncated file.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    fs::write(&tmp, bytes)?;
    fs::rename(&tmp, path)
}

impl ProfileStore {
    /// Loads `dir/*.json` (the file stem is the id), skipping unreadable or invalid files.
    pub fn load(dir: PathBuf) -> Result<Self, StoreError> {
        fs::create_dir_all(&dir)?;
        let mut profiles = Vec::new();
        for entry in fs::read_dir(&dir)? {
            let path = entry?.path();
            if path.extension().is_none_or(|e| e != "json") {
                continue;
            }
            let Some(id) = path
                .file_stem()
                .and_then(|s| s.to_str())
                .filter(|s| valid_id(s))
            else {
                log::warn!(
                    "skipping {}: file name is not a valid profile id",
                    path.display()
                );
                continue;
            };
            let parsed = fs::read_to_string(&path)
                .map_err(StoreError::from)
                .and_then(|s| Ok(serde_json::from_str::<Profile>(&s)?))
                .and_then(|p| validate_config(&p.config).map(|()| p));
            match parsed {
                Ok(mut p) => {
                    p.id = id.to_string();
                    profiles.push(p);
                }
                Err(e) => log::warn!("skipping profile {}: {e}", path.display()),
            }
        }
        let mut store = ProfileStore { dir, profiles };
        store.sort();
        store.fix_default()?;
        Ok(store)
    }

    pub fn list(&self) -> &[Profile] {
        &self.profiles
    }

    pub fn get(&self, id: &str) -> Option<&Profile> {
        self.profiles.iter().find(|p| p.id == id)
    }

    /// Creates the profile when its id is new or empty. The default flag can be moved here
    /// (`isDefault: true`) but never cleared.
    pub fn save(&mut self, mut profile: Profile) -> Result<Profile, StoreError> {
        if profile.id.is_empty() {
            profile.id = new_id();
        }
        if !valid_id(&profile.id) {
            return Err(StoreError::Invalid(format!(
                "invalid profile id {:?}",
                profile.id
            )));
        }
        profile.name = profile.name.trim().to_string();
        if profile.name.is_empty() {
            return Err(StoreError::Invalid("profile name is empty".into()));
        }
        profile.exe_paths = profile
            .exe_paths
            .iter()
            .map(|e| e.trim().to_string())
            .filter(|e| !e.is_empty())
            .collect();
        validate_config(&profile.config)?;

        let was_default = self.get(&profile.id).is_some_and(|p| p.is_default);
        profile.is_default |= was_default;
        self.write(&profile)?;
        match self.profiles.iter_mut().find(|p| p.id == profile.id) {
            Some(slot) => *slot = profile.clone(),
            None => self.profiles.push(profile.clone()),
        }
        if profile.is_default && !was_default {
            self.set_default(&profile.id)?;
        }
        self.sort();
        Ok(profile)
    }

    pub fn delete(&mut self, id: &str) -> Result<(), StoreError> {
        let i = self
            .profiles
            .iter()
            .position(|p| p.id == id)
            .ok_or_else(|| StoreError::NotFound(id.into()))?;
        if self.profiles[i].is_default {
            return Err(StoreError::DeleteDefault);
        }
        match fs::remove_file(self.path(id)) {
            Err(e) if e.kind() != io::ErrorKind::NotFound => return Err(e.into()),
            _ => {}
        }
        self.profiles.remove(i);
        Ok(())
    }

    pub fn set_default(&mut self, id: &str) -> Result<(), StoreError> {
        let new = self
            .profiles
            .iter()
            .position(|p| p.id == id)
            .ok_or_else(|| StoreError::NotFound(id.into()))?;
        // New default first: a crash in between leaves two defaults, which load() repairs.
        self.set_flag(new, true)?;
        for i in 0..self.profiles.len() {
            if i != new {
                self.set_flag(i, false)?;
            }
        }
        self.sort();
        Ok(())
    }

    fn set_flag(&mut self, i: usize, is_default: bool) -> Result<(), StoreError> {
        if self.profiles[i].is_default != is_default {
            let mut p = self.profiles[i].clone();
            p.is_default = is_default;
            self.write(&p)?;
            self.profiles[i] = p;
        }
        Ok(())
    }

    /// Imports as a new, non-default profile.
    pub fn import(&mut self, json: &str) -> Result<Profile, StoreError> {
        let mut p: Profile = serde_json::from_str(json)?;
        p.id = new_id();
        p.is_default = false;
        self.save(p)
    }

    pub fn export(&self, id: &str) -> Result<String, StoreError> {
        let p = self
            .get(id)
            .ok_or_else(|| StoreError::NotFound(id.into()))?;
        Ok(serde_json::to_string_pretty(p)?)
    }

    fn fix_default(&mut self) -> Result<(), StoreError> {
        let defaults: Vec<usize> = (0..self.profiles.len())
            .filter(|&i| self.profiles[i].is_default)
            .collect();
        if defaults.is_empty() {
            let p = Profile {
                id: new_id(),
                name: "Default".into(),
                exe_paths: Vec::new(),
                is_default: true,
                config: DeviceConfig::default(),
            };
            self.write(&p)?;
            self.profiles.insert(0, p);
        }
        for &i in defaults.iter().skip(1) {
            log::warn!(
                "profile {:?} was also marked default; unmarking",
                self.profiles[i].name
            );
            self.set_flag(i, false)?;
        }
        self.sort();
        Ok(())
    }

    fn path(&self, id: &str) -> PathBuf {
        self.dir.join(format!("{id}.json"))
    }

    fn write(&self, p: &Profile) -> Result<(), StoreError> {
        Ok(write_atomic(
            &self.path(&p.id),
            &serde_json::to_vec_pretty(p)?,
        )?)
    }

    /// Default first, then by name.
    fn sort(&mut self) {
        self.profiles
            .sort_by_cached_key(|p| (!p.is_default, p.name.to_lowercase()));
    }
}

fn validate_config(config: &DeviceConfig) -> Result<(), StoreError> {
    config.validate().map_err(|e| StoreError::Invalid(e.0))?;
    for k in config.key_bindings.iter().flat_map(|b| &b.keys) {
        if crate::keys::scan_code(k).is_none() {
            return Err(StoreError::Invalid(format!("unknown key {k:?}")));
        }
    }
    Ok(())
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use x3d_core::config::KeyBinding;

    /// Fresh per-test directory under the system temp dir.
    pub(crate) fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("ox3d-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn named(name: &str) -> Profile {
        Profile {
            id: String::new(),
            name: name.into(),
            exe_paths: vec![" game.exe ".into(), "".into()],
            is_default: false,
            config: DeviceConfig::default(),
        }
    }

    #[test]
    fn first_run_creates_default_and_persists() {
        let dir = temp_dir("store-first");
        let mut s = ProfileStore::load(dir.clone()).unwrap();
        assert_eq!(s.list().len(), 1);
        let def = s.list()[0].clone();
        assert!(def.is_default);
        assert_eq!(def.config, DeviceConfig::default());

        let saved = s.save(named("  Elite  ")).unwrap();
        assert_eq!(saved.name, "Elite");
        assert_eq!(saved.exe_paths, vec!["game.exe"]);
        assert!(uuid::Uuid::parse_str(&saved.id).is_ok());

        let s2 = ProfileStore::load(dir.clone()).unwrap();
        assert_eq!(s2.list().len(), 2);
        assert_eq!(s2.list()[0].id, def.id);
        assert_eq!(s2.get(&saved.id).unwrap(), &saved);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn default_rules() {
        let dir = temp_dir("store-default");
        let mut s = ProfileStore::load(dir.clone()).unwrap();
        let def = s.list()[0].id.clone();
        let other = s.save(named("Other")).unwrap().id;
        assert!(matches!(s.delete(&def), Err(StoreError::DeleteDefault)));

        // Clearing the flag on the default is ignored.
        let mut d = s.get(&def).unwrap().clone();
        d.is_default = false;
        assert!(s.save(d).unwrap().is_default);

        s.set_default(&other).unwrap();
        assert_eq!(s.list().iter().filter(|p| p.is_default).count(), 1);
        assert_eq!(s.list()[0].id, other);
        s.delete(&def).unwrap();
        assert!(!dir.join(format!("{def}.json")).exists());
        assert!(matches!(s.delete(&def), Err(StoreError::NotFound(_))));

        // Moving the default via save().
        let mut third = named("Third");
        third.is_default = true;
        let third = s.save(third).unwrap().id;
        let reloaded = ProfileStore::load(dir.clone()).unwrap();
        let defaults: Vec<_> = reloaded.list().iter().filter(|p| p.is_default).collect();
        assert_eq!(defaults.len(), 1);
        assert_eq!(defaults[0].id, third);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn load_repairs_and_skips_bad_files() {
        let dir = temp_dir("store-repair");
        let mut a = named("A");
        a.id = "a".into();
        a.is_default = true;
        let mut b = named("B");
        b.id = "b".into();
        b.is_default = true;
        fs::write(dir.join("a.json"), serde_json::to_vec(&a).unwrap()).unwrap();
        // Stem wins over the id inside the file.
        fs::write(
            dir.join("b.json"),
            serde_json::to_vec(&Profile {
                id: "zzz".into(),
                ..b
            })
            .unwrap(),
        )
        .unwrap();
        fs::write(dir.join("broken.json"), b"{").unwrap();
        fs::write(dir.join("bad name.json"), b"{}").unwrap();
        let mut invalid = named("Invalid");
        invalid.config.axes.x.sensitivity = 9.0;
        fs::write(dir.join("c.json"), serde_json::to_vec(&invalid).unwrap()).unwrap();

        let s = ProfileStore::load(dir.clone()).unwrap();
        let ids: Vec<_> = s
            .list()
            .iter()
            .map(|p| (p.id.as_str(), p.is_default))
            .collect();
        assert_eq!(ids, vec![("a", true), ("b", false)]);
        let b_disk: Profile =
            serde_json::from_slice(&fs::read(dir.join("b.json")).unwrap()).unwrap();
        assert!(!b_disk.is_default);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn save_validates() {
        let dir = temp_dir("store-validate");
        let mut s = ProfileStore::load(dir.clone()).unwrap();
        let mut p = named("X");
        p.id = "../evil".into();
        assert!(matches!(s.save(p), Err(StoreError::Invalid(_))));
        assert!(matches!(s.save(named(" ")), Err(StoreError::Invalid(_))));
        let mut p = named("Keys");
        p.config.key_bindings.push(KeyBinding {
            button: 1,
            keys: vec!["ControlLeft".into(), "Hyper".into()],
        });
        assert!(s.save(p.clone()).unwrap_err().to_string().contains("Hyper"));
        p.config.key_bindings[0].keys.pop();
        s.save(p).unwrap();
        let mut p = named("Bad");
        p.config.buttons[0].output = Some(40);
        assert!(s.save(p).is_err());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn import_export() {
        let dir = temp_dir("store-import");
        let mut s = ProfileStore::load(dir.clone()).unwrap();
        let def_id = s.list()[0].id.clone();
        let json = s.export(&def_id).unwrap();
        let imported = s.import(&json).unwrap();
        assert_ne!(imported.id, def_id);
        assert!(!imported.is_default);
        assert_eq!(imported.config, s.get(&def_id).unwrap().config);
        assert_eq!(s.list().len(), 2);
        assert!(matches!(s.import("{\"name\":1}"), Err(StoreError::Json(_))));
        assert!(matches!(s.export("nope"), Err(StoreError::NotFound(_))));
        fs::remove_dir_all(dir).unwrap();
    }
}
