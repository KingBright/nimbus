#!/bin/bash
# Nimbus 桌宠 macOS LaunchAgent 部署与服务管理
set -euo pipefail

SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
cd "$SCRIPT_DIR"
SERVICE_NAME="fun.hackerlife.nimbus-pet"
SERVICE_DOMAIN="gui/$(id -u)"
SERVICE_TARGET="$SERVICE_DOMAIN/$SERVICE_NAME"
PLIST_FILE="$HOME/Library/LaunchAgents/$SERVICE_NAME.plist"
LOG_FILE="$SCRIPT_DIR/nimbus.log"

die() { echo "❌ $*" >&2; exit 1; }

stop_service() {
    if launchctl print "$SERVICE_TARGET" >/dev/null 2>&1; then
        launchctl bootout "$SERVICE_TARGET" || die "无法停止 Nimbus 服务。"
    fi
}

install_service() {
    # 保留调用者的代理设置；先完成构建及配置验证，再停止旧进程。
    echo "正在编译 Nimbus 桌宠 (Release 模式)..."
    cargo build --release --locked || die "编译失败，现有服务未改动。"
    local metadata_dir pet_bin temp_plist pids target_pids pid executable attempt status
    metadata_dir=$(cargo metadata --offline --no-deps --format-version 1 | python3 -c '
import json, os, sys
path = json.load(sys.stdin)["target_directory"]
if not isinstance(path, str) or not os.path.isabs(path):
    raise SystemExit("Cargo target_directory 必须是绝对路径")
print(path)
') || die "无法确定 Cargo 构建目录，现有服务未改动。"
    pet_bin="$metadata_dir/release/nimbus"
    [ -x "$pet_bin" ] || die "未找到可执行产物: $pet_bin"

    mkdir -p "$(dirname "$PLIST_FILE")"
    temp_plist=$(mktemp "${PLIST_FILE}.XXXXXX")
    trap 'rm -f "${temp_plist:-}"' EXIT
    python3 - "$temp_plist" "$SERVICE_NAME" "$pet_bin" "$SCRIPT_DIR" "$LOG_FILE" <<'PY'
import plistlib
import sys
path, label, binary, directory, log = sys.argv[1:]
with open(path, "wb") as output:
    plistlib.dump({
        "Label": label,
        "ProgramArguments": [binary],
        "RunAtLoad": True,
        "KeepAlive": {"SuccessfulExit": False},
        "WorkingDirectory": directory,
        "StandardOutPath": log,
        "StandardErrorPath": log,
    }, output)
PY
    chmod 644 "$temp_plist"

    # 停止服务前完成进程检查，保留其他 checkout 的同名进程。
    target_pids=""
    if pids=$(pgrep -u "$(id -u)" -x nimbus); then
        for pid in $pids; do
            if ! executable=$(ps -p "$pid" -o comm=); then
                kill -0 "$pid" 2>/dev/null && die "无法确认 Nimbus (PID $pid) 的可执行路径。"
                continue
            fi
            [ "$executable" = "$pet_bin" ] || continue
            target_pids="$target_pids $pid"
        done
    else
        status=$?
        [ "$status" -eq 1 ] || die "无法检查已有 Nimbus 进程 (pgrep: $status)。"
    fi
    stop_service
    for pid in $target_pids; do
        kill -0 "$pid" 2>/dev/null || continue
        if ! kill -TERM "$pid" 2>/dev/null; then
            kill -0 "$pid" 2>/dev/null && die "无法停止 Nimbus (PID $pid)。"
            continue
        fi
        for ((attempt = 0; attempt < 50; attempt++)); do
            kill -0 "$pid" 2>/dev/null || break
            sleep 0.1
        done
        if kill -0 "$pid" 2>/dev/null; then
            die "Nimbus (PID $pid) 未在 5 秒内退出，请关闭后重试。"
        fi
    done
    mv "$temp_plist" "$PLIST_FILE"
    trap - EXIT
    launchctl bootstrap "$SERVICE_DOMAIN" "$PLIST_FILE" || die "服务注册失败，请检查上方 launchctl 错误。"
    launchctl print "$SERVICE_TARGET" >/dev/null || die "服务注册后无法读取状态。"
    echo "Nimbus 服务已注册；登录时启动，异常退出时重启，菜单正常退出后保持关闭。"
    echo "日志: $LOG_FILE"
}

case "${1:-install}" in
    install) install_service ;;
    uninstall)
        stop_service
        rm -f "$PLIST_FILE"
        echo "Nimbus 后台服务已卸载。"
        ;;
    restart)
        [ -f "$PLIST_FILE" ] || die "尚未安装 Nimbus 后台服务，请先执行 deploy.sh install。"
        stop_service
        launchctl bootstrap "$SERVICE_DOMAIN" "$PLIST_FILE" || die "Nimbus 服务重启失败。"
        launchctl print "$SERVICE_TARGET" >/dev/null || die "服务重启后无法读取状态。"
        echo "Nimbus 服务已重新注册启动。"
        ;;
    status)
        if launchctl print "$SERVICE_TARGET"; then
            :
        elif [ -f "$PLIST_FILE" ]; then
            echo "Nimbus 已安装，当前未加载。"
        else
            echo "Nimbus 尚未安装。"
        fi
        ;;
    *) die "用法: $0 [install|uninstall|restart|status]" ;;
esac
