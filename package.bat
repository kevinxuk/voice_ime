@echo off
chcp 65001 >nul 2>&1
setlocal

set "OUT=dist\voice_ime"

echo ========================================
echo   Voice IME 打包脚本
echo ========================================

:: 清理
if exist dist rmdir /s /q dist
mkdir "%OUT%\models"

:: 复制 exe
copy /y "target\release\voice_ime.exe" "%OUT%\" >nul

:: 复制 sherpa-onnx DLL（shared 模式需要，static 模式可跳过）
if exist "target\release\*.dll" (
    copy /y "target\release\*.dll" "%OUT%\" >nul
    echo [OK] DLL 已复制
)

:: 复制模型
copy /y "models\encoder-epoch-99-avg-1.int8.onnx" "%OUT%\models\" >nul
copy /y "models\decoder-epoch-99-avg-1.onnx"      "%OUT%\models\" >nul
copy /y "models\joiner-epoch-99-avg-1.int8.onnx"   "%OUT%\models\" >nul
copy /y "models\tokens.txt"                         "%OUT%\models\" >nul
if exist "models\bpe.model" copy /y "models\bpe.model" "%OUT%\models\" >nul

:: 创建启动说明
(
echo Voice IME - 离线中文语音输入法
echo ================================
echo.
echo 使用方法：
echo   双击 voice_ime.exe 启动
echo   Enter = 开始/暂停语音识别
echo   Q     = 退出
echo   对着麦克风说中文，识别结果自动输入到光标位置
echo.
echo 完全离线运行，无需联网。
echo.
echo 配置：
echo   创建 voice_ime.toml 可自定义参数。
) > "%OUT%\使用说明.txt"

:: 创建默认配置
(
echo [asr]
echo n_threads = 4
echo decoding_method = "greedy_search"
echo model_type = "transducer"
echo.
echo [audio]
echo sample_rate = 16000
echo channels = 1
echo vad_threshold = 0.01
echo silence_duration_ms = 500
echo min_speech_duration_ms = 300
echo buffer_frames = 1024
echo use_vad_endpoint = false
) > "%OUT%\voice_ime.toml"

:: 统计
echo.
echo ========================================
echo   打包完成！
echo ========================================
echo.
echo 输出目录: %OUT%
echo.

:: 列出文件和大小
dir /s "%OUT%" | findstr /i "File(s)"
echo.
echo 可直接将 %OUT% 目录拷贝到任何 Windows 11 x64 电脑上运行。
echo 无需安装任何运行库。

endlocal