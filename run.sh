#!/bin/bash
# Nimbus 桌宠一键编译与启动脚本
set -euo pipefail

SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
cd "$SCRIPT_DIR"
LOG_FILE="$SCRIPT_DIR/nimbus.log"
SERVICE_TARGET="gui/$(id -u)/fun.hackerlife.nimbus-pet"

die() { echo "❌ $*" >&2; exit 1; }

case "${1:-}" in
    ""|--daemon|-d) ;;
    *) die "用法: $0 [--daemon|-d]" ;;
esac

# 保留调用者的代理设置；构建失败时不打断已经运行的桌宠。
echo "正在编译 Nimbus 桌宠 (Release 模式)..."
cargo build --release --locked || die "编译失败，现有桌宠仍保持运行。"
METADATA_DIR=$(cargo metadata --offline --no-deps --format-version 1 | python3 -c '
import json, os, sys
path = json.load(sys.stdin)["target_directory"]
if not isinstance(path, str) or not os.path.isabs(path):
    raise SystemExit("Cargo target_directory 必须是绝对路径")
print(path)
') || die "无法确定 Cargo 构建目录，现有桌宠仍保持运行。"
PET_BIN="$METADATA_DIR/release/nimbus"
[ -x "$PET_BIN" ] || die "未找到可执行产物: $PET_BIN"

# 停止服务前完成枚举及路径核对，检查失败时保留现有桌宠。
TARGET_PIDS=""
if PIDS=$(pgrep -u "$(id -u)" -x nimbus); then
    for pid in $PIDS; do
        if ! executable=$(ps -p "$pid" -o comm=); then
            kill -0 "$pid" 2>/dev/null && die "无法确认 Nimbus (PID $pid) 的可执行路径。"
            continue
        fi
        [ "$executable" = "$PET_BIN" ] || continue
        TARGET_PIDS="$TARGET_PIDS $pid"
    done
else
    status=$?
    [ "$status" -eq 1 ] || die "无法检查已有 Nimbus 进程 (pgrep: $status)。"
fi

if launchctl print "$SERVICE_TARGET" >/dev/null 2>&1; then
    launchctl bootout "$SERVICE_TARGET" || die "无法停止已注册的 Nimbus 服务。"
fi

# 只终止仍存活的匹配进程；bootout 或 TERM 竞态中的自然退出视为成功。
for pid in $TARGET_PIDS; do
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

if [ "${1:-}" = "--daemon" ] || [ "${1:-}" = "-d" ]; then
    echo "正在后台启动 Nimbus，日志: $LOG_FILE"
    nohup "$PET_BIN" </dev/null >"$LOG_FILE" 2>&1 &
    pet_pid=$!
    sleep 0.3
    kill -0 "$pet_pid" 2>/dev/null || die "Nimbus 启动后已退出，请查看 ${LOG_FILE}。"
    echo "Nimbus 已启动 (PID $pet_pid)。"
else
    echo "在此终端输入文字按 [回车] 与桌宠聊天。"
    exec "$PET_BIN"
fi
