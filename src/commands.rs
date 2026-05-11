// src/commands.rs — 语音命令引擎
//
// commands.toml 格式：
// [[commands]]
// triggers = ["打开记事本", "启动记事本"]
// action = "launch"
// target = "notepad.exe"
//
// action 类型:
// - launch   : 启动程序  (target = 可执行文件名或路径)
// - open_url : 打开网址  (target = URL)
// - hotkey   : 模拟快捷键 (target = "Ctrl+C" 或 "Ctrl+S,Enter" 多步用逗号)
// - text     : 输入文本   (target = 要输入的内容)
// - builtin  : 内置命令   (target = "pause"|"resume"|"toggle")

use std::path::Path;
use std::process::Command as ProcCommand;

use serde::Deserialize;

/// 动作类型
#[derive(Debug, Clone)]
pub enum Action {
    Launch(String),
    OpenUrl(String),
    Hotkey(Vec<Vec<String>>),  // 多步骤，每步是一组按键
    Text(String),
    Builtin(BuiltinCmd),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuiltinCmd {
    Pause,
    Resume,
    Toggle,
}

/// 原始 TOML 结构
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

/// 解析后的命令
#[derive(Debug, Clone)]
pub struct Command {
    pub triggers: Vec<String>,
    pub action: Action,
}

/// 命令引擎
pub struct CommandEngine {
    commands: Vec<Command>,
}

/// 匹配结果
pub enum MatchResult {
    Matched(Action),
    NoMatch,
}

impl CommandEngine {
    /// 从 TOML 文件加载
    pub fn load(path: &Path) -> Self {
        if !path.exists() {
            log::info!("命令文件不存在: {} (跳过)", path.display());
            return Self { commands: Vec::new() };
        }

        let content = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(e) => {
                log::warn!("读取 commands.toml 失败: {}", e);
                return Self { commands: Vec::new() };
            }
        };

        let file: CommandsFile = match toml::from_str(&content) {
            Ok(f) => f,
            Err(e) => {
                log::warn!("解析 commands.toml 失败: {}", e);
                return Self { commands: Vec::new() };
            }
        };

        let mut commands = Vec::new();
        for entry in file.commands {
            let action = match parse_action(&entry.action, &entry.target) {
                Some(a) => a,
                None => {
                    log::warn!("未知动作类型: {} (target={})", entry.action, entry.target);
                    continue;
                }
            };
            commands.push(Command {
                triggers: entry.triggers,
                action,
            });
        }

        log::info!("已加载 {} 条语音命令", commands.len());
        Self { commands }
    }

    /// 严格匹配：文本等于触发词，或去标点后等于
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

    pub fn len(&self) -> usize { self.commands.len() }
}

/// 解析动作
fn parse_action(action: &str, target: &str) -> Option<Action> {
    match action {
        "launch" => Some(Action::Launch(target.to_string())),
        "open_url" => Some(Action::OpenUrl(target.to_string())),
        "text" => Some(Action::Text(target.to_string())),
        "hotkey" => {
            // "Ctrl+S,Enter" → [[Ctrl,S], [Enter]]
            let steps: Vec<Vec<String>> = target
                .split(',')
                .map(|step| {
                    step.split('+')
                        .map(|k| k.trim().to_string())
                        .filter(|k| !k.is_empty())
                        .collect()
                })
                .filter(|v: &Vec<String>| !v.is_empty())
                .collect();
            if steps.is_empty() { None } else { Some(Action::Hotkey(steps)) }
        }
        "builtin" => {
            let cmd = match target.to_lowercase().as_str() {
                "pause" => BuiltinCmd::Pause,
                "resume" => BuiltinCmd::Resume,
                "toggle" => BuiltinCmd::Toggle,
                _ => return None,
            };
            Some(Action::Builtin(cmd))
        }
        _ => None,
    }
}

/// 去除常见中英文标点和空白
pub fn strip_punct(s: &str) -> String {
    s.chars()
        .filter(|c| !matches!(c,
            '，' | '。' | '！' | '？' | '、' | '：' | '；' |
            '\u{201C}' | '\u{201D}' |       // " "
            '（' | '）' | '【' | '】' |
            ',' | '.' | '!' | '?' | ':' | ';' | '"' | '\'' |
            '(' | ')' | '[' | ']' |
            ' ' | '\t' | '\n' | '\r'
        ))
        .collect()
}

// ═══════════════════════════════════════════
//  动作执行
// ═══════════════════════════════════════════

/// 启动程序
pub fn exec_launch(target: &str) -> Result<(), String> {
    ProcCommand::new(target)
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("启动失败 {}: {}", target, e))
}

/// 打开 URL (Windows)
pub fn exec_open_url(url: &str) -> Result<(), String> {
    ProcCommand::new("cmd")
        .args(["/c", "start", "", url])
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("打开 URL 失败 {}: {}", url, e))
}

/// 执行快捷键（多步骤）
pub fn exec_hotkey(steps: &[Vec<String>]) -> Result<(), String> {
    use enigo::{Enigo, Key, Direction, Keyboard, Settings};

    let mut enigo = Enigo::new(&Settings::default())
        .map_err(|e| format!("Enigo 初始化失败: {:?}", e))?;

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

        // 按下修饰键
        for m in &modifiers {
            let _ = enigo.key(*m, Direction::Press);
        }

        // 点击主键
        if let Some(mk) = main_key {
            let _ = enigo.key(mk, Direction::Click);
        }

        // 释放修饰键（倒序）
        for m in modifiers.iter().rev() {
            let _ = enigo.key(*m, Direction::Release);
        }

        std::thread::sleep(std::time::Duration::from_millis(30));
    }

    Ok(())
}

fn is_modifier(s: &str) -> bool {
    matches!(s.to_lowercase().as_str(),
        "ctrl" | "control" | "alt" | "shift" | "win" | "meta" | "super" | "cmd")
}

fn parse_key(s: &str) -> Option<enigo::Key> {
    use enigo::Key;
    let lower = s.to_lowercase();
    match lower.as_str() {
        "ctrl" | "control" => Some(Key::Control),
        "alt" => Some(Key::Alt),
        "shift" => Some(Key::Shift),
        "win" | "meta" | "super" | "cmd" => Some(Key::Meta),
        "enter" | "return" => Some(Key::Return),
        "space" => Some(Key::Space),
        "tab" => Some(Key::Tab),
        "escape" | "esc" => Some(Key::Escape),
        "backspace" => Some(Key::Backspace),
        "delete" | "del" => Some(Key::Delete),
        "home" => Some(Key::Home),
        "end" => Some(Key::End),
        "pageup" => Some(Key::PageUp),
        "pagedown" => Some(Key::PageDown),
        "up" => Some(Key::UpArrow),
        "down" => Some(Key::DownArrow),
        "left" => Some(Key::LeftArrow),
        "right" => Some(Key::RightArrow),
        "f1" => Some(Key::F1),  "f2" => Some(Key::F2),
        "f3" => Some(Key::F3),  "f4" => Some(Key::F4),
        "f5" => Some(Key::F5),  "f6" => Some(Key::F6),
        "f7" => Some(Key::F7),  "f8" => Some(Key::F8),
        "f9" => Some(Key::F9),  "f10" => Some(Key::F10),
        "f11" => Some(Key::F11), "f12" => Some(Key::F12),
        _ => {
            // 单字符
            if lower.chars().count() == 1 {
                Some(Key::Unicode(lower.chars().next().unwrap()))
            } else {
                None
            }
        }
    }
}