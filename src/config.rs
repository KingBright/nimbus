use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct AppConfig {
    pub model_path: Option<PathBuf>,
    pub adapter_path: Option<PathBuf>,
    pub llama_server_port: u16,
    pub pet_size: u32,
    pub always_on_top: bool,
    pub click_through_when_idle: bool,
    pub idle_offload_secs: u64,
    pub bubble_timeout_secs: u64,
    pub expose_think: bool,
    #[serde(default = "default_persona")]
    pub persona: String,
    #[serde(default = "default_mutter_enabled")]
    pub mutter_enabled: bool,
    #[serde(default = "default_touch_response_cooldown_secs")]
    pub touch_response_cooldown_secs: u64,
    #[serde(default = "default_mutter_interval_secs")]
    pub mutter_interval_secs: u64,
    #[serde(default = "default_dream_interval_secs")]
    pub dream_interval_secs: u64,
    #[serde(default = "default_font_size")]
    pub font_size: f32,
    #[serde(default = "default_pet_name")]
    pub pet_name: String,
    #[serde(default = "default_pet_style")]
    pub pet_style: String,
    #[serde(default = "default_weather_mode")]
    pub weather_mode: String,
}

fn default_persona() -> String {
    "default".to_string()
}
fn default_mutter_enabled() -> bool {
    true
}
fn default_touch_response_cooldown_secs() -> u64 {
    15
}
fn default_mutter_interval_secs() -> u64 {
    180
}
fn default_dream_interval_secs() -> u64 {
    75
}
fn default_font_size() -> f32 {
    24.0
}
fn default_pet_name() -> String {
    "Nimbus".to_string()
}
fn default_pet_style() -> String {
    "nimbus".to_string()
}
fn default_weather_mode() -> String {
    "sunny".to_string()
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            model_path: None,
            adapter_path: None,
            llama_server_port: 18766,
            pet_size: 200,
            always_on_top: true,
            click_through_when_idle: true,
            idle_offload_secs: 30,
            bubble_timeout_secs: 10,
            expose_think: true,
            persona: default_persona(),
            mutter_enabled: default_mutter_enabled(),
            touch_response_cooldown_secs: default_touch_response_cooldown_secs(),
            mutter_interval_secs: default_mutter_interval_secs(),
            dream_interval_secs: default_dream_interval_secs(),
            font_size: default_font_size(),
            pet_name: default_pet_name(),
            pet_style: default_pet_style(),
            weather_mode: default_weather_mode(),
        }
    }
}

impl AppConfig {
    fn get_config_path() -> PathBuf {
        if let Ok(home) = std::env::var("HOME") {
            PathBuf::from(home).join(".nimbus_config.toml")
        } else {
            PathBuf::from("config.toml")
        }
    }

    pub fn load_or_create() -> Self {
        Self::load_from_paths(&Self::get_config_path(), Path::new("config.toml"))
    }

    fn load_from_paths(global_path: &Path, local_path: &Path) -> Self {
        match fs::read_to_string(global_path) {
            Ok(content) => match toml::from_str::<Self>(&content) {
                Ok(mut config) => {
                    config.normalize();
                    return config;
                }
                Err(_) => {
                    eprintln!(
                        "[Nimbus] 配置格式无效，暂用默认设置；原文件已保留：{}",
                        global_path.display()
                    );
                    return Self::default();
                }
            },
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => {
                eprintln!("[Nimbus] 无法读取配置，原文件未改动：{error}");
                return Self::default();
            }
        }

        // Only migrate a local configuration when the global file is absent.
        let mut config = match fs::read_to_string(local_path) {
            Ok(content) => toml::from_str::<Self>(&content).unwrap_or_else(|_| {
                eprintln!(
                    "[Nimbus] 本地配置格式无效，原文件已保留：{}",
                    local_path.display()
                );
                Self::default()
            }),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Self::default(),
            Err(error) => {
                eprintln!("[Nimbus] 无法读取旧配置：{error}");
                Self::default()
            }
        };
        config.normalize();
        if let Err(error) = config.save_to_path(global_path) {
            eprintln!("[Nimbus] 无法创建全局配置：{error}");
        }
        config
    }

    fn normalize(&mut self) {
        self.pet_size = self.pet_size.clamp(64, 512);
        self.idle_offload_secs = self.idle_offload_secs.clamp(5, 86_400);
        self.bubble_timeout_secs = self.bubble_timeout_secs.clamp(1, 300);
        self.touch_response_cooldown_secs = self.touch_response_cooldown_secs.clamp(1, 86_400);
        self.mutter_interval_secs = self.mutter_interval_secs.clamp(15, 86_400);
        self.dream_interval_secs = self.dream_interval_secs.clamp(15, 86_400);
        self.font_size = if self.font_size.is_finite() {
            self.font_size.clamp(12.0, 48.0)
        } else {
            default_font_size()
        };
        if !matches!(
            self.persona.as_str(),
            "default" | "tsundere" | "toxic" | "moe"
        ) {
            self.persona = default_persona();
        }
        if !matches!(
            self.pet_style.as_str(),
            "nimbus" | "mochi" | "starlit" | "crab" | "luma"
        ) {
            self.pet_style = default_pet_style();
        }
        if !matches!(
            self.weather_mode.as_str(),
            "sunny" | "windy" | "rainy" | "snowy"
        ) {
            self.weather_mode = default_weather_mode();
        }
    }

    fn save_to_path(&self, path: &Path) -> io::Result<()> {
        let mut config = self.clone();
        config.normalize();
        let contents =
            toml::to_string_pretty(&config).map_err(|_| io::Error::other("配置序列化失败"))?;
        crate::storage::atomic_write(path, contents.as_bytes(), |existing| {
            toml::from_str::<Self>(existing).is_ok()
        })
    }

    pub fn save(&self) {
        if let Err(error) = self.save_to_path(&Self::get_config_path()) {
            eprintln!("[Nimbus] 配置保存失败：{error}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::test_support::TempDir;

    #[test]
    fn fills_all_missing_settings_in_minimal_old_config() {
        let config: AppConfig = toml::from_str("pet_name = 'Old friend'").unwrap();
        assert_eq!(config.pet_name, "Old friend");
        assert_eq!(config.pet_size, 200);
        assert_eq!(config.idle_offload_secs, 30);
        assert!(config.mutter_enabled);
    }

    #[test]
    fn normalizes_unsafe_settings_when_loading() {
        let dir = TempDir::new();
        let path = dir.path("config.toml");
        fs::write(&path, "pet_size = 4294967295\nidle_offload_secs = 0\nfont_size = nan\nmutter_interval_secs = 9223372036854775807\npet_style = 'unknown'\npersona = 'unknown'\nweather_mode = 'unknown'").unwrap();
        let config = AppConfig::load_from_paths(&path, &dir.path("missing.toml"));
        assert_eq!(config.pet_size, 512);
        assert_eq!(config.idle_offload_secs, 5);
        assert_eq!(config.font_size, 24.0);
        assert_eq!(config.mutter_interval_secs, 86_400);
        assert_eq!(config.pet_style, "nimbus");
        assert_eq!(config.persona, "default");
        assert_eq!(config.weather_mode, "sunny");
    }

    #[test]
    fn retains_corrupt_global_config_instead_of_migrating_over_it() {
        let dir = TempDir::new();
        let global_path = dir.path("global.toml");
        let local_path = dir.path("local.toml");
        fs::write(&global_path, "model_path = [broken").unwrap();
        fs::write(&local_path, "pet_name = 'Legacy'").unwrap();
        let config = AppConfig::load_from_paths(&global_path, &local_path);
        assert_eq!(config.pet_name, "Nimbus");
        assert_eq!(
            fs::read_to_string(&global_path).unwrap(),
            "model_path = [broken"
        );

        config.save_to_path(&global_path).unwrap();
        assert_eq!(
            fs::read_to_string(dir.path("global.toml.invalid")).unwrap(),
            "model_path = [broken"
        );
        assert!(toml::from_str::<AppConfig>(&fs::read_to_string(global_path).unwrap()).is_ok());
    }

    #[test]
    fn migrates_old_config_only_when_global_file_is_missing() {
        let dir = TempDir::new();
        let global_path = dir.path("global.toml");
        let local_path = dir.path("local.toml");
        fs::write(
            &local_path,
            "model_path = '/models/companion.gguf'\npet_name = 'Legacy'",
        )
        .unwrap();
        let config = AppConfig::load_from_paths(&global_path, &local_path);
        assert_eq!(config.pet_name, "Legacy");
        assert_eq!(
            config.model_path,
            Some(PathBuf::from("/models/companion.gguf"))
        );
        assert!(global_path.exists());
    }

    #[test]
    fn fills_frequency_defaults_for_old_config_files() {
        let config: AppConfig = toml::from_str(
            r#"
llama_server_port = 18766
pet_size = 200
always_on_top = true
click_through_when_idle = true
idle_offload_secs = 30
bubble_timeout_secs = 10
expose_think = false
"#,
        )
        .unwrap();

        assert_eq!(config.touch_response_cooldown_secs, 15);
        assert_eq!(config.mutter_interval_secs, 180);
        assert_eq!(config.dream_interval_secs, 75);
    }
}
