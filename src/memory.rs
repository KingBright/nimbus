use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::fs;

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct PetMemory {
    // app name -> active count
    pub app_usage: HashMap<String, u32>,
    // hour -> count
    pub active_hours: HashMap<u32, u32>,
    // interaction count
    pub total_interactions: u32,
}

impl PetMemory {
    pub fn get_path() -> PathBuf {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
        PathBuf::from(home).join(".nimbus_memory.json")
    }

    pub fn load_or_create() -> Self {
        let path = Self::get_path();
        if path.exists() {
            if let Ok(content) = fs::read_to_string(&path) {
                if let Ok(memory) = serde_json::from_str(&content) {
                    return memory;
                }
            }
        }
        Self::default()
    }

    pub fn save(&self) {
        let path = Self::get_path();
        if let Ok(json_str) = serde_json::to_string_pretty(self) {
            let _ = fs::write(&path, json_str);
        }
    }

    pub fn clear(&mut self) {
        self.app_usage.clear();
        self.active_hours.clear();
        self.total_interactions = 0;
        self.save();
    }

    pub fn record_app(&mut self, app: &str, hour: u32) {
        if app.is_empty() || app == "未知应用" {
            return;
        }
        let counter = self.app_usage.entry(app.to_string()).or_insert(0);
        *counter += 1;

        let hour_counter = self.active_hours.entry(hour).or_insert(0);
        *hour_counter += 1;
        
        self.save();
    }

    pub fn increment_interactions(&mut self) {
        self.total_interactions += 1;
        self.save();
    }

    pub fn get_top_apps(&self) -> Vec<String> {
        let mut apps: Vec<_> = self.app_usage.iter().collect();
        apps.sort_by(|a, b| b.1.cmp(a.1));
        apps.into_iter().take(3).map(|(k, _)| k.clone()).collect()
    }

    pub fn get_top_hours(&self) -> Vec<u32> {
        let mut hours: Vec<_> = self.active_hours.iter().collect();
        hours.sort_by(|a, b| b.1.cmp(a.1));
        hours.into_iter().take(2).map(|(&k, _)| k).collect()
    }
}
