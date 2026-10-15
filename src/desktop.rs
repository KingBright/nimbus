//! Native dialogs run outside the window event loop; all text travels as argv data.
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};

pub const NO_MODEL_MESSAGE: &str =
    "我还没有连接本地模型。请从托盘打开“使用帮助”查看配置方法；现在也可以摸摸我。";
pub const EMPTY_RESPONSE_MESSAGE: &str = "这次没有收到模型回复。请稍后再试；如果持续失败，可在系统参数中清空对话历史，或查看 nimbus.log。";

const INPUT_SCRIPT: &str = r#"on run argv
    set answer to display dialog (item 2 of argv) with title (item 1 of argv) default answer (item 3 of argv) buttons {"取消", "确定"} default button "确定" cancel button "取消"
    return text returned of answer
end run"#;
const MESSAGE_SCRIPT: &str = r#"on run argv
    display dialog (item 2 of argv) with title (item 1 of argv) buttons {"好的"} default button "好的"
end run"#;

#[derive(Clone, Copy)]
pub enum InputKind {
    Chat,
    Rename,
}

pub enum DialogResult {
    Input(InputKind, String),
    Failed,
}

pub struct DesktopDialogs {
    busy: Arc<AtomicBool>,
    tx: mpsc::Sender<DialogResult>,
    rx: mpsc::Receiver<DialogResult>,
}

fn dialog_command(script: &str, title: &str, message: &str, initial: Option<&str>) -> Command {
    let mut command = Command::new("osascript");
    command.args(["-e", script, "--", title, message]);
    if let Some(initial) = initial {
        command.arg(initial);
    }
    command
}

impl DesktopDialogs {
    pub fn new() -> Self {
        let (tx, rx) = mpsc::channel();
        Self {
            busy: Arc::new(AtomicBool::new(false)),
            tx,
            rx,
        }
    }

    pub fn input(&self, kind: InputKind, title: &str, message: &str, initial: &str) {
        self.launch(
            dialog_command(INPUT_SCRIPT, title, message, Some(initial)),
            Some(kind),
        );
    }

    pub fn show(&self, title: &str, message: &str) {
        self.launch(dialog_command(MESSAGE_SCRIPT, title, message, None), None);
    }

    pub fn try_recv(&self) -> Option<DialogResult> {
        self.rx.try_recv().ok()
    }

    fn launch(&self, mut command: Command, kind: Option<InputKind>) {
        if self.busy.swap(true, Ordering::AcqRel) {
            return;
        }
        let busy = self.busy.clone();
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let result = match command.output() {
                Ok(output) if output.status.success() => {
                    if let Some(kind) = kind {
                        let text = String::from_utf8_lossy(&output.stdout).trim().to_owned();
                        if !text.is_empty() {
                            Some(DialogResult::Input(kind, text))
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                }
                Ok(output) if String::from_utf8_lossy(&output.stderr).contains("(-128)") => None,
                _ => Some(DialogResult::Failed),
            };
            busy.store(false, Ordering::Release);
            if let Some(result) = result {
                let _ = tx.send(result);
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn received_input_can_immediately_open_a_follow_up_dialog() {
        let dialogs = DesktopDialogs::new();
        let mut first = Command::new("/usr/bin/printf");
        first.args(["%s", "A name that needs a follow-up"]);
        dialogs.launch(first, Some(InputKind::Rename));
        let result = dialogs
            .rx
            .recv_timeout(std::time::Duration::from_secs(5))
            .expect("first dialog result");
        assert!(matches!(result, DialogResult::Input(InputKind::Rename, _)));

        // Simulate the UI responding as soon as it receives the first result.
        let mut follow_up = Command::new("/usr/bin/printf");
        follow_up.args(["%s", "Follow-up accepted"]);
        dialogs.launch(follow_up, Some(InputKind::Chat));
        let result = dialogs
            .rx
            .recv_timeout(std::time::Duration::from_secs(5))
            .expect("follow-up must not be discarded as busy");
        assert!(matches!(
            result,
            DialogResult::Input(InputKind::Chat, text) if text == "Follow-up accepted"
        ));
    }

    #[test]
    fn user_text_is_passed_as_data_not_applescript() {
        let text = "云朵\\\"\n--你好 $(example)";
        let command = dialog_command(INPUT_SCRIPT, text, text, Some(text));
        let args: Vec<_> = command
            .get_args()
            .map(|arg| arg.to_str().unwrap())
            .collect();
        assert_eq!(args, ["-e", INPUT_SCRIPT, "--", text, text, text]);
        assert!(!INPUT_SCRIPT.contains(text));
    }
}
