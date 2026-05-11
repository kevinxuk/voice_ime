# Voice IME — 完全离线的中文语音输入法

基于 **Rust + Sherpa-ONNX** 的轻量级语音输入工具。无需联网，开箱即用。

## 特性

- **完全离线** — 所有推理在本地 CPU 完成，不发送任何数据到网络
- **中文优化** — 使用 Zipformer Transducer 中英双语模型，中文识别准确率高
- **零安装** — 单 exe + 模型文件，无需 VC++ 运行库、.NET 或 Python
- **低延迟** — 流式 ASR + 能量 VAD 端点检测，说完即出结果
- **自动适配** — 兼容所有 Windows 麦克风设备，自动重采样到 16kHz
- **波纹可视化** — 屏幕顶部小窗口实时显示语音波纹，右键切换样式或退出
- **自学习** — 自动记录词频，支持用户自定义纠错映射
- **热词增强** — 预置科技/金融/品牌等 170+ 热词，提升专业术语识别率

## 系统要求

- Windows 11/10 x86_64
- 任意麦克风
- 约 220MB 磁盘空间（含模型）

## 快速开始

### 1. 下载模型文件

从 GitHub 下载预训练模型（**约 370MB**，下载一次即可永久离线使用）：

**推荐模型：sherpa-onnx-streaming-zipformer-bilingual-zh-en-2023-02-20**

| 下载地址 | 说明 |
|----------|------|
| [GitHub Releases (tar.bz2)](https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/sherpa-onnx-streaming-zipformer-bilingual-zh-en-2023-02-20.tar.bz2) | 中英双语流式模型，~370MB |
| [HuggingFace 镜像](https://huggingface.co/csukuangfj/sherpa-onnx-streaming-zipformer-bilingual-zh-en-2023-02-20) | 同上，国内访问更快 |

解压后将以下文件放入程序目录下的 `models/` 文件夹：

```
models/
├── encoder-epoch-99-avg-1.int8.onnx   (173 MB, int8量化版)
├── decoder-epoch-99-avg-1.onnx        (13 MB)
├── joiner-epoch-99-avg-1.int8.onnx    (3 MB)
├── tokens.txt                         (56 KB)
├── hotwords.txt                       (热词表，可自行编辑)
└── corrections.txt                    (纠错映射，可自行编辑)
```

> 其他可选模型见 [sherpa-onnx 模型列表](https://github.com/k2-fsa/sherpa-onnx/releases/tag/asr-models)

### 2. 运行

```
voice_ime.exe
```

### 3. 使用

| 操作 | 说明 |
|------|------|
| 启动后 | 默认进入监听状态，屏幕顶部出现波纹小窗口 |
| 说话 | 对着麦克风说中文，识别完毕自动输入到当前光标位置 |
| 右键小窗口 | 切换波纹样式（正弦波/柱状条/点阵/平直线）或退出 |

## 波纹窗口

程序启动后在屏幕顶部正中央显示一个小窗口（120×28 像素）：

```
┌────────────────────────────────────┐
│  ～～∿∿～～∿～～∿∿～～∿～～  │  ← 有语音时显示波纹
└────────────────────────────────────┘
```

- **无标题栏**、无边框、纯像素窗口
- **屏幕居中置顶**，不占任务栏位置
- **70% 透明度**，不遮挡工作内容
- 有语音输入时实时显示能量波纹动画
- 静默时显示平直线
- **右键弹出菜单**（原生 Windows 菜单）：
  - 正弦波 ～～
  - 柱状条 ▐▌
  - 点阵 ·•·
  - 平直线 ──
  - 退出

## 自学习功能

### 词频记录 — `models/word_freq.json`

程序自动维护，记录每次识别的词组和出现频率：

```json
{
  "人工智能": 15,
  "大模型": 8
}
```

### 纠错映射 — `models/corrections.txt`

用户手动编辑，识别结果输出前自动替换错误词：

```
# 格式: 错误词→正确词
人口智能→人工智能
大摸型→大模型
及其学习→机器学习
```

发现识别错误时，加一行即可，重启程序生效。

### 热词增强 — `models/hotwords.txt`

预置 170+ 常用热词，覆盖：

| 分类 | 示例 |
|------|------|
| 科技/AI | 人工智能、大模型、深度学习、微服务 |
| 金融 | 股票、市盈率、融资、涨停 |
| 品牌 | 华为、特斯拉、字节跳动、英伟达 |
| 会议 | 复盘、对齐、闭环、颗粒度 |
| 日常 | 没问题、搞定、确实、基本上 |

每行一个词，可自行添加。

## 配置

在程序同目录创建 `voice_ime.toml` 可自定义参数：

```toml
[asr]
n_threads = 4                    # CPU 线程数
decoding_method = "greedy_search"
model_type = "transducer"

[audio]
sample_rate = 16000
channels = 1
vad_threshold = 0.01             # VAD 灵敏度（越小越灵敏）
silence_duration_ms = 500        # 静音多久后认为说完
min_speech_duration_ms = 300     # 最短语音段（过滤噪声）
buffer_frames = 1024
use_vad_endpoint = false
```

## 技术架构

```
麦克风 (48kHz/2ch)
    ↓ cpal 音频采集 + 自动重采样
单声道 16kHz PCM
    ↓
能量 VAD 端点检测
    ↓ 检测到语音结束
Sherpa-ONNX Transducer 推理 (CPU, modified_beam_search)
    ↓
自学习: 纠错替换 + 词频记录
    ↓
enigo 键盘模拟输出到光标位置
    ↕
UI 波纹窗口 (minifb, 实时能量可视化)
```

| 模块 | 技术 | 说明 |
|------|------|------|
| 音频采集 | cpal 0.15 | 自适应设备格式，线性重采样 |
| 语音检测 | 能量 VAD | 纯计算无需额外模型 |
| 语音识别 | sherpa-onnx 1.13 | Zipformer Transducer, INT8量化 |
| 热词增强 | modified_beam_search | 提升专业术语识别率 |
| 自学习 | JSON 词频 + 纠错表 | 自动记录 + 用户可编辑 |
| 文本输出 | enigo 0.6 | Unicode 键盘模拟，支持中文 |
| 波纹 UI | minifb 0.27 | 无边框置顶小窗口，4种样式 |
| 编译 | Rust + 静态 CRT | 零运行时依赖 |

## 从源码编译

```bash
# 安装 Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# 编译（静态链接 CRT，无需 vcredist）
cargo build --release

# 输出在 target/release/voice_ime.exe (~18MB)
```

## 目录结构

```
voice_ime/
├── .cargo/config.toml     ← 静态 CRT 链接配置
├── Cargo.toml
├── src/
│   ├── main.rs            ← 入口 + 事件循环
│   ├── lib.rs             ← 模块声明
│   ├── asr.rs             ← Sherpa-ONNX 识别引擎
│   ├── audio.rs           ← cpal 音频采集 + 重采样
│   ├── vad.rs             ← 能量 VAD 端点检测
│   ├── output.rs          ← enigo 键盘输出
│   ├── learn.rs           ← 自学习（词频 + 纠错）
│   └── ui.rs              ← 波纹可视化窗口
├── models/
│   ├── tokens.txt         ← 词表
│   ├── hotwords.txt       ← 热词（可编辑）
│   └── corrections.txt    ← 纠错映射（可编辑）
└── package.bat            ← 打包脚本
```

## 许可证

MIT

## 致谢

- [sherpa-onnx](https://github.com/k2-fsa/sherpa-onnx) — 新一代 Kaldi 语音识别引擎
- [cpal](https://github.com/RustAudio/cpal) — 跨平台音频库
- [enigo](https://github.com/enigo-rs/enigo) — 键盘鼠标模拟
- [minifb](https://github.com/emoon/rust_minifb) — 轻量级窗口库