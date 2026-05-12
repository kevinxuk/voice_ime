# Voice IME — 完全离线的中文语音输入法

基于 **Rust + Sherpa-ONNX** 的轻量级语音输入工具。无需联网，开箱即用。

## 特性

- **完全离线** — 所有推理在本地 CPU 完成，不发送任何数据到网络
- **双模型支持** — Transducer（流式+热词）/ SenseVoice（高精度+自带标点），通过配置一键切换
- **按住说话** — 按住热键录音，松开即出结果，流式解码延迟极低
- **智能纠错** — Bigram 统计纠错 + 同音字泛化 + 上下文感知 corrections
- **自学习** — 按 Esc 纠正识别错误，系统自动学习并越用越准
- **实时预览** — Win32 原生文字窗口显示识别结果，中文完美渲染，超长文本自动滚动
- **波纹可视化** — 屏幕顶部小窗口实时显示语音波纹，右键切换样式或退出
- **语音命令** — 说出关键词直接执行系统动作（打开程序/模拟快捷键）
- **热词增强** — 预置 140+ 热词，支持逐词权重，自动 BPE 格式转换
- **零安装** — 单 exe + 模型文件，静态链接 CRT，无需 VC++ 运行库

## 系统要求

- Windows 11/10 x86_64
- 任意麦克风
- 约 230MB 磁盘空间（含模型）

## 快速开始

### 1. 下载模型文件

支持两种模型，任选其一：

**模型 A：Transducer（推荐入门，流式识别，支持热词）**

| 下载地址 | 说明 |
|----------|------|
| [GitHub Releases](https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/sherpa-onnx-streaming-zipformer-bilingual-zh-en-2023-02-20.tar.bz2) | 中英双语流式模型，~370MB |
| [HuggingFace 镜像](https://huggingface.co/csukuangfj/sherpa-onnx-streaming-zipformer-bilingual-zh-en-2023-02-20) | 国内访问更快 |

**模型 B：SenseVoice（更高精度，自带标点，推荐进阶）**

| 下载地址 | 说明 |
|----------|------|
| [GitHub Releases](https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/sherpa-onnx-sense-voice-zh-en-ja-ko-yue-2024-07-17.tar.bz2) | 中英日韩粤多语言，~1GB |
| [INT8 量化版](https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/sherpa-onnx-sense-voice-zh-en-ja-ko-yue-2024-07-17-int8.tar.bz2) | 体积更小，~230MB |

解压后将文件放入 `models/` 目录。

### 2. 运行

```
voice_ime.exe
```

### 3. 使用

| 操作 | 说明 |
|------|------|
| **按住热键** | 开始录音（默认 `Ctrl+Alt+V`，可在配置中修改） |
| **松开热键** | 停止录音，识别结果在预览窗口显示 1 秒后自动输出到光标位置 |
| **按 Esc** | 预览期间按 Esc 可弹出纠错对话框，修改后系统自动学习 |
| **右键窗口** | 切换波纹样式 / 打开设置 / 退出 |
| **说"打开记事本"** | 语音命令直接执行（严格匹配，不会误触发） |

## 波纹窗口

程序启动后在屏幕顶部正中央显示一个小窗口（160×28 像素）：

```
┌──────────────────────────────────────┐
│  ████████████░░░░░░░░░  80%          │  ← 模型加载中（进度条）
└──────────────────────────────────────┘
                  ↓
┌──────────────────────────────────────┐
│  ●────────────────────────────────   │  ← 就绪（绿色呼吸灯）
└──────────────────────────────────────┘
                  ↓ 按住热键说话
┌──────────────────────────────────────┐
│  ～～∿∿～～∿～～∿∿～～∿～～  │  ← 波纹动画
└──────────────────────────────────────┘
┌──────────────────────────────────────────────┐
│ 我想看看编程的效果怎么样这个方案...           │  ← Win32 文字预览（自动滚动）
└──────────────────────────────────────────────┘
                  ↓ 松开热键，1秒后自动输出
┌──────────────────────────────────────┐
│  ●────────────────────────────────   │  ← 回到就绪
└──────────────────────────────────────┘
```

- **无标题栏**、无边框、屏幕居中置顶、70% 透明度
- **文字预览窗口**：Win32 原生渲染（微软雅黑 14px），超 20 字自动从右向左滚动
- **右键弹出菜单**：波纹样式切换 / 设置 / 退出

## 自学习纠错

### 使用方式

1. 按住热键说话 → 松开 → 预览窗口显示识别结果
2. 发现错误？**1 秒内按 Esc** → 弹出纠错对话框
3. 修改文字后点确认 → 系统自动学习

### 学习效果（三重生效）

| 层 | 效果 | 说明 |
|---|---|---|
| `auto_corrections.txt` | 精确替换 | 下次遇到相同错误直接替换 |
| Bigram 频率调整 | 统计倾向 | 降低错误组合频率，提升正确组合 |
| 热词自动追加 | ASR 加分 | 正确词被加入热词表（Transducer 模式） |

### 上下文感知 + 同音泛化

```
用户纠正: "我就相看看" → "我就想看看"

系统学习:
  相→想              ← 精确映射
  就相看→就想看       ← 上下文窗口（前后各 1 字）
  就象看→就想看       ← 同音泛化（自动覆盖 ASR 的其他同音错误）
  就像看→就想看
  就向看→就想看
```

### 纠错文件

| 文件 | 说明 |
|------|------|
| `models/corrections.txt` | 手动维护的纠错映射（`错误词→正确词`） |
| `models/auto_corrections.txt` | 自动学习生成的纠错映射（程序自动追加） |
| `models/bigram.bin` | 53K 组二字频率表（来自 jieba 语料，运行时自动更新） |
| `models/homophones.txt` | 532 组同音字映射（用于同音泛化纠错） |

## 语音命令

说出触发词直接执行动作（严格匹配，完整说出才触发）：

| 说 | 执行 |
|----|------|
| "打开记事本" | 启动 notepad.exe |
| "打开计算器" | 启动 calc.exe |
| "打开百度" | 浏览器打开百度 |
| "复制" | Ctrl+C |
| "粘贴" | Ctrl+V |
| "保存" | Ctrl+S |

自定义命令：编辑 `models/commands.toml`

## 双模型切换

在 `voice_ime.toml` 中修改 `model_type` 即可切换：

```toml
[asr]
n_threads = 4

# Transducer（流式，支持热词，实时预览中间结果）
model_type = "transducer"
encoder = "encoder-epoch-99-avg-1.int8.onnx"
decoder = "decoder-epoch-99-avg-1.onnx"
joiner = "joiner-epoch-99-avg-1.int8.onnx"

# SenseVoice（高精度，自带标点，中英日韩粤）
# model_type = "sense_voice"
# sense_voice_model = "sense_voice_model.int8.onnx"
# sense_voice_tokens = "sense_voice_tokens.txt"

[audio]
sample_rate = 16000
channels = 1
vad_threshold = 0.01
silence_duration_ms = 500
min_speech_duration_ms = 300
buffer_frames = 1024

[hotkey]
toggle = "Ctrl+Alt+V"
```

### 模型对比

| | Transducer | SenseVoice |
|---|---|---|
| 准确率 | ★★★ | ★★★★★ |
| 标点 | 规则自动添加 | 模型自带标点 |
| 流式预览 | 实时中间结果 | 显示"录音中..." |
| 热词 | 支持（逐词权重） | 不支持 |
| 松开后延迟 | ~0.3s | ~1-2s |

## 设置

右键波纹窗口 → 点击"设置..." → 浏览器打开设置页面（暗色主题，4 个标签页）：

- **基本配置** — 编辑 `voice_ime.toml`（线程数、热键、模型类型等）
- **热词** — 编辑 `hotwords.txt`（支持逐词权重：`编程 6.0`）
- **纠错映射** — 编辑 `corrections.txt`
- **语音命令** — 编辑 `commands.toml`

## 技术架构

```
按住热键 → 麦克风采集 (48kHz/2ch)
              ↓ cpal 自动重采样 (16kHz/1ch)
           流式喂入 ASR (Transducer / SenseVoice)
              ↓ 松开热键
           Bigram 统计纠错 → corrections 精确覆盖 → 自动标点
              ↓
           Win32 文字预览窗口（1秒）
              ↓
           enigo 键盘输出到光标 → 更新 bigram 频率
```

| 模块 | 技术 | 说明 |
|------|------|------|
| 音频采集 | cpal 0.15 | 自适应设备格式，线性重采样 |
| 语音识别 | sherpa-onnx 1.13 | Transducer + SenseVoice 双模式 |
| 热词增强 | modified_beam_search | BPE 自动转换，逐词权重 |
| 纠错引擎 | bigram + homophones | 53K 频率表 + 532 组同音字 |
| 自学习 | Esc 纠错对话框 | 上下文窗口 + 同音泛化 |
| 语音命令 | commands.toml | 严格匹配，支持 launch/hotkey/text |
| 文字预览 | Win32 DrawTextW | 微软雅黑 14px，自动滚动 |
| 波纹 UI | minifb 0.27 | 4 种样式，进度条，右键菜单 |
| 设置 | 内嵌 HTTP 服务器 | 浏览器暗色主题设置页 |
| 编译 | Rust + 静态 CRT | 零运行时依赖 |

## 目录结构

```
voice_ime/
├── .cargo/config.toml          ← 静态 CRT 链接
├── Cargo.toml
├── src/
│   ├── main.rs                 ← 入口 + App 状态机
│   ├── lib.rs                  ← 模块声明
│   ├── asr.rs                  ← 双模式 ASR 引擎
│   ├── audio.rs                ← cpal 音频采集 + 重采样
│   ├── output.rs               ← enigo 键盘输出
│   ├── correct.rs              ← 纠错引擎（bigram + 同音 + corrections）
│   ├── bigram.rs               ← Bigram 频率表
│   ├── punctuation.rs          ← 规则标点
│   ├── commands.rs             ← 语音命令引擎
│   ├── hotkey.rs               ← 全局热键（按住/松开/Esc）
│   ├── ui.rs                   ← 波纹可视化窗口
│   ├── preview.rs              ← Win32 文字预览窗口
│   └── settings.rs             ← HTML 设置页面
├── models/
│   ├── tokens.txt              ← 词表（Transducer）
│   ├── sense_voice_tokens.txt  ← 词表（SenseVoice）
│   ├── hotwords.txt            ← 热词（可编辑，支持逐词权重）
│   ├── corrections.txt         ← 手动纠错映射
│   ├── auto_corrections.txt    ← 自动学习纠错映射
│   ├── commands.toml           ← 语音命令配置
│   ├── bigram.bin              ← 二字频率表（616KB，运行时自动更新）
│   └── homophones.txt          ← 同音字映射（532 组）
└── package.bat                 ← 打包脚本
```

## 从源码编译

```bash
# 安装 Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# 编译（静态链接 CRT，无需 vcredist）
cargo build --release

# 运行测试（15 个单元测试）
cargo test

# 输出在 target/release/voice_ime.exe
```

## 许可证

MIT

## 致谢

- [sherpa-onnx](https://github.com/k2-fsa/sherpa-onnx) — 新一代 Kaldi 语音识别引擎
- [cpal](https://github.com/RustAudio/cpal) — 跨平台音频库
- [enigo](https://github.com/enigo-rs/enigo) — 键盘鼠标模拟
- [minifb](https://github.com/emoon/rust_minifb) — 轻量级窗口库
- [jieba](https://github.com/fxsjy/jieba) — 中文分词（Bigram 语料来源）