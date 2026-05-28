#!/bin/bash
# Nimbus 桌宠一键编译与启动脚本

LOG_FILE="nimbus.log"

# 1. 查找并清理已有进程
PIDS=$(pgrep -x "nimbus")
if [ -n "$PIDS" ]; then
    echo "发现正在运行的 Nimbus 桌宠，正在终止..."
    echo "$PIDS" | xargs kill
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

# 3. 自动探测二进制路径
METADATA_DIR=$(cargo metadata --format-version 1 | grep -o '"target_directory":"[^"]*' | cut -d'"' -f4)

if [ -n "$CARGO_TARGET_DIR" ] && [ -f "$CARGO_TARGET_DIR/release/nimbus" ]; then
    PET_BIN="$CARGO_TARGET_DIR/release/nimbus"
elif [ -f "$METADATA_DIR/release/nimbus" ]; then
    PET_BIN="$METADATA_DIR/release/nimbus"
elif [ -f "target/release/nimbus" ]; then
    PET_BIN="target/release/nimbus"
else
    if [ -f "/Users/jinliang/rust-target/release/nimbus" ]; then
        PET_BIN="/Users/jinliang/rust-target/release/nimbus"
    elif [ -f "/Users/jinliang/rust-target-nimbus/release/nimbus" ]; then
        PET_BIN="/Users/jinliang/rust-target-nimbus/release/nimbus"
    else
        echo "❌ 未找到编译好的二进制文件！"
        exit 1
    fi
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
