# Voice IME — 完全离线的中文语音输入法

基于 **Rust + Sherpa-ONNX** 的轻量级语音输入工具。无需联网，开箱即用。

## 特性

- **完全离线** — 所有推理在本地 CPU 完成，不发送任何数据到网络
- **中文优化** — 使用 Zipformer Transducer 中英双语模型，中文识别准确率高
- **零安装** — 单 exe + 模型文件，无需 VC++ 运行库、.NET 或 Python
- **低延迟** — 流式 ASR + 能量 VAD 端点检测，说完即出结果
- **自动适配** — 兼容所有 Windows 麦克风设备，自动重采样到 16kHz

## 系统要求

- Windows 11 x86_64（或 Windows 10 1903+）
- 任意麦克风
- 约 200MB 磁盘空间（含模型）

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
├── encoder-epoch-99-avg-1.int8.onnx   (173 MB, int8量化版，推荐)
├── decoder-epoch-99-avg-1.onnx        (13 MB)
├── joiner-epoch-99-avg-1.int8.onnx    (3 MB)
├── tokens.txt                         (56 KB)
└── bpe.model                          (可选)
```

> 如果你想要更小的模型或纯中文模型，可以在 [sherpa-onnx 模型列表](https://github.com/k2-fsa/sherpa-onnx/releases/tag/asr-models) 中选择其他模型。

### 2. 运行

```
voice_ime.exe
```

### 3. 使用

| 操作 | 说明 |
|------|------|
| 启动后 | 默认进入监听状态 |
| Enter | 暂停/恢复语音识别 |
| Q | 退出程序 |
| 说话 | 对着麦克风说中文，识别完毕自动输入到当前光标位置 |

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
    ↓ cpal 音频采集
自动重采样 (16kHz/1ch)
    ↓
能量 VAD 端点检测
    ↓ 检测到语音结束
Sherpa-ONNX Transducer 推理 (CPU)
    ↓
enigo 键盘模拟输出到光标位置
```

| 模块 | 技术 | 说明 |
|------|------|------|
| 音频采集 | cpal 0.15 | 自适应设备格式，线性重采样 |
| 语音检测 | 能量 VAD | 纯计算无需额外模型 |
| 语音识别 | sherpa-onnx 1.13 | Zipformer Transducer, INT8量化 |
| 文本输出 | enigo 0.6 | Unicode 键盘模拟，支持中文 |
| 编译 | Rust + 静态 CRT | 零运行时依赖 |

## 从源码编译

```bash
# 安装 Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# 编译
cargo build --release

# 输出在 target/release/voice_ime.exe
```

## 许可证

MIT

## 致谢

- [sherpa-onnx](https://github.com/k2-fsa/sherpa-onnx) — 新一代 Kaldi 语音识别引擎
- [cpal](https://github.com/RustAudio/cpal) — 跨平台音频库
- [enigo](https://github.com/enigo-rs/enigo) — 键盘鼠标模拟