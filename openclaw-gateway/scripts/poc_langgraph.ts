import { StateGraph, MemorySaver, Annotation, messagesStateReducer } from "@langchain/langgraph";
import { ChatOpenAI } from "@langchain/openai";
import { tool } from "@langchain/core/tools";
import { z } from "zod";
import { HumanMessage, BaseMessage } from "@langchain/core/messages";
import { ToolNode } from "@langchain/langgraph/prebuilt";
import * as dotenv from "dotenv";

dotenv.config();

// 1. Define State: Lưu trữ mảng messages nối tiếp nhau
const StateAnnotation = Annotation.Root({
  messages: Annotation<BaseMessage[]>({
    reducer: messagesStateReducer,
    default: () => [],
  }),
});

// 2. Define Tools: Định nghĩa Native Tool
const getWeatherTool = tool(
  async ({ location }) => {
    console.log(`\n[Tool Executing] Cào dữ liệu thời tiết cho: ${location}...`);
    return `Thời tiết ở ${location} là 25 độ C, nắng nhẹ, không mưa.`;
  },
  {
    name: "get_weather",
    description: "Lấy thời tiết hiện tại của một địa điểm",
    schema: z.object({
      location: z.string().describe("Tên thành phố, ví dụ: Hồ Chí Minh, Hà Nội"),
    }),
  }
);
const tools = [getWeatherTool];
const toolNode = new ToolNode(tools);

// 3. Define LLM Model: Kết nối Model với Tools (Bypass XML Parser)
const model = new ChatOpenAI({ 
    modelName: process.env.AI_MODEL || "gpt-4o-mini",
    temperature: 0,
    apiKey: process.env.AI_API_KEY || process.env.OPENAI_API_KEY || "dummy", // Thêm key nếu dùng OAI thật
    configuration: {
        baseURL: process.env.AI_BASE_URL // Có thể map với OpenAI-compatible local server
    }
}).bindTools(tools);

async function callModel(state: typeof StateAnnotation.State) {
  console.log("\n[Node: LLM] Đang suy nghĩ...");
  const response = await model.invoke(state.messages);
  return { messages: [response] };
}

// 4. Routing Logic
function shouldContinue(state: typeof StateAnnotation.State) {
  const messages = state.messages;
  const lastMessage = messages[messages.length - 1];
  // Nếu AI trả về function call -> chuyển sang Tool Node
  if (lastMessage.additional_kwargs?.tool_calls?.length || (lastMessage as any).tool_calls?.length) {
    return "tools";
  }
  return "__end__";
}

// 5. Compile Graph: Kết dính các Node thành Flow
const workflow = new StateGraph(StateAnnotation)
  .addNode("agent", callModel)
  .addNode("tools", toolNode)
  .addEdge("__start__", "agent")
  .addConditionalEdges("agent", shouldContinue)
  .addEdge("tools", "agent"); // Từ Tool quay lại Agent để đọc kết quả

// 6. Persistence: Bộ nhớ Checkpointing
const checkpointer = new MemorySaver();
const app = workflow.compile({ checkpointer });

// =====================================
// POC RUNNER
// =====================================
async function runPOC() {
  console.log("🚀 [LIVA LangGraph POC] Kích hoạt luồng Agent Graph...\n");
  const config = { configurable: { thread_id: "test-session-1" } };
  
  // Turn 1
  console.log("👤 User: Thời tiết ở Hồ Chí Minh hôm nay thế nào?");
  let state = await app.invoke({
    messages: [new HumanMessage("Thời tiết ở Hồ Chí Minh hôm nay thế nào? Trả lời bằng tiếng Việt nhé.")]
  }, config);
  
  let lastMsg = state.messages[state.messages.length - 1];
  console.log(`\n🤖 LIVA: ${lastMsg.content}\n`);
  console.log("-".repeat(50));
  
  // Turn 2 (Testing Memory / Checkpointer)
  console.log("\n👤 User: Thế còn Hà Nội?");
  state = await app.invoke({
    messages: [new HumanMessage("Thế còn Hà Nội?")]
  }, config);
  
  lastMsg = state.messages[state.messages.length - 1];
  console.log(`\n🤖 LIVA: ${lastMsg.content}\n`);
  
  console.log("\n✅ [POC Hoàn Tất] Ưu điểm cốt lõi của LangGraph so với AgentLoop hiện tại:");
  console.log("1. KHÔNG parse XML - Tool được trigger qua Native structured outputs (Function Calling).");
  console.log("2. KHÔNG loop while() tay - StateGraph tự xử lý luồng tuần hoàn (Agent <-> Tool).");
  console.log("3. BỘ NHỚ TỰ ĐỘNG - MemorySaver tự lưu context window theo thread_id.");
}

runPOC().catch(console.error);
