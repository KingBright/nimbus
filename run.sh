#!/bin/bash
# Nimbus 桌宠一键编译与启动脚本

LOG_FILE="nimbus.log"

# 1. 查找并清理已有进程
SERVICE_NAME="fun.hackerlife.nimbus-pet"
PLIST_FILE="$HOME/Library/LaunchAgents/${SERVICE_NAME}.plist"
if [ -f "$PLIST_FILE" ]; then
    echo "发现已注册的 launchd 服务，正在停止服务以防冲突..."
    launchctl bootout "gui/$(id -u)" "$PLIST_FILE" 2>/dev/null
    launchctl unload "$PLIST_FILE" 2>/dev/null
fi

PIDS=$(pgrep -x "nimbus")
if [ -n "$PIDS" ]; then
    echo "发现正在运行的 Nimbus 桌宠，正在终止..."
    echo "$PIDS" | xargs kill -9 2>/dev/null
    sleep 0.5
fi

# 2. 编译项目
echo "正在编译 Nimbus 桌宠 (Release 模式)..."
# 设置代理提速，遵照 user_global 规则
export http_proxy=http://127.0.0.1:8228
export https_proxy=http://127.0.0.1:8228

cargo build --release
if [ $? -ne 0 ]; then
    echo "❌ 编译失败，请检查编译错误。"
    exit 1
fi

# 3. 定位当前编译的二进制路径 (使用 sed 确保提取准确)
METADATA_DIR=$(cargo metadata --format-version 1 | tr -d '\n' | sed -E 's/.*"target_directory":"([^"]*)".*/\1/' 2>/dev/null)
if [ -n "$METADATA_DIR" ]; then
    PET_BIN="$METADATA_DIR/release/nimbus"
else
    PET_BIN="target/release/nimbus"
fi

if [ ! -f "$PET_BIN" ]; then
    echo "❌ 未找到当前编译的二进制文件：$PET_BIN"
    exit 1
fi

echo "编译产物定位成功: $PET_BIN"

# 4. 运行配置
if [ "$1" = "--daemon" ] || [ "$1" = "-d" ]; then
    echo "🚀 以后台守护进程模式启动 Nimbus 桌宠..."
    echo "标准输出与输入将被重定向到: $(pwd)/$LOG_FILE"
    nohup "$PET_BIN" > "$LOG_FILE" 2>&1 &
    echo "Nimbus 桌宠已在后台运行。可以使用 'tail -f $LOG_FILE' 监控日志。"
else
    echo "🚀 以前台终端交互模式启动 Nimbus 桌宠..."
    echo "您可以在此终端直接输入文字按 [回车] 与桌宠进行对话交流。"
    echo "=================================================="
    "$PET_BIN"
fi
