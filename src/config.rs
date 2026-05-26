use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::fs;

#[derive(Debug, Serialize, Deserialize, Clone)]
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
    #[serde(default = "default_font_size")]
    pub font_size: f32,
    #[serde(default = "default_pet_name")]
    pub pet_name: String,
}

fn default_persona() -> String {
    "default".to_string()
}
fn default_mutter_enabled() -> bool {
    true
}
fn default_font_size() -> f32 {
    24.0
}
fn default_pet_name() -> String {
    "Nimbus".to_string()
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
            font_size: default_font_size(),
            pet_name: default_pet_name(),
        }
    }
}

impl AppConfig {
    pub fn load_or_create() -> Self {
        let config_path = PathBuf::from("config.toml");
        if config_path.exists() {
            if let Ok(content) = fs::read_to_string(&config_path) {
                if let Ok(config) = toml::from_str(&content) {
                    return config;
                }
            }
        }
        
        let default_config = Self::default();
        if let Ok(toml_str) = toml::to_string_pretty(&default_config) {
            let _ = fs::write(&config_path, toml_str);
        }
        default_config
    }

    pub fn save(&self) {
        let config_path = PathBuf::from("config.toml");
        if let Ok(toml_str) = toml::to_string_pretty(self) {
            let _ = fs::write(&config_path, toml_str);
        }
    }
}
