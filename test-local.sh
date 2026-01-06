#!/bin/bash
# Quick local test script for C2 simulator
# This runs the server and allows you to test with the client

set -e

echo "Building C2 Simulator..."
cargo build --release

echo ""
echo "Starting c2-server in background..."
echo "Server will listen on 127.0.0.1:5000"
echo "Logs will be in ./server.log"
echo ""

# Kill any existing server
pkill -f c2-server || true
sleep 1

# Start server in background
RUST_LOG=info ./target/release/c2-server > server.log 2>&1 &
SERVER_PID=$!

echo "Server PID: $SERVER_PID"
sleep 2

# Check if server started successfully
if ! kill -0 $SERVER_PID 2>/dev/null; then
    echo "ERROR: Server failed to start. Check server.log for details."
    cat server.log
    exit 1
fi

echo "Server started successfully!"
echo ""
echo "You can now run the client in another terminal:"
echo "  ./target/release/c2-client"
echo ""
echo "Or test with multiple clients:"
echo "  ./target/release/c2-client &"
echo "  ./target/release/c2-client &"
echo ""
echo "To stop the server:"
echo "  kill $SERVER_PID"
echo ""
echo "Server logs are being written to ./server.log"
echo "Watch logs with: tail -f server.log"
echo ""

# Keep script running and tail server logs
tail -f server.log
