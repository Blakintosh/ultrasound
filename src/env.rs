use std::path::{Path, PathBuf};

pub struct Env {
    game_dir: PathBuf,
    tools_dir: PathBuf,
    platform_working_dir: PathBuf,
    source_dir: PathBuf
}

impl Env {
    pub fn new(platform_working_dir: &str, source_dir: &str) -> Result<Self, String> {
        let game_dir = env_path("TA_GAME_PATH")?;
        let tools_dir = env_path("TA_TOOLS_PATH")?;
        // The incoming source_dir is a project-relative path; resolve it against
        // the game root.
        let source_dir = game_dir.join(source_dir);
        let platform_working_dir = game_dir.join(platform_working_dir);

        Ok(Self {
            game_dir,
            tools_dir,
            platform_working_dir,
            source_dir
        })
    }

    /// Gets the game directory
    pub fn get_game_dir(&self) -> &Path {
        &self.game_dir
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

    /// Gets the Mod Tools directory
    pub fn get_tools_dir(&self) -> &Path {
        &self.tools_dir
    }

    /// Gets the sounds directory
    pub fn get_sound_dir(&self) -> PathBuf {
        self.get_game_dir().join("share/raw/sound")
    }

    /// Gets the sounds directory for a particular language.
    pub fn get_sound_dir_for_language(&self, language: &str) -> PathBuf {
        self.get_game_dir().join("share/raw").join(language).join("sound")
    }

    /// Gets the database cache directory
    pub fn get_db_cache_dir(&self) -> PathBuf {
        self.get_sound_dir().join("db")
    }

    /// Gets the sound bin directory
    pub fn get_sound_bin_dir(&self) -> PathBuf {
        self.get_tools_dir().join("sound")
    }

    /// Gets the cache directory
    pub fn get_cache_dir(&self) -> PathBuf {
        self.get_db_cache_dir().join("cache")
    }

    /// Gets the client scripts directory
    pub fn get_clientscript_dir(&self) -> PathBuf {
        self.get_game_dir().join("share/raw/scripts")
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

    /// Gets the sound ship aliases directory
    pub fn get_sound_ship_alias_dir(&self) -> PathBuf {
        self.get_sound_dir().join("ship")
    }

    /// Gets the sound globals directory
    pub fn get_sound_globals_dir(&self) -> PathBuf {
        self.get_sound_dir().join("globals")
    }

    /// Gets the sound ship globals directory
    pub fn get_sound_ship_globals_dir(&self) -> PathBuf {
        self.get_sound_dir().join("ship")
    }

    /// Gets the sound contexts directory
    pub fn get_souund_context_dir(&self) -> PathBuf {
        self.get_sound_dir().join("contexts")
    }

    /// Gets the script ID directory
    pub fn get_script_id_dir(&self) -> PathBuf {
        self.get_sound_dir().join("scriptid")
    }

    /// Gets the sound zone config directory
    pub fn get_sound_zone_config_dir(&self) -> PathBuf {
        self.source_dir.join("sound").join("zoneconfig")
    }

    /// Gets the zone source directory
    pub fn get_zone_source_dir(&self) -> PathBuf {
        self.get_game_dir().join("share/zone_source")
    }

    /// Gets the CSV or XLS path for a given name (returns CSV path)
    pub fn get_csv_or_xls(&self, name: &Path) -> PathBuf {
        name.with_extension("csv")
    }

    /// Gets the alias source XLSX path
    pub fn get_alias_source_xlsx(&self, name: &str) -> PathBuf {
        self.get_sound_alias_dir().join(format!("{}.xlsx", name))
    }

    /// Gets the alias source CSV path
    pub fn get_alias_source_csv(&self, name: &str) -> PathBuf {
        self.get_sound_alias_dir().join(format!("{}.csv", name))
    }

    /// Gets the ship alias source CSV path
    pub fn get_ship_alias_source_csv(&self, name: &str) -> PathBuf {
        self.get_sound_ship_alias_dir().join(format!("{}.csv", name))
    }

    /// Gets the ambient source CSV path
    pub fn get_ambient_source_csv(&self, name: &str) -> PathBuf {
        self.get_sound_ambient_dir().join(format!("{}.csv", name))
    }

    /// Gets the ship ambient source CSV path
    pub fn get_ship_ambient_source_csv(&self, name: &str) -> PathBuf {
        self.get_sound_ship_alias_dir().join(format!("{}.csv", name))
    }

    /// Gets the sound global CSV path
    pub fn get_sound_global_csv(&self, name: &str) -> PathBuf {
        let path = self.get_sound_globals_dir().join(format!("{}.csv", name));
        self.get_csv_or_xls(&path)
    }

    /// Gets the sound ship global CSV path
    pub fn get_sound_ship_global_csv(&self, name: &str) -> PathBuf {
        let path = self.get_sound_ship_globals_dir().join("globals").join(format!("{}.csv", name));
        self.get_csv_or_xls(&path)
    }

    /// Gets the source asset directory
    pub fn get_source_asset_dir(&self, localized: bool) -> PathBuf {
        if localized {
            self.get_game_dir().join("soundloc_assets")
        } else {
            self.get_game_dir().join("sound_assets")
        }
    }

    /// Gets the SoX directory
    pub fn get_sox_dir(&self) -> PathBuf {
        self.get_sound_bin_dir().join("sox-14.3.2")
    }

    /// Gets the music source path
    pub fn get_music_source_path(&self) -> PathBuf {
        self.get_game_dir().join("../source_music")
    }

    /// Gets the music target path
    pub fn get_music_target_path(&self) -> PathBuf {
        self.get_source_asset_dir(false).join("mus")
    }

    /// Gets the music directory
    pub fn get_music_dir(&self) -> PathBuf {
        self.get_sound_dir().join("music")
    }

    /// Gets the build changelist filename
    pub fn get_build_cl_filename(&self) -> PathBuf {
        self.get_sound_dir().join("buildcl.txt")
    }

    /// Gets the zone source .zone file path
    pub fn get_zone_source_zone(&self, zone: &str) -> PathBuf {
        self.source_dir.join("zone_source").join(format!("{}.zone", zone))
    }

    /// Gets the zone source .zpkg file path
    pub fn get_zone_source_pkg(&self, zone: &str) -> PathBuf {
        self.get_zone_source_dir().join(format!("{}.zpkg", zone))
    }

    /// Gets the dialog directory
    pub fn get_dialog_dir(&self) -> PathBuf {
        self.get_game_dir().join("../source_dialog")
    }

    /// Gets the anim directory
    pub fn get_anim_dir(&self) -> PathBuf {
        self.get_game_dir().join("../source_anim")
    }
}

fn env_path(key: &str) -> Result<PathBuf, String> {
    std::env::var(key)
        .map(PathBuf::from)
        .map_err(|_| format!("{} environment variable not set", key))
}