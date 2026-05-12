# Voice IME — 完全离线的中文语音输入法

基于 **Rust + Sherpa-ONNX** 的轻量级语音输入工具。无需联网，开箱即用。

## 特性

- **完全离线** — 所有推理在本地 CPU 完成，不发送任何数据到网络
- **双模型支持** — Transducer（流式，支持热词）和 SenseVoice（更准确，自带标点），可在配置中切换
- **零安装** — 单 exe + 模型文件，无需 VC++ 运行库、.NET 或 Python
- **按住说话** — 按住热键录音，松开即出结果，流式解码延迟极低
- **自动适配** — 兼容所有 Windows 麦克风设备，自动重采样到 16kHz
- **波纹可视化** — 屏幕顶部小窗口实时显示语音波纹，右键切换样式或退出
- **文字预览** — Win32 原生窗口渲染中文，超过 20 字自动滚动
- **自学习纠错** — 按 Esc 纠正识别错误，系统自动学习：corrections + bigram 频率 + 热词
- **上下文纠错** — Bigram 统计纠错 + 同音字泛化，越用越准
- **语音命令** — 说出关键词执行系统动作（打开程序、模拟快捷键等）
- **浏览器设置** — 右键菜单打开设置页面，可视化编辑所有配置

## 系统要求

- Windows 11/10 x86_64
- 任意麦克风
- 约 420MB 磁盘空间（含双模型）

## 快速开始

### 1. 下载模型文件

**Transducer 模型**（流式，支持热词）：

| 下载地址 | 说明 |
|----------|------|
| [GitHub (tar.bz2)](https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/sherpa-onnx-streaming-zipformer-bilingual-zh-en-2023-02-20.tar.bz2) | 中英双语流式模型，~370MB |
| [HuggingFace 镜像](https://huggingface.co/csukuangfj/sherpa-onnx-streaming-zipformer-bilingual-zh-en-2023-02-20) | 国内访问更快 |

解压后放入 `models/` 目录：
```
models/
├── encoder-epoch-99-avg-1.int8.onnx
├── decoder-epoch-99-avg-1.onnx
├── joiner-epoch-99-avg-1.int8.onnx
└── tokens.txt
```

**SenseVoice 模型**（更准确，自带标点，推荐）：

| 下载地址 | 说明 |
|----------|------|
| [GitHub (tar.bz2)](https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/sherpa-onnx-sense-voice-zh-en-ja-ko-yue-2024-07-17.tar.bz2) | 中英日韩粤多语言，~1GB |
| [int8 量化版](https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/sherpa-onnx-sense-voice-zh-en-ja-ko-yue-2024-07-17-int8.tar.bz2) | 量化版更小 |

解压后重命名放入 `models/` 目录：
```
models/
├── sense_voice_model.int8.onnx    (原名 model.int8.onnx)
└── sense_voice_tokens.txt          (原名 tokens.txt，注意不要覆盖 Transducer 的)
```

> 其他可选模型见 [sherpa-onnx 模型列表](https://github.com/k2-fsa/sherpa-onnx/releases/tag/asr-models)

### 2. 运行

```
voice_ime.exe
```

### 3. 使用

| 操作 | 说明 |
|------|------|
| 启动后 | 窗口显示加载进度，模型就绪后自动切换为绿色"就绪"状态 |
| **按住热键** | 按住 F1（可配置）开始录音，波纹窗口显示动画 |
| **松开热键** | 停止录音，识别结果在预览窗口显示 1 秒后自动输出到光标位置 |
| **按 Esc** | 在预览期间按 Esc 弹出纠错对话框，修改后系统自动学习 |
| 右键小窗口 | 切换波纹样式 / 打开设置 / 退出 |
| 说出命令 | 如"打开记事本"、"复制"、"保存"等直接执行系统动作 |

## 波纹窗口

程序启动后在屏幕顶部正中央显示一个小窗口（160×28 像素）：

**启动流程：**

```
┌─────────────────────────────────────────┐
│  ████████████░░░░░░░░░░  80%            │  ← 模型加载中（进度条）
└─────────────────────────────────────────┘
                    ↓
┌─────────────────────────────────────────┐
│  ●─────────────────────────────────     │  ← 就绪（绿色呼吸灯）
└─────────────────────────────────────────┘
                    ↓ 按住热键说话
┌─────────────────────────────────────────┐
│  ～～∿∿～～∿～～∿∿～～∿～～～  │  ← 波纹动画
├─────────────────────────────────────────┤
│  我今天想讨论一下关于人工智能的方案      │  ← Win32 文字预览（自动滚动）
└─────────────────────────────────────────┘
                    ↓ 松开 → 1秒后自动输出
┌─────────────────────────────────────────┐
│  ●─────────────────────────────────     │  ← 回到就绪
└─────────────────────────────────────────┘
```

- **无标题栏**、无边框、纯像素窗口
- **屏幕居中置顶**，不占任务栏位置
- **70% 透明度**，不遮挡工作内容
- **文字预览窗口**：Win32 原生窗口，微软雅黑字体，超 20 字自动从右向左滚动
- **右键弹出菜单**：正弦波 / 柱状条 / 点阵 / 平直线 / 设置 / 退出

## 自学习纠错

### 纠正流程

```
识别结果预览 → 按 Esc → 弹出纠错对话框
┌───────────────────────────────┐
│  纠正识别结果 (Enter=确认)     │
│  ┌───────────────────────────┐│
│  │我就想看看编程的效果怎么样 ││ ← 修改文字
│  └───────────────────────────┘│
│        [确认(Enter)] [取消]   │
└───────────────────────────────┘
```

点击确认后，系统自动执行三重学习：

| 层 | 效果 | 文件 |
|---|---|---|
| **auto_corrections.txt** | 下次直接替换 | `models/auto_corrections.txt` |
| **bigram 频率** | 统计倾向正确词 | `models/bigram.bin` |
| **hotwords.txt** | ASR 解码加分 | `models/hotwords.txt` |

### 上下文窗口 + 同音泛化

纠正 "相→想" 在 "我就相看看" 中时，系统不仅学到精确映射，还自动生成上下文变体：

```
精确:     相 → 想
上下文:   就相看 → 就想看
同音泛化: 就象看 → 就想看
          就像看 → 就想看
          就向看 → 就想看
```

无论 ASR 下次输出哪个同音字，在相同上下文中都会被纠正。**越用越准。**

### 纠错文件

| 文件 | 说明 | 编辑方式 |
|------|------|---------|
| `models/corrections.txt` | 手动维护的纠错映射 | 用户手动编辑 |
| `models/auto_corrections.txt` | 自动学习的纠错映射 | 程序自动追加 |
| `models/homophones.txt` | 同音字映射表（~530 组） | 一般不需要改 |
| `models/bigram.bin` | 二字组合频率表（~53K 条） | 程序自动更新 |

## 语音命令

说出关键词直接执行系统动作，不输入到键盘（严格匹配，完全说出触发词才触发）：

| 说 | 执行 |
|----|------|
| "打开记事本" | 启动 notepad.exe |
| "打开计算器" | 启动 calc.exe |
| "打开百度" | 浏览器打开百度 |
| "复制" | Ctrl+C |
| "粘贴" | Ctrl+V |
| "保存" | Ctrl+S |
| "撤销" | Ctrl+Z |
| "全选" | Ctrl+A |

13 条预置命令，可编辑 `models/commands.toml` 自定义。

## 配置

在程序同目录创建 `voice_ime.toml` 可自定义所有参数：

```toml
[asr]
n_threads = 4                                          # CPU 推理线程数，建议设为 CPU 核心数的一半
model_type = "sense_voice"                             # 模型类型: "transducer"(流式/热词) 或 "sense_voice"(更准/自带标点)
encoder = "encoder-epoch-99-avg-1.int8.onnx"           # Transducer encoder 模型文件名
decoder = "decoder-epoch-99-avg-1.onnx"                # Transducer decoder 模型文件名
joiner = "joiner-epoch-99-avg-1.int8.onnx"             # Transducer joiner 模型文件名
sense_voice_model = "sense_voice_model.int8.onnx"      # SenseVoice 模型文件名
sense_voice_tokens = "sense_voice_tokens.txt"          # SenseVoice 专用词表文件名

[audio]
sample_rate = 16000          # 目标采样率（Hz），模型要求 16000
channels = 1                 # 声道数，1=单声道
vad_threshold = 0.01         # VAD 能量阈值（0~1），越小越灵敏，过小会误触
silence_duration_ms = 500    # 静音持续多少毫秒后认为一句话结束
min_speech_duration_ms = 300 # 最短语音段（毫秒），过滤掉太短的噪声
buffer_frames = 1024         # 音频缓冲区帧数

[hotkey]
toggle = "F1"                # 按住录音的热键，支持组合键如 "Ctrl+Alt+V"
```

### 参数详解

#### `[asr]` 语音识别

| 参数 | 说明 |
|------|------|
| `n_threads` | CPU 推理线程数。4 核 CPU 设 2-4，8 核设 4-6。过高反而降速 |
| `model_type` | `"transducer"` = 流式解码（边说边出字，支持热词，但中文准确率较低）；`"sense_voice"` = 离线解码（说完后出字，准确率高，自带标点） |
| `encoder/decoder/joiner` | Transducer 模型的三个文件名，放在 `models/` 目录下 |
| `sense_voice_model` | SenseVoice 模型文件名 |
| `sense_voice_tokens` | SenseVoice 专用的词表文件（不同于 Transducer 的 tokens.txt） |

#### `[audio]` 音频采集

| 参数 | 说明 |
|------|------|
| `sample_rate` | 16000 是模型要求的采样率，不要改。程序会自动把设备的 48kHz 重采样到 16kHz |
| `channels` | 固定 1（单声道），程序会自动混合多声道 |
| `vad_threshold` | 能量阈值，用于能量 VAD 端点检测。0.01 适合安静环境，嘈杂环境可调高到 0.03-0.05 |
| `silence_duration_ms` | 说话中途停顿超过此时间判定为一句话结束。500ms 适合日常对话，正式场合可设 800-1000 |
| `min_speech_duration_ms` | 过滤短于此时间的音频段（防止咳嗽、敲桌子等噪声被误识别） |
| `buffer_frames` | 音频采集缓冲区大小，一般不需要改 |

#### `[hotkey]` 热键

| 参数 | 说明 |
|------|------|
| `toggle` | 按住录音的热键。支持单键（如 `F1`）和组合键（如 `Ctrl+Alt+V`、`Shift+F2`） |

## 热词

`models/hotwords.txt` 支持逐词权重（仅 Transducer 模式生效）：

```
# 容易混淆的词设高权重
编程 6.0
程序 5.0

# 普通热词使用默认权重 3.0
人工智能
大模型
```

权重越高，ASR 越倾向输出该词。容易被同音词替换的词建议 5.0-8.0。

## 技术架构

```
按住热键 → 麦克风 (48kHz)
    ↓ cpal 采集 + 自动重采样 (16kHz)
流式解码 (Transducer) 或 攒音频 (SenseVoice)
    ↓ 松开热键
最终识别文本
    ↓
Bigram 统计纠错 (53K 二字频率对)
    ↓
corrections.txt + auto_corrections.txt 精确替换
    ↓
自动标点 (Transducer: 规则标点 / SenseVoice: 模型自带)
    ↓
语音命令匹配 → 命中则执行动作，不输出
    ↓
Win32 文字预览窗口 (微软雅黑, 自动滚动)
    ↓ 1秒后 (或按 Esc 纠正)
enigo 键盘模拟输出到光标位置
    ↓
更新 bigram 频率 (自学习)
```

| 模块 | 技术 | 说明 |
|------|------|------|
| 音频采集 | cpal 0.15 | 自适应设备格式，线性重采样 |
| 语音识别 | sherpa-onnx 1.13 | Transducer 或 SenseVoice 双模型 |
| 热词增强 | modified_beam_search | Transducer 模式下提升专业术语识别率 |
| Bigram 纠错 | jieba 语料 53K 对 | 同音字频率比较 + 同音泛化 |
| 自学习 | Esc 纠正触发 | auto_corrections + bigram 频率 + 热词追加 |
| 文本输出 | enigo 0.6 | Unicode 键盘模拟 |
| 波纹 UI | minifb 0.27 | 无边框置顶窗口，4 种样式 |
| 文字预览 | Win32 API | DrawTextW 原生渲染，自动滚动 |
| 设置页面 | 内置 HTTP | 浏览器编辑配置，127.0.0.1:17630 |
| 编译 | Rust + 静态 CRT | 零运行时依赖 |

## 从源码编译

```bash
# 安装 Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# 编译（静态链接 CRT，无需 vcredist）
cargo build --release

# 运行测试
cargo test

# 输出在 target/release/voice_ime.exe
```

## 目录结构

```
voice_ime/
├── .cargo/config.toml        ← 静态 CRT 链接配置
├── Cargo.toml
├── src/
│   ├── main.rs               ← 入口 + App 状态机
│   ├── lib.rs                ← 模块声明
│   ├── asr.rs                ← 双模式 ASR（Transducer / SenseVoice）
│   ├── audio.rs              ← cpal 音频采集 + 重采样 + compute_energy
│   ├── output.rs             ← enigo 键盘输出
│   ├── correct.rs            ← 统一纠错（corrections + bigram + 同音泛化 + 自学习）
│   ├── bigram.rs             ← Bigram 频率表（加载/查询/运行时更新）
│   ├── punctuation.rs        ← 规则标点（。？！）
│   ├── commands.rs           ← 语音命令引擎
│   ├── hotkey.rs             ← 全局热键（按住/松开检测 + Esc）
│   ├── ui.rs                 ← 波纹可视化窗口
│   ├── preview.rs            ← Win32 文字预览窗口（自动滚动）
│   └── settings.rs           ← 浏览器设置页面
├── models/
│   ├── tokens.txt            ← Transducer 词表
│   ├── sense_voice_tokens.txt ← SenseVoice 词表
│   ├── hotwords.txt          ← 热词（可编辑）
│   ├── corrections.txt       ← 手动纠错映射（可编辑）
│   ├── auto_corrections.txt  ← 自动学习纠错（程序追加）
│   ├── homophones.txt        ← 同音字映射表
│   ├── bigram.bin            ← 二字频率表（53K 条）
│   └── commands.toml         ← 语音命令（可编辑）
└── package.bat               ← 打包脚本
```

## 许可证

MIT

## 致谢

- [sherpa-onnx](https://github.com/k2-fsa/sherpa-onnx) — 新一代 Kaldi 语音识别引擎
- [cpal](https://github.com/RustAudio/cpal) — 跨平台音频库
- [enigo](https://github.com/enigo-rs/enigo) — 键盘鼠标模拟
- [minifb](https://github.com/emoon/rust_minifb) — 轻量级窗口库
- [jieba](https://github.com/fxsjy/jieba) — 中文分词词典（Bigram 语料来源）