import grpc
import asyncio
import time
import sys
import liva_engine_pb2
import liva_engine_pb2_grpc

# Force utf-8 for stdout
sys.stdout.reconfigure(encoding='utf-8')

async def run_test():
    channel = grpc.aio.insecure_channel('127.0.0.1:8100')
    stub = liva_engine_pb2_grpc.LivaInferenceServiceStub(channel)

    print("Sending HealthCheck...")
    try:
        health_res = await stub.HealthCheck(liva_engine_pb2.HealthRequest())
        print(f"HealthCheck response: {health_res}")
    except Exception as e:
        print(f"HealthCheck failed: {e}")

    print("\nSending Chat request...")
    chat_req = liva_engine_pb2.ChatCompletionRequest(
        model="liva-native",
        messages=[
            liva_engine_pb2.ChatMessage(role="user", content="Test prompt, say hello!")
        ],
        max_tokens=50
    )
    
    try:
        res = await stub.Chat(chat_req)
        print(f"Chat response: {res.choices[0].message.content}")
    except Exception as e:
        print(f"Chat failed: {e}")

    print("\nSending StreamChat request...")
    stream_req = liva_engine_pb2.ChatCompletionRequest(
        model="liva-native",
        messages=[
            liva_engine_pb2.ChatMessage(role="user", content="Count from 1 to 5.")
        ],
        max_tokens=50
    )
    
    try:
        async for chunk in stub.StreamChat(stream_req):
            if chunk.choices and len(chunk.choices) > 0:
                print(chunk.choices[0].delta.content, end='', flush=True)
        print("\nStream done.")
    except Exception as e:
        print(f"\nStreamChat failed: {e}")

    await channel.close()

if __name__ == '__main__':
    asyncio.run(run_test())
