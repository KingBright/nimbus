pub fn parse_activity_output(output: &str) -> (String, u32) {
    let mut lines = output
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty());
    let app_name = lines.next().unwrap_or("未知应用");
    let hour = lines
        .next()
        .and_then(|line| line.parse::<u32>().ok())
        .filter(|hour| *hour <= 23)
        .unwrap_or(12);

    if is_invalid_app_name(app_name) {
        ("未知应用".to_string(), hour)
    } else {
        (app_name.to_string(), hour)
    }
}

pub async fn observe_user_activity() -> (String, u32) {
    let script = r#"
try
tell application "System Events"
    set frontApp to name of first application process whose frontmost is true
end tell
on error
    set frontApp to "未知应用"
end try
set currentHour to hours of (current date)
return frontApp & linefeed & currentHour
"#;

    let output = tokio::process::Command::new("osascript")
        .arg("-e")
        .arg(script)
        .output()
        .await;

    match output {
        Ok(out) if out.status.success() => {
            parse_activity_output(&String::from_utf8_lossy(&out.stdout))
        }
        Ok(out) => {
            eprintln!(
                "Failed to observe user activity: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            );
            ("未知应用".to_string(), 12)
        }
        Err(e) => {
            eprintln!("Failed to run osascript activity observer: {e:?}");
            ("未知应用".to_string(), 12)
        }
    }
}

pub fn is_invalid_app_name(app: &str) -> bool {
    app.is_empty() || app == "未知应用" || app.chars().all(|c| c.is_ascii_digit())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_app_and_hour() {
        assert_eq!(
            parse_activity_output("Code\n23\n"),
            ("Code".to_string(), 23)
        );
    }

    #[test]
    fn rejects_old_numeric_script_result() {
        assert_eq!(parse_activity_output("0\n"), ("未知应用".to_string(), 12));
    }

    #[test]
    fn rejects_invalid_hour() {
        assert_eq!(
            parse_activity_output("Safari\n99\n"),
            ("Safari".to_string(), 12)
        );
    }
}
