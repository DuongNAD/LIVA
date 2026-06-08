#!/bin/bash
export AI_MODELS_DIR=/Users/duongnad/AI_Models
export ROUTER_MODEL_NAME=gemma-4-12B-it-Q6_K.gguf

echo "=== Starting LIVA Native Engine in background ==="
./venv/bin/python liva_native_engine.py > engine_m4_test.log 2>&1 &
ENGINE_PID=$!
echo "Engine started with PID: $ENGINE_PID"

echo "=== Waiting for gRPC Server to start ==="
for i in {1..60}; do
    if grep -q "Server listening" engine_m4_test.log; then
        echo "Server is ready!"
        break
    fi
    sleep 1
done

echo "=== Running gRPC Client Test Suite ==="
./venv/bin/python tests/test_grpc_client.py > test_suite_output.log 2>&1
TEST_EXIT_CODE=$?
echo "Test suite exit code: $TEST_EXIT_CODE"

echo "=== Stopping Native Engine (PID $ENGINE_PID) ==="
kill -9 $ENGINE_PID
echo "Engine process killed."

echo "=== Verification Command Log ==="
cat test_suite_output.log
