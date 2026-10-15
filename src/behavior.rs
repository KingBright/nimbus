use std::collections::HashSet;
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BehaviorEvent {
    Startup,
    Touch,
    Ambient,
    Dream,
    Renamed,
    PersonaShift,
    BodyChanged,
    WeatherChanged,
    MemoryReset,
}

impl BehaviorEvent {
    pub fn priority(self) -> u8 {
        match self {
            Self::Startup => 5,
            Self::Touch => 4,
            Self::Renamed
            | Self::PersonaShift
            | Self::BodyChanged
            | Self::WeatherChanged
            | Self::MemoryReset => 3,
            Self::Dream => 2,
            Self::Ambient => 1,
        }
    }

    pub fn display_duration(self) -> Duration {
        match self {
            Self::Touch => Duration::from_secs(7),
            Self::Dream => Duration::from_secs(8),
            Self::Startup
            | Self::Ambient
            | Self::Renamed
            | Self::PersonaShift
            | Self::BodyChanged
            | Self::WeatherChanged
            | Self::MemoryReset => Duration::from_secs(10),
        }
    }

    pub fn max_retries(self) -> u8 {
        if self == Self::Startup {
            4
        } else {
            2
        }
    }
}

#[derive(Debug, Clone)]
pub struct BehaviorContext {
    pub pet_style: String,
    pub persona: String,
    pub weather: String,
    pub app: String,
    pub hour: u32,
    pub memory_summary: String,
    pub recent_lines: Vec<String>,
}

pub fn build_behavior_prompt(
    event: BehaviorEvent,
    context: &BehaviorContext,
    clean_retry: bool,
) -> String {
    let event_desc = match event {
        BehaviorEvent::Startup => {
            "你刚睁开眼，今天第一次见到屏幕前的人；主动打个招呼，表达重新见面和开始陪伴的心情"
        }
        BehaviorEvent::Touch => "屏幕前的人刚轻轻碰到你的身体；用自己的感受或小动作回应这次触碰",
        BehaviorEvent::Ambient => "安静陪伴了一会儿；自主做一个小动作，或说出此刻新发现的一点",
        BehaviorEvent::Dream => "你正在熟睡；把梦里刚出现的一个轻小片段说成梦话",
        BehaviorEvent::Renamed => "屏幕前的人刚给了你一个新称呼；自然回应并把它记在心里",
        BehaviorEvent::PersonaShift => "你的性格和说话方式刚有一点变化；自然地第一次试试新语气",
        BehaviorEvent::BodyChanged => "你刚换成现在这副身体；感受并活动自己的新身体",
        BehaviorEvent::WeatherChanged => "周围天气刚发生变化；说出自己的身体感受或反应",
        BehaviorEvent::MemoryReset => "记忆日记刚清空；准备从此刻重新认识屏幕前的人",
    };
    let style_desc = style_description(&context.pet_style);
    let recent = recent_avoidance(&context.recent_lines, &context.pet_style);

    if clean_retry {
        let response_goal = if event == BehaviorEvent::Startup {
            "直接用自己的口吻，对屏幕前的人说一句有见面感的问候，让他知道你已经醒来陪他"
        } else {
            "换一个具体而自然、没说过的小动作或感受，从正在发生的事情直接开口"
        };
        return format!(
            "你是桌面宠物，屏幕前的人是你陪伴的对象。身体动作属于你自己，用“我”表达；“你”只指屏幕前的人。此刻事件：{event_desc}。身体经验：{style_desc}。{response_goal}。用一句简短中文，在动作或话语完整结束后停下。"
        );
    }

    let persona = persona_description(&context.persona);
    let weather = weather_description(&context.weather);
    let memory = compact_text(
        &style_safe_memory(&context.memory_summary, &context.pet_style),
        96,
    );
    format!(
        "你是桌面宠物，屏幕前的人是你陪伴的对象。身体动作属于你自己，用“我”表达；“你”只指屏幕前的人。此刻事件：{event_desc}。身体经验：{style_desc}。性格正在长成{persona}。现在{context_hour}点，前台是{app}，周围是{weather}。结合记忆里真正相关的一点，自主选择一个动作、观察、念头或情绪来回应，像此刻真的活着；从正在发生的事情直接开口。记忆：{memory}。近期表达：{recent}。用一句自然简短的中文，在动作或话语完整结束后停下。",
        context_hour = context.hour,
        app = compact_text(&context.app, 24),
    )
}

pub fn clean_behavior_line(
    raw: &str,
    event: BehaviorEvent,
    pet_style: &str,
    hour: u32,
    recent_lines: &[String],
) -> Option<String> {
    raw.lines()
        .map(clean_line)
        .filter(|line| !line.is_empty())
        .filter(|line| line.chars().count() <= 42)
        .filter(|line| !has_unfinished_tail(line))
        .filter(|line| !line_is_prompt_echo(line))
        .filter(|line| !line_is_self_intro(line))
        .filter(|line| !style_identity_leaks(line, pet_style))
        .filter(|line| !assigns_pet_body_to_user(line, pet_style))
        .filter(|line| !has_unstable_repetition(line))
        .filter(|line| matches_event_intent(line, event))
        .filter(|line| event == BehaviorEvent::Dream || !is_sleep_or_farewell(line, hour))
        .find(|line| !is_too_similar(line, recent_lines))
}

pub fn jittered_interval(base_secs: u64, entropy: u128) -> Duration {
    let base = base_secs.max(5);
    let offset_percent = (entropy % 41) as i64 - 20;
    let adjusted = (base as i64 * (100 + offset_percent) / 100).max(5) as u64;
    Duration::from_secs(adjusted)
}

fn style_description(style: &str) -> &'static str {
    match style {
        "mochi" => "我的身体是一只抹茶小猫；耳朵、爪子和尾巴都属于我",
        "starlit" => "我的身体是一只星光章鱼；触角、星泡和漂浮感都属于我",
        "crab" => "我的身体是一只红螯小螃蟹；蟹壳和两只大螯都属于我",
        "luma" => "我的身体是一只月灯兔；长耳朵、圆尾巴和月牙光纹都属于我",
        _ => "我的身体是一朵蓬松小云；柔软的轮廓、小雨滴和漂浮感都属于我",
    }
}

fn persona_description(persona: &str) -> &'static str {
    match persona {
        "tsundere" => "嘴硬心软、有一点傲娇但会认真陪伴",
        "toxic" => "观察敏锐、会轻轻吐槽但不刻薄",
        "moe" => "柔软亲近、愿意表达依赖但不过分重复",
        _ => "温柔机灵、好奇并会从共同经历里变化",
    }
}

fn weather_description(weather: &str) -> &'static str {
    match weather {
        "windy" => "有风的空气",
        "rainy" => "轻轻落雨",
        "snowy" => "缓慢飘雪",
        _ => "温暖晴光",
    }
}

fn recent_avoidance(lines: &[String], pet_style: &str) -> String {
    let lines = lines
        .iter()
        .filter(|line| !style_identity_leaks(line, pet_style))
        .filter(|line| !assigns_pet_body_to_user(line, pet_style))
        .filter(|line| !has_unfinished_tail(line))
        .filter(|line| !line_is_prompt_echo(line))
        .filter(|line| normalized_key(line).chars().count() >= 6)
        .rev()
        .take(3)
        .map(|line| compact_text(line, 18))
        .collect::<Vec<_>>();
    if lines.is_empty() {
        return "还没有需要避开的旧句子".to_string();
    }
    lines.join(" / ")
}

fn style_safe_memory(memory: &str, pet_style: &str) -> String {
    let memory = memory
        .replace("Nimbus", "过去的旧称呼")
        .replace("NIMBUS", "过去的旧称呼")
        .replace("nimbus", "过去的旧称呼")
        .replace("灵霓", "过去的旧称呼");
    match pet_style {
        "crab" => memory
            .replace("云朵", "过去的身体")
            .replace("猫咪", "过去的身体")
            .replace("章鱼", "过去的身体"),
        "mochi" => memory
            .replace("螃蟹", "过去的身体")
            .replace("章鱼", "过去的身体")
            .replace("云朵", "过去的身体"),
        "starlit" => memory
            .replace("螃蟹", "过去的身体")
            .replace("猫咪", "过去的身体")
            .replace("云朵", "过去的身体"),
        "luma" => memory
            .replace("螃蟹", "过去的身体")
            .replace("猫咪", "过去的身体")
            .replace("章鱼", "过去的身体")
            .replace("云朵", "过去的身体"),
        _ => memory
            .replace("螃蟹", "过去的身体")
            .replace("猫咪", "过去的身体")
            .replace("章鱼", "过去的身体"),
    }
}

fn compact_text(text: &str, max_chars: usize) -> String {
    text.chars()
        .filter(|ch| *ch != '\n' && *ch != '\r')
        .take(max_chars)
        .collect()
}

fn clean_line(line: &str) -> String {
    let line = line
        .trim()
        .trim_matches(['"', '“', '”', '\'', '。', '，', ',', ' ', '　']);
    let line = strip_speaker_prefix(line);
    line.trim()
        .trim_matches(['"', '“', '”', '\'', ' ', '　'])
        .to_string()
}

fn has_unfinished_tail(line: &str) -> bool {
    let tail = line.trim_end_matches([
        '。', '！', '？', '!', '?', '~', '～', '…', '”', '"', '\'', '）', ')', ' ', '　',
    ]);
    [
        "我用",
        "我会",
        "我想",
        "我要",
        "想要",
        "准备用",
        "准备把",
        "轻轻把",
        "悄悄把",
        "然后",
        "但是",
        "不过",
        "因为",
        "所以",
        "如果",
        "只是想",
        "还想",
        "再用",
        "和",
        "与",
        "跟",
        "用",
        "把",
        "被",
        "在",
        "让",
        "给",
        "向",
        "对",
        "从",
        "到",
    ]
    .iter()
    .any(|ending| tail.ends_with(ending))
}

fn strip_speaker_prefix(line: &str) -> &str {
    let Some((prefix, rest)) = line.split_once([':', '：']) else {
        return line;
    };
    let prefix_key = prefix.to_lowercase();
    if prefix.chars().count() <= 12
        && (prefix_key.contains("nimbus")
            || prefix.contains("桌宠")
            || prefix.contains("小螃蟹")
            || prefix.contains("小猫")
            || prefix.contains("章鱼")
            || prefix.contains("月灯兔")
            || prefix.contains("兔子")
            || prefix.contains("云朵"))
    {
        rest
    } else {
        line
    }
}

fn line_is_prompt_echo(line: &str) -> bool {
    [
        "此刻事件",
        "当前身体",
        "当前外观",
        "近期表达",
        "只说一句",
        "台词",
        "设定",
        "提示词",
        "启动",
        "上线",
        "窗口",
        "程序",
        "模型",
        "显存",
        "内存",
        "已释放",
    ]
    .iter()
    .any(|needle| line.contains(needle))
}

fn line_is_self_intro(line: &str) -> bool {
    let key: String = line.chars().filter(|ch| !ch.is_whitespace()).collect();
    let lower = key.to_lowercase();
    lower.contains("nimbus")
        || key.contains("灵霓")
        || (key.starts_with("我是") && !key.starts_with("我是被"))
        || key.starts_with("我是桌宠")
        || key.starts_with("我叫")
        || key.contains("我的名字")
}

fn style_identity_leaks(line: &str, style: &str) -> bool {
    match style {
        "crab" => ["云朵", "小云", "猫咪", "喵", "章鱼", "触角"]
            .iter()
            .any(|word| line.contains(word)),
        "mochi" => ["螃蟹", "大螯", "蟹壳", "章鱼", "触角", "云朵"]
            .iter()
            .any(|word| line.contains(word)),
        "starlit" => ["螃蟹", "大螯", "蟹壳", "猫咪", "喵", "云朵"]
            .iter()
            .any(|word| line.contains(word)),
        "luma" => [
            "螃蟹", "大螯", "蟹壳", "猫咪", "喵", "章鱼", "触角", "云朵", "小云",
        ]
        .iter()
        .any(|word| line.contains(word)),
        _ => ["螃蟹", "大螯", "蟹壳", "猫咪", "喵", "章鱼", "触角"]
            .iter()
            .any(|word| line.contains(word)),
    }
}

fn assigns_pet_body_to_user(line: &str, style: &str) -> bool {
    let body_words: &[&str] = match style {
        "mochi" => &["猫耳", "耳朵", "猫爪", "爪子", "尾巴", "喵"],
        "starlit" => &["触角", "星泡", "漂浮"],
        "crab" => &["螃蟹", "蟹壳", "大螯", "螯钳", "横步", "泡泡"],
        "luma" => &["兔耳", "长耳朵", "圆尾巴", "月牙", "蹦跳"],
        _ => &["云朵", "小云", "雨滴", "漂浮", "变形"],
    };
    ["你在用", "你正用", "你会用", "你用", "你的"]
        .iter()
        .any(|prefix| {
            body_words
                .iter()
                .any(|body_word| line.contains(&format!("{prefix}{body_word}")))
        })
}

fn has_unstable_repetition(line: &str) -> bool {
    if line.contains('/') || line.contains('\\') {
        return true;
    }
    let chars: Vec<char> = line.chars().filter(|ch| !ch.is_whitespace()).collect();
    if chars.len() < 6 {
        return false;
    }
    (1..=3).any(|unit_len| {
        chars.windows(unit_len * 3).any(|window| {
            window[..unit_len] == window[unit_len..unit_len * 2]
                && window[..unit_len] == window[unit_len * 2..]
        })
    })
}

fn matches_event_intent(line: &str, event: BehaviorEvent) -> bool {
    if event != BehaviorEvent::Startup {
        return true;
    }

    [
        "嗨",
        "嘿",
        "哈喽",
        "你好",
        "早安",
        "午安",
        "晚上好",
        "早上好",
        "醒",
        "见到你",
        "见面",
        "来啦",
        "回来",
        "陪你",
        "在这里",
        "今天",
    ]
    .iter()
    .any(|signal| line.contains(signal))
}

fn is_sleep_or_farewell(line: &str, hour: u32) -> bool {
    if hour >= 23 || hour <= 5 {
        return false;
    }
    ["晚安", "睡觉", "睡着", "困了", "再见", "拜拜", "明天见"]
        .iter()
        .any(|word| line.contains(word))
}

fn is_too_similar(candidate: &str, recent_lines: &[String]) -> bool {
    let candidate_key = normalized_key(candidate);
    recent_lines.iter().any(|recent| {
        let recent_key = normalized_key(recent);
        if candidate_key.is_empty() || recent_key.is_empty() {
            return false;
        }
        if candidate_key == recent_key {
            return true;
        }
        if candidate_key.chars().count() >= 6
            && recent_key.chars().count() >= 6
            && (candidate_key.contains(&recent_key) || recent_key.contains(&candidate_key))
        {
            return true;
        }
        bigram_similarity(&candidate_key, &recent_key) >= 0.72
    })
}

fn normalized_key(text: &str) -> String {
    text.chars()
        .filter(|ch| {
            !ch.is_whitespace()
                && !matches!(
                    ch,
                    '，' | '。' | '！' | '？' | ',' | '!' | '?' | '~' | '～' | '、' | '：' | ':'
                )
        })
        .collect()
}

fn bigram_similarity(left: &str, right: &str) -> f32 {
    let left = bigrams(left);
    let right = bigrams(right);
    if left.is_empty() || right.is_empty() {
        return 0.0;
    }
    let intersection = left.intersection(&right).count() as f32;
    let union = left.union(&right).count() as f32;
    intersection / union
}

fn bigrams(text: &str) -> HashSet<String> {
    let chars: Vec<char> = text.chars().collect();
    chars
        .windows(2)
        .map(|window| window.iter().collect::<String>())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn context() -> BehaviorContext {
        BehaviorContext {
            pet_style: "crab".to_string(),
            persona: "tsundere".to_string(),
            weather: "sunny".to_string(),
            app: "Codex".to_string(),
            hour: 14,
            memory_summary: "对方经常写代码，我会安静陪伴；旧名字 Nimbus。".to_string(),
            recent_lines: vec!["我横着走两步，悄悄举起大螯".to_string()],
        }
    }

    #[test]
    fn rich_prompt_uses_event_memory_and_growth_context() {
        let prompt = build_behavior_prompt(BehaviorEvent::Touch, &context(), false);
        assert!(prompt.contains("屏幕前的人刚轻轻碰到你的身体"));
        assert!(prompt.contains("我的身体是一只红螯小螃蟹"));
        assert!(prompt.contains("记忆"));
        assert!(prompt.contains("身体动作属于你自己"));
        assert!(!prompt.contains("绝对不要"));
    }

    #[test]
    fn clean_retry_drops_memory_and_keeps_event() {
        let prompt = build_behavior_prompt(BehaviorEvent::Touch, &context(), true);
        assert!(prompt.contains("屏幕前的人刚轻轻碰到你的身体"));
        assert!(!prompt.contains("旧名字 Nimbus"));
        assert!(!prompt.contains("我横着走两步"));
        assert!(!prompt.contains("性格正在长成"));
    }

    #[test]
    fn rich_prompt_does_not_repeat_low_quality_recent_lines() {
        let mut ctx = context();
        ctx.recent_lines = vec![
            "谢谢啦～".to_string(),
            "你在用横步和泡泡回应我吗？".to_string(),
            "别逗我啦！你在用横步和".to_string(),
            "我把大螯轻轻收好".to_string(),
        ];

        let prompt = build_behavior_prompt(BehaviorEvent::Startup, &ctx, false);
        assert!(!prompt.contains("谢谢啦"));
        assert!(!prompt.contains("你在用横步"));
        assert!(prompt.contains("我把大螯轻轻收好"));
    }

    #[test]
    fn rich_prompt_neutralizes_stale_names_and_old_bodies() {
        let prompt = build_behavior_prompt(BehaviorEvent::Ambient, &context(), false);
        assert!(!prompt.contains("Nimbus"));
        assert!(prompt.contains("过去的旧称呼"));
    }

    #[test]
    fn startup_prompt_uses_living_moment_language_instead_of_system_terms() {
        let prompt = build_behavior_prompt(BehaviorEvent::Startup, &context(), false);
        assert!(prompt.contains("刚睁开眼"));
        assert!(prompt.contains("主动打个招呼"));
        assert!(!prompt.contains("窗口刚启动"));

        let retry = build_behavior_prompt(BehaviorEvent::Startup, &context(), true);
        assert!(retry.contains("直接用自己的口吻"));
        assert!(retry.contains("有见面感的问候"));
    }

    #[test]
    fn startup_gets_more_model_attempts_than_background_events() {
        assert_eq!(BehaviorEvent::Startup.max_retries(), 4);
        assert_eq!(BehaviorEvent::Touch.max_retries(), 2);
        assert_eq!(BehaviorEvent::Ambient.max_retries(), 2);
    }

    #[test]
    fn cleaner_rejects_identity_leaks_and_recent_duplicates() {
        let ctx = context();
        assert!(clean_behavior_line(
            "小云朵轻轻飘起来",
            BehaviorEvent::Touch,
            "crab",
            14,
            &ctx.recent_lines,
        )
        .is_none());
        assert!(clean_behavior_line(
            "我横着走两步，悄悄举起大螯。",
            BehaviorEvent::Ambient,
            "crab",
            14,
            &ctx.recent_lines,
        )
        .is_none());
    }

    #[test]
    fn cleaner_rejects_pet_body_actions_assigned_to_the_user() {
        assert!(clean_behavior_line(
            "啊！你在用横步和泡泡回应我吗？",
            BehaviorEvent::Startup,
            "crab",
            14,
            &[],
        )
        .is_none());
        assert_eq!(
            clean_behavior_line(
                "你看，我把大螯轻轻收好了。",
                BehaviorEvent::Touch,
                "crab",
                14,
                &[],
            )
            .as_deref(),
            Some("你看，我把大螯轻轻收好了")
        );
    }

    #[test]
    fn cleaner_keeps_fresh_shape_correct_line() {
        let line = clean_behavior_line(
            "小螯碰了碰指尖，又悄悄收好。",
            BehaviorEvent::Touch,
            "crab",
            14,
            &context().recent_lines,
        );
        assert_eq!(line.as_deref(), Some("小螯碰了碰指尖，又悄悄收好"));
    }

    #[test]
    fn luma_prompt_and_cleaner_keep_the_rabbit_body_consistent() {
        let mut ctx = context();
        ctx.pet_style = "luma".to_string();
        ctx.memory_summary = "以前是云朵，也试过猫咪、章鱼和螃蟹；对方喜欢安静陪伴。".to_string();

        let prompt = build_behavior_prompt(BehaviorEvent::Touch, &ctx, false);
        assert!(prompt.contains("月灯兔"));
        assert!(prompt.contains("长耳朵"));
        assert!(!prompt.contains("以前是云朵"));
        assert!(!prompt.contains("猫咪、章鱼和螃蟹"));

        assert_eq!(
            clean_behavior_line(
                "我把长耳朵轻轻靠向你的指尖。",
                BehaviorEvent::Touch,
                "luma",
                14,
                &[],
            )
            .as_deref(),
            Some("我把长耳朵轻轻靠向你的指尖")
        );
        assert!(
            clean_behavior_line("小云朵晃了晃。", BehaviorEvent::Touch, "luma", 14, &[],).is_none()
        );
    }

    #[test]
    fn cleaner_preserves_a_complete_line_instead_of_cutting_it_mid_sentence() {
        let raw = "（身体微微前倾，用一只大螯轻点地面）哼，我只是顺便回应你一下而已。";
        let line = clean_behavior_line(raw, BehaviorEvent::Touch, "crab", 14, &[]);

        assert_eq!(line.as_deref(), Some(raw.trim_end_matches('。')));
    }

    #[test]
    fn cleaner_rejects_an_obviously_unfinished_tail() {
        assert!(clean_behavior_line(
            "（身体微微前倾，用大螯轻点地面）哦，我用",
            BehaviorEvent::Touch,
            "crab",
            14,
            &[],
        )
        .is_none());
        assert!(clean_behavior_line(
            "快走啊，别逗我啦！你在用横步和",
            BehaviorEvent::Touch,
            "crab",
            14,
            &[],
        )
        .is_none());
    }

    #[test]
    fn cleaner_rejects_generic_self_introduction() {
        assert!(clean_behavior_line(
            "我是屏幕上的小螃蟹，今天有点傲娇。",
            BehaviorEvent::Ambient,
            "crab",
            14,
            &[],
        )
        .is_none());
    }

    #[test]
    fn cleaner_rejects_system_status_language() {
        for line in ["螃蟹，启动！", "本地模型已加载", "显存已释放"] {
            assert!(clean_behavior_line(line, BehaviorEvent::Startup, "crab", 14, &[],).is_none());
        }
    }

    #[test]
    fn startup_requires_a_greeting_or_reunion_signal() {
        assert!(clean_behavior_line(
            "（身体轻轻蹭着屏幕边缘）",
            BehaviorEvent::Startup,
            "crab",
            14,
            &[],
        )
        .is_none());
        assert_eq!(
            clean_behavior_line(
                "我刚醒过来，今天第一次见到你……",
                BehaviorEvent::Startup,
                "crab",
                14,
                &[],
            )
            .as_deref(),
            Some("我刚醒过来，今天第一次见到你……")
        );
    }

    #[test]
    fn intervals_are_jittered_but_bounded() {
        for entropy in [0, 7, 20, 40, 99, 1_000] {
            let seconds = jittered_interval(180, entropy).as_secs();
            assert!((144..=216).contains(&seconds));
        }
    }
}
