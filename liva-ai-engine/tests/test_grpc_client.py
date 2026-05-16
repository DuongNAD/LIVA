import unittest
import grpc
import asyncio
import sys
import os

# Ensure the root directory is in sys.path to import liva_engine_pb2
sys.path.append(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

import liva_engine_pb2
import liva_engine_pb2_grpc

# Force utf-8 for stdout
sys.stdout.reconfigure(encoding='utf-8')

class TestGRPCClient(unittest.IsolatedAsyncioTestCase):
    async def asyncSetUp(self):
        # We need a longer timeout because the Native Engine might be thinking
        self.channel = grpc.aio.insecure_channel('127.0.0.1:8100')
        self.stub = liva_engine_pb2_grpc.LivaInferenceServiceStub(self.channel)

    async def asyncTearDown(self):
        await self.channel.close()

    async def test_grpc_health_check(self):
        try:
            health_res = await self.stub.HealthCheck(liva_engine_pb2.HealthRequest())
            self.assertTrue(health_res.alive)
        except Exception as e:
            self.fail(f"HealthCheck failed: {e}")

    async def test_grpc_chat(self):
        chat_req = liva_engine_pb2.ChatCompletionRequest(
            model="liva-native",
            messages=[
                liva_engine_pb2.ChatMessage(role="user", content="Test prompt, say hello!")
            ],
            max_tokens=50
        )
        try:
            res = await self.stub.Chat(chat_req)
            self.assertIsNotNone(res.choices)
            self.assertGreater(len(res.choices), 0)
        except Exception as e:
            self.fail(f"Chat failed: {e}")

if __name__ == '__main__':
    unittest.main()
