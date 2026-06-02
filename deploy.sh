#!/bin/bash
# Nimbus 桌宠 macOS Launchd 一键部署与服务管理脚本

SERVICE_NAME="fun.hackerlife.nimbus-pet"
PLIST_FILE="$HOME/Library/LaunchAgents/${SERVICE_NAME}.plist"
SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
LOG_FILE="${SCRIPT_DIR}/nimbus.log"

# 设置代理提速，遵照全局规则
export http_proxy=http://127.0.0.1:8228
export https_proxy=http://127.0.0.1:8228

# 1. 查找二进制路径
METADATA_DIR=$(cargo metadata --format-version 1 | tr -d '\n' | sed -E 's/.*"target_directory":"([^"]*)".*/\1/' 2>/dev/null)
if [ -n "$METADATA_DIR" ]; then
    PET_BIN="$METADATA_DIR/release/nimbus"
else
    PET_BIN="${SCRIPT_DIR}/target/release/nimbus"
fi

install_service() {
    echo "=================================================="
    echo "⚙️ 开始一键编译与 macOS 后台服务部署..."
    echo "=================================================="

    # 先停掉现有的非 launchd 进程或已存在的 launchd 进程
    uninstall_service --quiet

    # 1. 编译 release 产物
    echo "Step 1: 正在编译桌宠 (Release 模式)..."
    cargo build --release
    if [ $? -ne 0 ]; then
        echo "❌ 编译失败，请检查编译错误。"
        exit 1
    fi
    echo "✓ 编译成功! 二进制路径: $PET_BIN"

    # 2. 生成 launchd .plist 配置文件
    echo "Step 2: 正在生成 macOS LaunchAgent 配置文件..."
    cat <<EOF > "$PLIST_FILE"
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>${SERVICE_NAME}</string>
    <key>ProgramArguments</key>
    <array>
        <string>${PET_BIN}</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
    <key>KeepAlive</key>
    <true/>
    <key>WorkingDirectory</key>
    <string>${SCRIPT_DIR}</string>
    <key>StandardOutPath</key>
    <string>${LOG_FILE}</string>
    <key>StandardErrorPath</key>
    <string>${LOG_FILE}</string>
</dict>
</plist>
EOF
    chmod 644 "$PLIST_FILE"
    echo "✓ 配置文件已写入: $PLIST_FILE"

    # 3. 启动并注册服务
    echo "Step 3: 正在向 macOS 系统注册并启动后台常驻服务..."
    launchctl bootstrap "gui/$(id -u)" "$PLIST_FILE"
    if [ $? -eq 0 ]; then
        echo "=================================================="
        echo "🎉 部署成功! Nimbus 桌宠现已作为系统服务运行。"
        echo "🛡️ 特性: 开机自动启动 | 异常崩溃自动拉起(KeepAlive)"
        echo "📝 日志输出文件: $LOG_FILE"
        echo "=================================================="
    else
        # 兼容老版本 macOS 载入方式
        launchctl load "$PLIST_FILE"
        echo "✓ 服务已载入运行。"
    fi
}

uninstall_service() {
    local quiet=$1
    if [ -z "$quiet" ]; then
        echo "停止并卸载 Nimbus 后台常驻服务..."
    fi

    # 清理 launchd 注册
    if [ -f "$PLIST_FILE" ]; then
        launchctl bootout "gui/$(id -u)" "$PLIST_FILE" 2>/dev/null
        launchctl unload "$PLIST_FILE" 2>/dev/null
        rm -f "$PLIST_FILE"
        if [ -z "$quiet" ]; then
            echo "✓ launchd 后台配置已清除。"
        fi
    fi

    # 清理可能残留的孤立进程
    PIDS=$(pgrep -x "nimbus")
    if [ -n "$PIDS" ]; then
        echo "$PIDS" | xargs kill -9 2>/dev/null
    fi
    if [ -z "$quiet" ]; then
        echo "✓ 进程已全部清理干净。"
    fi
}

restart_service() {
    echo "正在重启 Nimbus 桌宠后台服务..."
    if [ -f "$PLIST_FILE" ]; then
        launchctl bootout "gui/$(id -u)" "$PLIST_FILE" 2>/dev/null
        launchctl unload "$PLIST_FILE" 2>/dev/null
        sleep 0.5
        launchctl bootstrap "gui/$(id -u)" "$PLIST_FILE" 2>/dev/null
        launchctl load "$PLIST_FILE" 2>/dev/null
        echo "✓ 服务重启指令已发送。"
    else
        echo "❌ 未检测到已安装的后台服务，请先执行部署。"
    fi
}

status_service() {
    echo "--- Nimbus 后台服务状态 ---"
    if [ -f "$PLIST_FILE" ]; then
        echo "配置文件: 已安装"
        PIDS=$(pgrep -x "nimbus")
        if [ -n "$PIDS" ]; then
            echo "运行状态: 正在运行 (PID: $PIDS)"
        else
            echo "运行状态: 未运行 (或正在等待 launchd 调度)"
        fi
    else
        echo "运行状态: 未部署"
    fi
}

case "$1" in
    uninstall)
        uninstall_service
        ;;
    restart)
        restart_service
        ;;
    status)
        status_service
        ;;
    install|*)
        install_service
        ;;
esac
