use std::path::{Path, PathBuf};

#[derive(Clone)]
pub struct Env {
    game_dir: PathBuf,
    platform_working_dir: PathBuf,
    source_dir: PathBuf,
}

impl Env {
    pub fn with_output_root(&self, root: &Path) -> Self {
        Self {
            platform_working_dir: root.to_path_buf(),
            ..self.clone()
        }
    }

    pub fn new(platform_working_dir: &str, source_dir: &str) -> Result<Self, String> {
        let game_dir = env_path("TA_GAME_PATH")?;
        // The incoming source_dir is a project-relative path; resolve it against
        // the game root.
        let source_dir = game_dir.join(source_dir);
        let platform_working_dir = game_dir.join(platform_working_dir);

        Ok(Self {
            game_dir,
            platform_working_dir,
            source_dir,
        })
    }

    /// Gets the game directory
    pub fn get_game_dir(&self) -> &Path {
        &self.game_dir
    }

    /// The project directory the Linker passed
    pub fn get_source_dir(&self) -> &Path {
        &self.source_dir
    }

    /// Gets the platform working directory (per-platform build output root)
    pub fn get_platform_working_dir(&self) -> &Path {
        &self.platform_working_dir
    }

    /// Deploy bank directory: `<platform_working>/zone/snd/<language.DeployName>`
    pub fn get_deploy_bank_dir(&self, language_deploy_name: &str) -> PathBuf {
        self.platform_working_dir
            .join("zone")
            .join("snd")
            .join(language_deploy_name)
    }

    /// Cache bank directory: `<platform_working>/sound/zone/CachedBanks/<language.CacheName>`
    pub fn get_cache_bank_dir(&self, language_cache_name: &str) -> PathBuf {
        self.platform_working_dir
            .join("sound")
            .join("zone")
            .join("CachedBanks")
            .join(language_cache_name)
    }

    /// Per-zone .sz output directory. Shared (`all`) outputs go directly under
    /// `<platform_working>/sound/zone/`; per-locale outputs nest a language
    /// segment in: `<platform_working>/<language>/sound/zone/`.
    pub fn get_zone_output_dir(&self, language_name: &str) -> PathBuf {
        if language_name == "all" {
            self.platform_working_dir.join("sound").join("zone")
        } else {
            self.platform_working_dir
                .join(language_name)
                .join("sound")
                .join("zone")
        }
    }

    /// Gets the sounds directory
    pub fn get_sound_dir(&self) -> PathBuf {
        self.get_game_dir().join("share/raw/sound")
    }

    /// Gets the map source directory
    pub fn get_maps_source_dir(&self) -> PathBuf {
        self.get_game_dir().join("map_source")
    }

    /// Gets the sound aliases directory
    pub fn get_sound_alias_dir(&self) -> PathBuf {
        self.get_sound_dir().join("aliases")
    }

    /// Gets the sound templates directory
    pub fn get_sound_alias_template_dir(&self) -> PathBuf {
        self.get_sound_dir().join("templates")
    }

    /// Gets the sound ambients directory
    pub fn get_sound_ambient_dir(&self) -> PathBuf {
        self.get_sound_dir().join("ambients")
    }

    /// Gets the sound reverbs directory
    pub fn get_sound_reverb_dir(&self) -> PathBuf {
        self.get_sound_dir().join("reverb")
    }

    /// Gets the sound ducks directory
    pub fn get_duck_source_dir(&self) -> PathBuf {
        self.get_sound_dir().join("ducks")
    }

    /// Gets the sound globals directory
    pub fn get_sound_globals_dir(&self) -> PathBuf {
        self.get_sound_dir().join("globals")
    }

    /// Gets the script ID directory
    pub fn get_script_id_dir(&self) -> PathBuf {
        self.get_sound_dir().join("scriptid")
    }

    /// Gets the sound zone config directory
    pub fn get_sound_zone_config_dir(&self) -> PathBuf {
        self.source_dir.join("sound").join("zoneconfig")
    }

    /// Gets the source asset directory
    pub fn get_source_asset_dir(&self, localized: bool) -> PathBuf {
        if localized {
            self.get_game_dir().join("soundloc_assets")
        } else {
            self.get_game_dir().join("sound_assets")
        }
    }

    /// Gets the music directory
    pub fn get_music_dir(&self) -> PathBuf {
        self.get_sound_dir().join("music")
    }
}

fn env_path(key: &str) -> Result<PathBuf, String> {
    std::env::var(key)
        .map(PathBuf::from)
        .map_err(|_| format!("{} environment variable not set", key))
}
