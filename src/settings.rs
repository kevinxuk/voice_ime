// src/settings.rs — 设置界面（HTML页面 + 微型 HTTP 服务器）
//
// 点击"设置"后:
// 1. 启动本地 HTTP 服务器（端口 17630）
// 2. 生成 HTML 设置页面
// 3. 用系统浏览器打开 http://127.0.0.1:17630
// 4. 用户编辑配置后点保存，HTTP POST 接收并写入文件
// 5. 保存完成页面提示"已保存，重启生效"

use std::io::{Read, Write, BufRead, BufReader};
use std::net::TcpListener;
use std::path::Path;
use std::thread;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

const PORT: u16 = 17630;

/// 启动设置服务器（非阻塞，在独立线程运行）
pub fn open_settings(model_dir: &Path) {
    let model_dir = model_dir.to_path_buf();
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()))
        .unwrap_or_else(|| std::path::PathBuf::from("."));

    thread::spawn(move || {
        if let Err(e) = run_settings_server(&exe_dir, &model_dir) {
            log::error!("设置服务器错误: {}", e);
        }
    });

    // 打开浏览器
    let url = format!("http://127.0.0.1:{}", PORT);
    log::info!("打开设置页面: {}", url);
    let _ = std::process::Command::new("cmd")
        .args(["/c", "start", "", &url])
        .spawn();
}

fn run_settings_server(exe_dir: &Path, model_dir: &Path) -> anyhow::Result<()> {
    let listener = TcpListener::bind(format!("127.0.0.1:{}", PORT))?;
    listener.set_nonblocking(false)?;
    log::info!("设置服务器启动: 127.0.0.1:{}", PORT);

    // 只服务前几个请求，用完自动关闭（防止端口泄露）
    let mut request_count = 0;
    let max_requests = 20;

    for stream in listener.incoming() {
        if request_count >= max_requests { break; }
        request_count += 1;

        let mut stream = match stream {
            Ok(s) => s,
            Err(_) => continue,
        };

        let mut reader = BufReader::new(stream.try_clone()?);
        let mut request_line = String::new();
        reader.read_line(&mut request_line)?;

        // 读取 headers
        let mut content_length: usize = 0;
        let mut headers = String::new();
        loop {
            let mut line = String::new();
            reader.read_line(&mut line)?;
            if line.trim().is_empty() { break; }
            if line.to_lowercase().starts_with("content-length:") {
                content_length = line.split(':').nth(1)
                    .unwrap_or("0").trim().parse().unwrap_or(0);
            }
            headers.push_str(&line);
        }

        if request_line.starts_with("GET / ") || request_line.starts_with("GET /index") {
            // 返回设置页面
            let html = generate_settings_html(exe_dir, model_dir);
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                html.len(), html
            );
            stream.write_all(response.as_bytes())?;
        } else if request_line.starts_with("POST /save") {
            // 读取 body
            let mut body = vec![0u8; content_length];
            reader.read_exact(&mut body)?;
            let body_str = String::from_utf8_lossy(&body);

            // 解析 form data
            let result = handle_save(exe_dir, model_dir, &body_str);
            let msg = if result.is_ok() { "保存成功！重启程序后生效。" } else { "保存失败，请检查文件权限。" };
            let html = format!(r#"<!DOCTYPE html><html><head><meta charset="utf-8"><title>Voice IME</title></head>
<body style="font-family:sans-serif;text-align:center;padding:60px;background:#1a1a2e;color:#eee">
<h2>{}</h2><p><a href="/" style="color:#4fc3f7">返回设置</a></p>
</body></html>"#, msg);
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                html.len(), html
            );
            stream.write_all(response.as_bytes())?;
        } else {
            let response = "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n";
            stream.write_all(response.as_bytes())?;
        }
    }

    log::info!("设置服务器已关闭");
    Ok(())
}

fn handle_save(exe_dir: &Path, model_dir: &Path, body: &str) -> anyhow::Result<()> {
    // URL-decode form data
    let params = parse_form(body);

    // 保存 voice_ime.toml
    if let Some(toml_content) = params.get("config_toml") {
        let decoded = url_decode(toml_content);
        std::fs::write(exe_dir.join("voice_ime.toml"), decoded)?;
        log::info!("已保存 voice_ime.toml");
    }

    // 保存 hotwords.txt
    if let Some(hw) = params.get("hotwords") {
        let decoded = url_decode(hw);
        std::fs::write(model_dir.join("hotwords.txt"), decoded)?;
        log::info!("已保存 hotwords.txt");
    }

    // 保存 corrections.txt
    if let Some(corr) = params.get("corrections") {
        let decoded = url_decode(corr);
        std::fs::write(model_dir.join("corrections.txt"), decoded)?;
        log::info!("已保存 corrections.txt");
    }

    // 保存 commands.toml
    if let Some(cmds) = params.get("commands") {
        let decoded = url_decode(cmds);
        std::fs::write(model_dir.join("commands.toml"), decoded)?;
        log::info!("已保存 commands.toml");
    }

    Ok(())
}

fn generate_settings_html(exe_dir: &Path, model_dir: &Path) -> String {
    // 读取当前配置内容
    let config_toml = std::fs::read_to_string(exe_dir.join("voice_ime.toml"))
        .unwrap_or_else(|_| default_config_toml());
    let hotwords = std::fs::read_to_string(model_dir.join("hotwords.txt"))
        .unwrap_or_default();
    let corrections = std::fs::read_to_string(model_dir.join("corrections.txt"))
        .unwrap_or_default();
    let commands = std::fs::read_to_string(model_dir.join("commands.toml"))
        .unwrap_or_default();

    format!(r#"<!DOCTYPE html>
<html lang="zh-CN">
<head>
<meta charset="utf-8">
<title>Voice IME 设置</title>
<style>
* {{ margin:0; padding:0; box-sizing:border-box; }}
body {{ font-family: -apple-system, "Microsoft YaHei", sans-serif; background:#0d1117; color:#c9d1d9; padding:20px; }}
h1 {{ color:#58a6ff; margin-bottom:20px; font-size:22px; }}
h2 {{ color:#79c0ff; margin:20px 0 10px; font-size:16px; border-bottom:1px solid #21262d; padding-bottom:6px; }}
.tab-bar {{ display:flex; gap:2px; margin-bottom:15px; }}
.tab {{ padding:8px 16px; background:#161b22; border:1px solid #30363d; cursor:pointer; color:#8b949e; border-radius:6px 6px 0 0; }}
.tab.active {{ background:#0d1117; color:#58a6ff; border-bottom-color:#0d1117; }}
.panel {{ display:none; }}
.panel.active {{ display:block; }}
textarea {{ width:100%; min-height:200px; background:#161b22; color:#c9d1d9; border:1px solid #30363d; padding:10px; font-family:monospace; font-size:13px; border-radius:6px; resize:vertical; }}
label {{ display:block; margin:8px 0 4px; color:#8b949e; font-size:13px; }}
input[type=text],input[type=number] {{ width:100%; padding:6px 10px; background:#161b22; color:#c9d1d9; border:1px solid #30363d; border-radius:4px; font-size:14px; }}
.btn {{ display:inline-block; padding:8px 20px; background:#238636; color:#fff; border:none; border-radius:6px; cursor:pointer; font-size:14px; margin-top:15px; }}
.btn:hover {{ background:#2ea043; }}
.note {{ color:#8b949e; font-size:12px; margin-top:4px; }}
.row {{ display:grid; grid-template-columns:1fr 1fr; gap:12px; }}
</style>
</head>
<body>
<h1>Voice IME 设置</h1>

<div class="tab-bar">
  <div class="tab active" onclick="switchTab('general')">基本设置</div>
  <div class="tab" onclick="switchTab('hotwords')">热词</div>
  <div class="tab" onclick="switchTab('corrections')">纠错映射</div>
  <div class="tab" onclick="switchTab('commands')">语音命令</div>
</div>

<form method="POST" action="/save">

<div id="panel-general" class="panel active">
<h2>基本配置 (voice_ime.toml)</h2>
<p class="note">修改后保存，重启程序生效。</p>
<textarea name="config_toml" rows="16">{config_toml}</textarea>
</div>

<div id="panel-hotwords" class="panel">
<h2>热词表 (hotwords.txt)</h2>
<p class="note">每行一个词。模型会优先识别热词中的内容。</p>
<textarea name="hotwords" rows="20">{hotwords}</textarea>
</div>

<div id="panel-corrections" class="panel">
<h2>纠错映射 (corrections.txt)</h2>
<p class="note">格式：错误词→正确词。识别结果会自动替换。# 开头为注释。</p>
<textarea name="corrections" rows="20">{corrections}</textarea>
</div>

<div id="panel-commands" class="panel">
<h2>语音命令 (commands.toml)</h2>
<p class="note">TOML 格式。triggers=触发词列表，action=动作类型，target=参数。</p>
<textarea name="commands" rows="20">{commands}</textarea>
</div>

<button type="submit" class="btn">保存所有设置</button>
</form>

<script>
function switchTab(name) {{
  document.querySelectorAll('.panel').forEach(p => p.classList.remove('active'));
  document.querySelectorAll('.tab').forEach(t => t.classList.remove('active'));
  document.getElementById('panel-' + name).classList.add('active');
  event.target.classList.add('active');
}}
</script>
</body>
</html>"#,
        config_toml = html_escape(&config_toml),
        hotwords = html_escape(&hotwords),
        corrections = html_escape(&corrections),
        commands = html_escape(&commands),
    )
}

fn default_config_toml() -> String {
    r#"[asr]
n_threads = 4
decoding_method = "greedy_search"
model_type = "transducer"

[audio]
sample_rate = 16000
channels = 1
vad_threshold = 0.01
silence_duration_ms = 500
min_speech_duration_ms = 300
buffer_frames = 1024
use_vad_endpoint = false

[hotkey]
toggle = "Ctrl+Alt+V"
"#.to_string()
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
     .replace('<', "&lt;")
     .replace('>', "&gt;")
     .replace('"', "&quot;")
}

fn parse_form(body: &str) -> std::collections::HashMap<String, String> {
    let mut map = std::collections::HashMap::new();
    for pair in body.split('&') {
        if let Some((k, v)) = pair.split_once('=') {
            map.insert(k.to_string(), v.to_string());
        }
    }
    map
}

fn url_decode(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        match c {
            '%' => {
                let hex: String = chars.by_ref().take(2).collect();
                if let Ok(byte) = u8::from_str_radix(&hex, 16) {
                    result.push(byte as char);
                } else {
                    result.push('%');
                    result.push_str(&hex);
                }
            }
            '+' => result.push(' '),
            _ => result.push(c),
        }
    }
    // 处理 UTF-8 编码（%E4%B8%AD → 中）
    let bytes: Vec<u8> = result.bytes().collect();
    String::from_utf8(bytes).unwrap_or_else(|_| result)
}