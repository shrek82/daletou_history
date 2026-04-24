#!/bin/bash
set -e

BINARY="target/release/examples/ai_pick"

# 编译
echo "正在编译 ai_pick (release)..."
cargo build --example ai_pick --release
echo "编译成功: $BINARY"

# 运行模式
if [ "$1" = "--server" ]; then
    echo "启动 HTTP 服务 (端口 8888)..."
    exec "$BINARY" --server
elif [ "$1" = "--run" ]; then
    exec "$BINARY"
fi

# 无参数：仅提示用法
echo ""
echo "运行方式:"
echo "  终端模式:    $BINARY"
echo "  HTTP服务:    $BINARY --server"
echo "  自定义端口:  $BINARY --server --port 9000"
