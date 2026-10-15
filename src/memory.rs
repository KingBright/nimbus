use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
#[serde(default)]
pub struct PetMemory {
    // app name -> active count
    pub app_usage: HashMap<String, u32>,
    // hour -> count
    pub active_hours: HashMap<u32, u32>,
    // interaction count
    pub total_interactions: u32,
    #[serde(default)]
    pub touch_count: u32,
    #[serde(default)]
    pub dream_count: u32,
    #[serde(default)]
    pub growth_points: u32,
    #[serde(default)]
    pub self_habits: Vec<String>,
    #[serde(default)]
    pub personality_notes: Vec<String>,
    #[serde(default)]
    pub recent_lines: Vec<String>,
}

impl PetMemory {
    pub fn get_path() -> PathBuf {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
        PathBuf::from(home).join(".nimbus_memory.json")
    }

    pub fn load_or_create() -> Self {
        Self::load_from_path(&Self::get_path())
    }

    fn load_from_path(path: &Path) -> Self {
        match fs::read_to_string(path) {
            Ok(content) => match serde_json::from_str::<Self>(&content) {
                Ok(mut memory) => {
                    if memory.normalize() {
                        if let Err(error) = memory.save_to_path(path) {
                            eprintln!("[Nimbus] 记忆整理后保存失败：{error}");
                        }
                    }
                    memory
                }
                Err(_) => {
                    eprintln!("[Nimbus] 记忆格式无效，原文件已保留：{}", path.display());
                    Self::default()
                }
            },
            Err(error) if error.kind() == io::ErrorKind::NotFound => Self::default(),
            Err(error) => {
                eprintln!("[Nimbus] 无法读取记忆，原文件未改动：{error}");
                Self::default()
            }
        }
    }

    fn save_to_path(&self, path: &Path) -> io::Result<()> {
        let contents =
            serde_json::to_vec_pretty(self).map_err(|_| io::Error::other("记忆序列化失败"))?;
        crate::storage::atomic_write(path, &contents, |existing| {
            serde_json::from_str::<Self>(existing).is_ok()
        })
    }

    pub fn save(&self) {
        if let Err(error) = self.save_to_path(&Self::get_path()) {
            eprintln!("[Nimbus] 记忆保存失败：{error}");
        }
    }

    pub fn clear(&mut self) {
        self.app_usage.clear();
        self.active_hours.clear();
        self.total_interactions = 0;
        self.touch_count = 0;
        self.dream_count = 0;
        self.growth_points = 0;
        self.self_habits.clear();
        self.personality_notes.clear();
        self.recent_lines.clear();
        self.save();
    }

    pub fn record_app(&mut self, app: &str, hour: u32) {
        if is_invalid_memory_app(app) || hour > 23 {
            return;
        }
        let counter = self.app_usage.entry(app.to_string()).or_insert(0);
        *counter += 1;
        let app_count = *counter;

        let hour_counter = self.active_hours.entry(hour).or_insert(0);
        *hour_counter += 1;

        if app_count == 3 || app_count.is_multiple_of(12) {
            self.remember_self_habit(format!("对方常在 {app} 里停留，我会把陪伴语气放轻一点。"));
        }

        self.save();
    }

    pub fn increment_interactions(&mut self) {
        self.total_interactions += 1;
        self.growth_points = self.growth_points.saturating_add(2);
        if self.total_interactions == 3 || self.total_interactions.is_multiple_of(10) {
            self.remember_self_habit("对方会主动和我聊天，我要延续前后的情绪和话题。");
        }
        self.save();
    }

    pub fn increment_touch(&mut self) {
        self.touch_count += 1;
        self.growth_points = self.growth_points.saturating_add(1);
        if self.touch_count == 3 || self.touch_count.is_multiple_of(8) {
            self.remember_self_habit("被摸到身体时，我会先确认这是互动，再做柔和回应。");
        }
        self.save();
    }

    #[allow(dead_code)]
    pub fn increment_dreams(&mut self) {
        self.dream_count += 1;
        self.growth_points = self.growth_points.saturating_add(1);
        if self.dream_count == 2 || self.dream_count.is_multiple_of(6) {
            self.remember_self_habit("睡着时我也会保留一点陪伴感，但梦话要轻，不打扰对方。");
        }
        self.save();
    }

    pub fn add_personality_note(&mut self, note: impl Into<String>) {
        if self.remember_personality_note(note) {
            self.save();
        }
    }

    fn remember_personality_note(&mut self, note: impl Into<String>) -> bool {
        let note = note.into().trim().to_string();
        if note.is_empty()
            || self
                .personality_notes
                .iter()
                .any(|existing| existing == &note)
        {
            return false;
        }
        self.personality_notes.push(note);
        if self.personality_notes.len() > 8 {
            let excess = self.personality_notes.len() - 8;
            self.personality_notes.drain(0..excess);
        }
        true
    }

    fn remember_self_habit(&mut self, habit: impl Into<String>) -> bool {
        let habit = clean_memory_line(&habit.into());
        if habit.is_empty() || self.self_habits.iter().any(|existing| existing == &habit) {
            return false;
        }
        self.self_habits.push(habit);
        if self.self_habits.len() > 8 {
            let excess = self.self_habits.len() - 8;
            self.self_habits.drain(0..excess);
        }
        true
    }

    pub fn record_spoken_line(&mut self, line: &str) {
        if self.remember_spoken_line(line) {
            self.save();
        }
    }

    fn remember_spoken_line(&mut self, line: &str) -> bool {
        let line = clean_memory_line(line);
        if line.is_empty() {
            return false;
        }

        self.recent_lines.retain(|existing| existing != &line);
        self.recent_lines.push(line);
        if self.recent_lines.len() > 12 {
            let excess = self.recent_lines.len() - 12;
            self.recent_lines.drain(0..excess);
        }

        let mut changed = true;
        if self.recent_lines.len() >= 6 {
            changed |= self
                .remember_personality_note("我会记住自己最近说过的话，主动换新的说法和小动作。");
            changed |= self.remember_self_habit("我说过的话会留下痕迹，下一句要主动换角度。");
        }

        changed
    }

    pub fn recent_lines(&self, limit: usize) -> Vec<String> {
        self.recent_lines
            .iter()
            .rev()
            .take(limit)
            .cloned()
            .collect()
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

    pub fn describe_for_prompt(&self) -> String {
        let top_apps = self.get_top_apps();
        let top_hours = self.get_top_hours();
        let habits = if top_apps.is_empty() {
            "你还没有关于屏幕前这个人的日常偏好记忆。".to_string()
        } else {
            format!(
                "你记得对方的日常偏好：最常打开的应用是 [{}]，通常在 [{}] 点最活跃。",
                top_apps.join(", "),
                top_hours
                    .iter()
                    .map(|h| format!("{}点", h))
                    .collect::<Vec<_>>()
                    .join("/")
            )
        };

        let notes = if self.personality_notes.is_empty() {
            "你仍在形成自己的性格画像：温柔、机灵、会观察屏幕前的状态。".to_string()
        } else {
            let notes = self
                .personality_notes
                .iter()
                .map(|note| prompt_safe_memory_text(note))
                .collect::<Vec<_>>();
            format!("你的自我性格笔记：{}。", notes.join("；"))
        };
        let growth = format!(
            "你的成长阶段：{}；自己的习惯：{}。",
            self.growth_stage(),
            self.self_habit_summary()
        );
        let recent_lines = self
            .recent_lines(5)
            .iter()
            .map(|line| prompt_safe_memory_text(line))
            .collect::<Vec<_>>();
        let recent = if recent_lines.is_empty() {
            "你还没有稳定的近期口头表达记忆。".to_string()
        } else {
            format!(
                "你记得自己最近说过：{}。接下来要避免重复这些句式，换新的动作、比喻或语气。",
                recent_lines.join(" / ")
            )
        };

        format!(
            "{habits} 你和对方的互动次数：{}，被摸摸次数：{}，梦话次数：{}。{growth}{notes}{recent}",
            self.total_interactions, self.touch_count, self.dream_count
        )
    }

    fn growth_stage(&self) -> &'static str {
        match self.growth_points {
            0..=5 => "刚醒来，正在认识屏幕前的人",
            6..=24 => "开始熟悉对方的节奏",
            25..=79 => "形成了稳定的陪伴习惯",
            _ => "已经会根据共同经历调整表达",
        }
    }

    fn self_habit_summary(&self) -> String {
        if self.self_habits.is_empty() {
            "还在观察，不要固定表演；每次回应都先结合当前事件和记忆。".to_string()
        } else {
            self.self_habits
                .iter()
                .rev()
                .take(4)
                .map(|habit| prompt_safe_memory_text(habit))
                .collect::<Vec<_>>()
                .join("；")
        }
    }

    fn normalize(&mut self) -> bool {
        let mut changed = false;

        let app_len = self.app_usage.len();
        self.app_usage
            .retain(|app, _| !is_invalid_memory_app(app.as_str()));
        changed |= self.app_usage.len() != app_len;

        let hour_len = self.active_hours.len();
        self.active_hours.retain(|hour, _| *hour <= 23);
        changed |= self.active_hours.len() != hour_len;

        let original_notes = self.personality_notes.clone();
        let mut seen = HashSet::new();
        self.personality_notes.retain(|note| {
            let trimmed = note.trim();
            !trimmed.is_empty() && seen.insert(trimmed.to_string())
        });
        if self.personality_notes.len() > 8 {
            let excess = self.personality_notes.len() - 8;
            self.personality_notes.drain(0..excess);
        }
        changed |= self.personality_notes != original_notes;

        let original_habits = self.self_habits.clone();
        let mut seen_habits = HashSet::new();
        self.self_habits = self
            .self_habits
            .iter()
            .map(|habit| {
                let cleaned = clean_memory_line(habit);
                cleaned.replace("云朵本体", "身体").replace("云朵", "我")
            })
            .filter(|habit| !habit.is_empty())
            .filter(|habit| seen_habits.insert(habit.clone()))
            .collect();
        if self.self_habits.len() > 8 {
            let excess = self.self_habits.len() - 8;
            self.self_habits.drain(0..excess);
        }
        changed |= self.self_habits != original_habits;

        let original_seeded_habits_len = self.self_habits.len();
        if self.total_interactions >= 3 {
            self.remember_self_habit("对方会主动和我聊天，我要延续前后的情绪和话题。");
        }
        if self.touch_count >= 3 {
            self.remember_self_habit("被摸到身体时，我会先确认这是互动，再做柔和回应。");
        }
        if self.dream_count >= 2 {
            self.remember_self_habit("睡着时我也会保留一点陪伴感，但梦话要轻，不打扰对方。");
        }
        if let Some(app) = self.get_top_apps().into_iter().next() {
            self.remember_self_habit(format!("对方常在 {app} 里停留，我会把陪伴语气放轻一点。"));
        }
        changed |= self.self_habits.len() != original_seeded_habits_len;

        let derived_growth = self
            .total_interactions
            .saturating_mul(2)
            .saturating_add(self.touch_count)
            .saturating_add(self.dream_count);
        if self.growth_points < derived_growth {
            self.growth_points = derived_growth;
            changed = true;
        }

        let original_lines = self.recent_lines.clone();
        self.recent_lines = self
            .recent_lines
            .iter()
            .map(|line| {
                let cleaned = clean_memory_line(line);
                cleaned
                    .replace("云朵本体", "身体")
                    .replace("小云朵", "我")
                    .replace("云朵", "我")
            })
            .filter(|line| !line.is_empty())
            .collect();
        let mut seen_lines = HashSet::new();
        self.recent_lines
            .retain(|line| seen_lines.insert(line.clone()));
        if self.recent_lines.len() > 12 {
            let excess = self.recent_lines.len() - 12;
            self.recent_lines.drain(0..excess);
        }
        changed |= self.recent_lines != original_lines;

        changed
    }
}

fn clean_memory_line(line: &str) -> String {
    let line = line
        .trim()
        .trim_matches(['"', '“', '”', '\'', '。', '，', ',', ' ']);
    let line = line.chars().take(48).collect::<String>();
    if is_low_signal_memory_line(&line) {
        String::new()
    } else {
        line
    }
}

fn memory_line_key(line: &str) -> String {
    line.chars()
        .filter(|ch| {
            !ch.is_whitespace()
                && !matches!(
                    ch,
                    '，' | '。'
                        | '！'
                        | '？'
                        | ','
                        | '!'
                        | '?'
                        | '~'
                        | '～'
                        | '、'
                        | ':'
                        | '：'
                        | '"'
                        | '“'
                        | '”'
                        | '\''
                )
        })
        .collect()
}

fn prompt_safe_memory_text(line: &str) -> String {
    line.replace("主人大人", "对方")
        .replace("主人的", "对方的")
        .replace("主人", "对方")
}

fn is_low_signal_memory_line(line: &str) -> bool {
    let key = memory_line_key(line);
    if key.is_empty() {
        return true;
    }

    if key.contains("对话记录")
        || key.contains("人设设定")
        || key.contains("性格是")
        || (key.contains("我是一只") && key.contains("性格"))
    {
        return true;
    }

    let meaningful: String = key
        .chars()
        .filter(|ch| {
            !matches!(
                ch,
                '呀' | '啊'
                    | '呢'
                    | '哦'
                    | '喔'
                    | '啦'
                    | '哇'
                    | '哈'
                    | '嘿'
                    | '嗨'
                    | '欸'
                    | '诶'
                    | '唔'
                    | '嗯'
            )
        })
        .collect();

    matches!(meaningful.as_str(), "主人" | "主人大人")
        || (meaningful.contains("主人") && meaningful.chars().count() <= 4)
}

fn is_invalid_memory_app(app: &str) -> bool {
    app.is_empty()
        || app == "未知应用"
        || app.eq_ignore_ascii_case("nimbus")
        || app.chars().all(|c| c.is_ascii_digit())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::test_support::TempDir;

    #[test]
    fn loads_partial_old_memory_without_losing_existing_counts() {
        let dir = TempDir::new();
        let path = dir.path("memory.json");
        fs::write(&path, r#"{"touch_count": 4}"#).unwrap();
        let memory = PetMemory::load_from_path(&path);
        assert_eq!(memory.touch_count, 4);
        assert_eq!(memory.growth_points, 4);
        assert!(memory.app_usage.is_empty());
        let saved: PetMemory = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
        assert_eq!(saved.touch_count, 4);
    }

    #[test]
    fn retains_invalid_memory_until_save_then_preserves_a_backup() {
        let dir = TempDir::new();
        let path = dir.path("memory.json");
        let damaged = b"{\"touch_count\": 7, ";
        fs::write(&path, damaged).unwrap();
        let memory = PetMemory::load_from_path(&path);
        assert_eq!(fs::read(&path).unwrap(), damaged);
        memory.save_to_path(&path).unwrap();
        assert_eq!(fs::read(dir.path("memory.json.invalid")).unwrap(), damaged);
        assert!(serde_json::from_slice::<PetMemory>(&fs::read(path).unwrap()).is_ok());
    }

    #[test]
    fn ignores_numeric_app_names() {
        assert!(is_invalid_memory_app("0"));
        assert!(is_invalid_memory_app("未知应用"));
        assert!(is_invalid_memory_app("nimbus"));
        assert!(!is_invalid_memory_app("Code"));
    }

    #[test]
    fn describes_personality_context() {
        let memory = PetMemory {
            personality_notes: vec!["喜欢在主人写代码时安静陪伴".to_string()],
            ..Default::default()
        };
        let desc = memory.describe_for_prompt();
        assert!(desc.contains("性格笔记"));
        assert!(desc.contains("安静陪伴"));
        assert!(!desc.contains("主人"));
        assert!(desc.contains("对方"));
    }

    #[test]
    fn normalizes_stale_memory_entries() {
        let mut memory = PetMemory {
            app_usage: HashMap::from([
                ("nimbus".to_string(), 3),
                ("0".to_string(), 1),
                ("Code".to_string(), 2),
            ]),
            active_hours: HashMap::from([(1, 2), (99, 1)]),
            personality_notes: vec![
                "我会在睡着时说很轻的梦话，但不打扰主人。".to_string(),
                "我会在睡着时说很轻的梦话，但不打扰主人。".to_string(),
                "  ".to_string(),
            ],
            touch_count: 3,
            ..Default::default()
        };

        assert!(memory.normalize());
        assert_eq!(memory.app_usage.len(), 1);
        assert!(memory.app_usage.contains_key("Code"));
        assert_eq!(memory.active_hours.len(), 1);
        assert_eq!(memory.personality_notes.len(), 1);
        assert!(memory.growth_points > 0);
        assert!(!memory.self_habits.is_empty());
    }

    #[test]
    fn remembers_recent_spoken_lines_without_duplicates() {
        let mut memory = PetMemory::default();
        memory.remember_spoken_line("云朵轻轻蹭蹭主人。");
        memory.remember_spoken_line("云朵轻轻蹭蹭主人");
        memory.remember_spoken_line("我换个姿势陪你");

        assert_eq!(
            memory.recent_lines(3),
            vec!["我换个姿势陪你".to_string(), "云朵轻轻蹭蹭主人".to_string()]
        );
        assert!(memory.describe_for_prompt().contains("最近说过"));
        assert!(!memory.describe_for_prompt().contains("蹭蹭主人"));
        assert!(memory.describe_for_prompt().contains("蹭蹭对方"));
    }

    #[test]
    fn drops_low_signal_recent_lines_during_memory_cleanup() {
        let mut memory = PetMemory {
            recent_lines: vec![
                "主人！".to_string(),
                "嗨，主人！".to_string(),
                "Nimbus的对话记录： 主人！".to_string(),
                "我是一只软萌活泼的Nimbus，性格温柔体贴".to_string(),
                "安静陪伴你，专注又放松".to_string(),
            ],
            ..Default::default()
        };

        assert!(memory.normalize());
        assert_eq!(
            memory.recent_lines,
            vec!["安静陪伴你，专注又放松".to_string()]
        );
    }

    #[test]
    fn describes_growth_and_self_habits() {
        let memory = PetMemory {
            growth_points: 30,
            self_habits: vec!["说过的话会留下痕迹，下一句要主动换角度".to_string()],
            ..Default::default()
        };
        let desc = memory.describe_for_prompt();
        assert!(desc.contains("成长阶段"));
        assert!(desc.contains("稳定的陪伴习惯"));
        assert!(desc.contains("主动换角度"));
    }
}
