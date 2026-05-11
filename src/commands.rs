// src/commands.rs — 语音命令引擎
//
// action 类型:
// - launch   : 启动程序
// - open_url : 打开网址
// - hotkey   : 模拟快捷键 ("Ctrl+C" 或 "Ctrl+S,Enter")
// - text     : 输入文本

use std::path::Path;
use std::process::Command as ProcCommand;
use serde::Deserialize;

#[derive(Debug, Clone)]
pub enum Action {
    Launch(String),
    OpenUrl(String),
    Hotkey(Vec<Vec<String>>),
    Text(String),
}

#[derive(Debug, Deserialize)]
struct CommandsFile {
    #[serde(default)]
    commands: Vec<CommandEntry>,
}

#[derive(Debug, Deserialize)]
struct CommandEntry {
    triggers: Vec<String>,
    action: String,
    target: String,
}

#[derive(Debug, Clone)]
pub struct Command {
    pub triggers: Vec<String>,
    pub action: Action,
}

pub struct CommandEngine {
    commands: Vec<Command>,
}

pub enum MatchResult {
    Matched(Action),
    NoMatch,
}

impl CommandEngine {
    pub fn load(path: &Path) -> Self {
        if !path.exists() {
            return Self { commands: Vec::new() };
        }
        let content = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(e) => { log::warn!("读取 commands.toml 失败: {}", e); return Self { commands: Vec::new() }; }
        };
        let file: CommandsFile = match toml::from_str(&content) {
            Ok(f) => f,
            Err(e) => { log::warn!("解析 commands.toml 失败: {}", e); return Self { commands: Vec::new() }; }
        };

        let mut commands = Vec::new();
        for entry in file.commands {
            if let Some(action) = parse_action(&entry.action, &entry.target) {
                commands.push(Command { triggers: entry.triggers, action });
            }
        }
        log::info!("语音命令: {} 条", commands.len());
        Self { commands }
    }

    /// 严格匹配（完全相等或去标点后相等）
    pub fn match_text(&self, text: &str) -> MatchResult {
        let t1 = text.trim();
        if t1.is_empty() { return MatchResult::NoMatch; }
        let t2 = strip_punct(t1);
        for cmd in &self.commands {
            for trigger in &cmd.triggers {
                let tt = trigger.trim();
                if tt == t1 || tt == t2 {
                    return MatchResult::Matched(cmd.action.clone());
                }
            }
        }
        MatchResult::NoMatch
    }
}

fn parse_action(action: &str, target: &str) -> Option<Action> {
    match action {
        "launch" => Some(Action::Launch(target.to_string())),
        "open_url" => Some(Action::OpenUrl(target.to_string())),
        "text" => Some(Action::Text(target.to_string())),
        "hotkey" => {
            let steps: Vec<Vec<String>> = target.split(',')
                .map(|step| step.split('+').map(|k| k.trim().to_string()).filter(|k| !k.is_empty()).collect())
                .filter(|v: &Vec<String>| !v.is_empty())
                .collect();
            if steps.is_empty() { None } else { Some(Action::Hotkey(steps)) }
        }
        _ => { log::warn!("未知动作: {}", action); None }
    }
}

pub fn strip_punct(s: &str) -> String {
    s.chars()
        .filter(|c| !matches!(c,
            '\u{FF0C}' | '\u{3002}' | '\u{FF01}' | '\u{FF1F}' | '\u{3001}' |
            '\u{FF1A}' | '\u{FF1B}' | '\u{201C}' | '\u{201D}' |
            '\u{FF08}' | '\u{FF09}' | '\u{3010}' | '\u{3011}' |
            ',' | '.' | '!' | '?' | ':' | ';' | '"' | '\'' |
            '(' | ')' | '[' | ']' | ' ' | '\t' | '\n' | '\r'
        ))
        .collect()
}

pub fn exec_launch(target: &str) -> Result<(), String> {
    ProcCommand::new(target).spawn().map(|_| ()).map_err(|e| format!("启动失败 {}: {}", target, e))
}

pub fn exec_open_url(url: &str) -> Result<(), String> {
    ProcCommand::new("cmd").args(["/c", "start", "", url]).spawn().map(|_| ()).map_err(|e| format!("URL 失败: {}", e))
}

pub fn exec_hotkey(steps: &[Vec<String>]) -> Result<(), String> {
    use enigo::{Enigo, Key, Direction, Keyboard, Settings};
    let mut enigo = Enigo::new(&Settings::default()).map_err(|e| format!("Enigo: {:?}", e))?;
    for step in steps {
        let mut modifiers: Vec<Key> = Vec::new();
        let mut main_key: Option<Key> = None;
        for key_str in step {
            match parse_key(key_str) {
                Some(k) if is_modifier(key_str) => modifiers.push(k),
                Some(k) => main_key = Some(k),
                None => return Err(format!("未知按键: {}", key_str)),
            }
        }
        for m in &modifiers { let _ = enigo.key(*m, Direction::Press); }
        if let Some(mk) = main_key { let _ = enigo.key(mk, Direction::Click); }
        for m in modifiers.iter().rev() { let _ = enigo.key(*m, Direction::Release); }
        std::thread::sleep(std::time::Duration::from_millis(30));
    }
    Ok(())
}

fn is_modifier(s: &str) -> bool {
    matches!(s.to_lowercase().as_str(), "ctrl"|"control"|"alt"|"shift"|"win"|"meta")
}

fn parse_key(s: &str) -> Option<enigo::Key> {
    use enigo::Key;
    match s.to_lowercase().as_str() {
        "ctrl"|"control" => Some(Key::Control),
        "alt" => Some(Key::Alt),
        "shift" => Some(Key::Shift),
        "win"|"meta" => Some(Key::Meta),
        "enter"|"return" => Some(Key::Return),
        "space" => Some(Key::Space),
        "tab" => Some(Key::Tab),
        "escape"|"esc" => Some(Key::Escape),
        "backspace" => Some(Key::Backspace),
        "delete"|"del" => Some(Key::Delete),
        "home" => Some(Key::Home),
        "end" => Some(Key::End),
        "up" => Some(Key::UpArrow),
        "down" => Some(Key::DownArrow),
        "left" => Some(Key::LeftArrow),
        "right" => Some(Key::RightArrow),
        "f1" => Some(Key::F1), "f2" => Some(Key::F2), "f3" => Some(Key::F3),
        "f4" => Some(Key::F4), "f5" => Some(Key::F5), "f6" => Some(Key::F6),
        "f7" => Some(Key::F7), "f8" => Some(Key::F8), "f9" => Some(Key::F9),
        "f10" => Some(Key::F10), "f11" => Some(Key::F11), "f12" => Some(Key::F12),
        _ => {
            let lower = s.to_lowercase();
            if lower.chars().count() == 1 {
                Some(Key::Unicode(lower.chars().next().unwrap()))
            } else { None }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_strip_punct() {
        assert_eq!(strip_punct("打开记事本。"), "打开记事本");
        assert_eq!(strip_punct("复制！"), "复制");
        assert_eq!(strip_punct("hello, world"), "helloworld");
    }
}