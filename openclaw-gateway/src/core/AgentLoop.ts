import OpenAI from "openai";
import { MemoryManager } from "../MemoryManager";
import { SkillRegistry } from "../SkillRegistry";
import { logger } from "../utils/logger";
import { SensoryManager } from "../memory/SensoryManager";
import { ModelOrchestrator } from "./ModelOrchestrator";
import { PromptBuilder } from "./PromptBuilder";

export enum TaskLane {
  UI_INTERACTION = "ui_interaction",
  LLM_REASONING = "llm_reasoning",
  BACKGROUND_JOB = "background_job",
}

export interface MessageTask {
  id: string;
  lane: TaskLane;
  data: any;
  execute: () => Promise<void>;
}

export class AgentLoop {
  private orchestrator: ModelOrchestrator;
  private aiRouterClient: OpenAI;
  private aiExpertClient: OpenAI;
  private memory: MemoryManager;
  private registry: SkillRegistry;

  public onThinkingStart?: () => void;
  public onThinkingEnd?: () => void;
  public onStreamStart?: () => void;
  public onStreamChunk?: (chunk: string) => void;
  public onSpokenResponse?: (text: string) => void;

  private lanes: Map<TaskLane, MessageTask[]> = new Map();
  private activeLanes: Set<TaskLane> = new Set();
  public currentSystemLocation = "Vị trí không xác định";

  constructor(memory: MemoryManager, registry: SkillRegistry) {
    this.memory = memory;
    this.registry = registry;
    this.orchestrator = new ModelOrchestrator();

    // Client trỏ tới bản thể Router (trực chiến RAM ngầm, cổng 8000)
    this.aiRouterClient = new OpenAI({
      baseURL: "http://127.0.0.1:8000/v1",
      apiKey: "local-ghost-router", 
    });

    // Client trỏ tới bản thể Expert (khi gọi mới tải lên VRAM, cổng 8001)
    this.aiExpertClient = new OpenAI({
      baseURL: "http://127.0.0.1:8001/v1",
      apiKey: "local-ghost-expert", 
    });

    Object.values(TaskLane).forEach((lane) => {
      this.lanes.set(lane, []);
    });
    logger.info("💻 [System] Kiến trúc Orchestrator Mới (Dual-Port) đã nạp cốt lõi.");
  }

  public async initModels() {
    try {
      await this.orchestrator.startRouter();
    } catch (e: any) {
      logger.error("Lỗi khi mồi Router Server:", e.message);
    }
  }

  public setSystemLocation(loc: string) {
    this.currentSystemLocation = loc;
  }

  public dispatch(task: MessageTask): void {
    const queue = this.lanes.get(task.lane);
    if (queue) {
      queue.push(task);
      this.processLane(task.lane);
    }
  }

  private async processLane(lane: TaskLane): Promise<void> {
    if (this.activeLanes.has(lane)) return;

    this.activeLanes.add(lane);
    const queue = this.lanes.get(lane);

    while (queue && queue.length > 0) {
      const task = queue.shift();
      if (task) {
        try {
          await task.execute();
        } catch (error) {
          logger.error(`[AgentLoop] Lỗi tại [${task.id}]:`, error);
        }
      }
    }
    this.activeLanes.delete(lane);
  }

  public handleUserInput(userText: string) {
    this.dispatch({
      id: `voice-cmd-${Date.now()}`,
      lane: TaskLane.LLM_REASONING,
      data: { text: userText },
      execute: async () => {
        if (this.onThinkingStart) this.onThinkingStart();

        logger.info(`Đang Load Ngữ Cảnh...`);

        // Báo cáo Zalo Mid-flight khi bắt đầu nhận Job
        if (userText.includes("[Tin nhắn từ Zalo điện thoại]")) {
          try {
            await this.registry.executeSkill("send_zalo_bot", {
               message: "⚡ Dạ thưa sếp, LIVA đã tiếp nhận yêu cầu và đang đánh giá. Dự kiến mất 10-15s nếu là tìm kiếm mạng nhẹ, hoặc 1-2 phút nếu cần chuyển giao não chuyên gia. Xin sếp ráng nán lại chờ nha!"
            });
          } catch(e) {}
        }

        try {
          const toolsDef = this.registry.getAllSkills().map((skill) => ({
            name: skill.name,
            description: skill.description,
            parameters: skill.parameters,
          }));

          const aiMessages = await PromptBuilder.prepareFullAiMessages(
              userText,
              this.memory,
              this.currentSystemLocation,
              toolsDef
          );

          let isFinished = false;
          let turnCount = 0;
          let finalReply = "";
          let isExpertAwake = false;
          const allExecutedTools: string[] = [];
          let currentQuery = userText;

          // Streaming Helper function
          const generateText = async (
            msgs: any[],
            newQuery: string,
            useExpert: boolean = false,
            maxTokens: number = 2500,
          ) => {
            const localMsgs = [...msgs, { role: "user", content: newQuery }];
            
            // Quyết định dùng Router hay Expert
            const client = useExpert ? this.aiExpertClient : this.aiRouterClient;
            const usingTarget = useExpert ? "local-ghost-expert" : "local-ghost-router";

            const stream = await client.chat.completions.create({
              model: usingTarget,
              messages: localMsgs,
              temperature: 0.3,
              max_tokens: maxTokens,
              stream: true,
            });

            let fullContent = "";
            let buffer = "";
            let isToolCallMode = false;
            let passedBufferCheck = false;

            for await (const chunk of stream) {
              const token = chunk.choices[0]?.delta?.content || "";
              fullContent += token;

              if (!passedBufferCheck) {
                buffer += token;
                if (buffer.length >= 15 || chunk.choices[0]?.finish_reason) {
                  passedBufferCheck = true;
                  
                  // FIXED CPU Intensive Parse: Check on last ~30 chars instead of full buffer
                  const recentTail = buffer.slice(-30);
                  if (
                    recentTail.includes("<to") ||
                    buffer.includes('{"name":') ||
                    buffer.trim().startsWith("{")
                  ) {
                    isToolCallMode = true;
                    logger.info("[Stream Mute] 🤫 LIVA đang nhẩm tính lệnh Kỹ năng ngầm...");
                  } else {
                    if (this.onStreamStart) this.onStreamStart();
                    if (this.onStreamChunk) this.onStreamChunk(buffer);
                  }
                }
              } else {
                if (!isToolCallMode) {
                  if (this.onStreamChunk) this.onStreamChunk(token);
                }
              }
            }
            return fullContent;
          };

          while (!isFinished && turnCount < 4) {
            turnCount++;
            logger.info(`Đang đập cánh luồng Tư Duy bằng [${isExpertAwake ? "Expert Model 26B" : "Router Model 4B"}] (Vòng #${turnCount})...`);

            const responseRawText = await generateText(
              aiMessages,
              currentQuery,
              isExpertAwake
            );
            logger.debug(`RAW AI Response (Turn ${turnCount}):`, responseRawText);

            let contentText = responseRawText || "";
            let parsedToolCalls: any[] = [];

            // XML Tool Parser
            if (contentText.includes("<tool_call>")) {
              try {
                const regex = /<tool_call>([\s\S]*?)<\/tool_call>/g;
                const matches = [...contentText.matchAll(regex)];
                if (matches && matches.length > 0) {
                  for (const match of matches) {
                    if (match[1]) {
                      const toolJson = JSON.parse(match[1].trim());
                      parsedToolCalls.push(toolJson);
                    }
                  }
                  contentText = contentText.replace(regex, "").trim();
                }
              } catch (e) {
                logger.error("Lỗi Regex Parse Multi-Tool:", e);
              }
            } else if (contentText.includes('{"name":') && contentText.includes("}")) {
              // JSON Fallback
              try {
                const match = contentText.match(/(\{(?:[^{}]|(?!<)\{(?:[^{}]|(?!<)\{.*?\})*?\})*\})/);
                if (match) {
                  const toolJson = JSON.parse(match[1].trim());
                  if (toolJson.name) parsedToolCalls = [toolJson];
                  contentText = contentText.replace(match[1], "").trim();
                }
              } catch (e) {}
            }

            if (parsedToolCalls.length > 0) {
              logger.info(`AI gọi ${parsedToolCalls.length} kỹ năng trong Turn ${turnCount}:`, parsedToolCalls);
              let finalToolResults = "";

              aiMessages.push({ role: "user", content: currentQuery });
              aiMessages.push({ role: "assistant", content: responseRawText });

              for (const toolCall of parsedToolCalls) {
                const functionName = toolCall.name;

                // Logic Cascade Handoff KHÔNG ĐỘT TỬ
                if (functionName === "handoff_to_expert") {
                  logger.warn(`🚀 [Handoff] Router gọi cứu viện. Đang ép 26B lên VRAM GPU (Router nghỉ ngơi giữ chỗ)...`);
                  if (userText.includes("[Tin nhắn từ Zalo điện thoại]")) {
                    try {
                      await this.registry.executeSkill("send_zalo_bot", {
                         message: "🔥 LIVA: Tác vụ này khá căng nên em đang đẩy não Chuyên Gia 26B lên VRAM! Không cần reload toàn bộ hệ thống nữa nên chỉ chờ khoảng 5s..."
                      });
                    } catch(e) {}
                  }
                  
                  try {
                    await this.orchestrator.startExpert();
                    isExpertAwake = true;
                    // Handoff Success => Bơm ngay bối cảnh lại cho lượt tiếp theo nó chạy bằng Client 8001
                    finalToolResults += `[Hệ thống]: Handoff Zero-Overhead Thành Công sang Expert Model (Cổng 8001 VRAM). Các tham số trước đó đã tự động được bê sang. Hãy phục vụ user ngay nhé.\n\n`;
                  } catch (ex: any) {
                    logger.error(`[AgentLoop] Lỗi tải VRAM Expert: ${ex.message}`);
                    finalToolResults += `[Hệ thống Lỗi]: Handoff thất bại! Có thể do VRAM bị tràn cứng. Đã chuyển lại cho Router Model xử lý cục bộ...\n\n`;
                    isExpertAwake = false; // Rơi về dùng Router
                  }
                  continue;
                }

                allExecutedTools.push(functionName);
                
                let functionArgs: any = null;
                try {
                  let argsStr = toolCall.arguments;
                  if (typeof argsStr === "string") {
                    argsStr = argsStr.replace(/\n/g, "\\n").replace(/\r/g, "\\r").replace(/\t/g, "\\t");
                    functionArgs = JSON.parse(argsStr);
                  } else {
                    functionArgs = argsStr;
                  }
                } catch (e) {
                   logger.error(`Lỗi Parse JSON Argument định dạng hỏng kỹ năng ${functionName}`);
                }

                // FIXED Silent Crash if arguments JSON parse is invalid and breaks skill registry 
                if (functionArgs === null) {
                    logger.warn(`Bỏ qua Kỹ năng ${functionName} do LLM trả sai cấu trúc Arguments.`);
                    finalToolResults += `[Hệ thống]: Không thể chạy ${functionName} vì Argument JSON bị định dạng sai. Vui lòng thử lại với khối Argument chuẩn.\n\n`;
                    continue;
                }

                logger.info(`Đang chạy hàm: ${functionName}`, functionArgs);
                const result = await this.registry.executeSkill(
                  functionName,
                  functionArgs,
                );
                logger.info(`Kết quả chạy hàm ${functionName}:`, result);

                let resultStr = typeof result === "string" ? result : JSON.stringify(result);

                const CHUNK_SIZE = 6000;
                if (resultStr.length > CHUNK_SIZE) {
                  logger.warn(`Cắt bớt dữ liệu đuôi (${resultStr.length} chars) bảo vệ VRAM.`);
                  resultStr = resultStr.substring(0, CHUNK_SIZE) + "\n\n[Hệ thống: Dữ liệu bị cắt bớt]";
                }
                finalToolResults += `[Hệ thống trả kết quả từ ${functionName}]:\n${resultStr}\n\n`;
              }

              let nextActionPrompt = `[DỮ LIỆU TỪ CÔNG CỤ VỪA CHẠY]:\n${finalToolResults}`;
              const executedTools = parsedToolCalls.map((t) => t.name).join(", ");

              if (!executedTools.includes("zalo") && turnCount < 4 && userText.toLowerCase().includes("zalo")) {
                 nextActionPrompt += `\n[Gợi ý]: Hãy gọi \`send_zalo_bot\` để gửi Zalo cho Sếp.`;
              } else {
                 nextActionPrompt += `\n[Hệ thống]: Dữ liệu đã ráp nối. Hoàn thiện báo cáo nhé.`;
              }
              currentQuery = nextActionPrompt;
            } else {
              aiMessages.push({ role: "user", content: currentQuery });
              aiMessages.push({ role: "assistant", content: responseRawText });

              isFinished = true;
              finalReply = contentText || "Xin lỗi Anh, em chưa rõ ý này ạ.";
              logger.info(`Liva phản hồi cuối (Final Response): "${finalReply}"`);
            }
          }

          // Xả tải (Offload Expert) khỏi VRAM để trả lại không gian mát mẻ. Router vẫn nằm im trên RAM
          if (isExpertAwake) {
             logger.info("Hoàn tất tác vụ siêu nặng hột. Đang Clear 16GB VRAM của Expert Model...");
             await this.orchestrator.stopExpert();
          }

          await this.memory.addMessage("user", userText);
          await this.memory.addMessage("assistant", finalReply);

          SensoryManager.getInstance().flush();

          if (this.onThinkingEnd) this.onThinkingEnd();
          if (this.onSpokenResponse) this.onSpokenResponse(finalReply);
        } catch (error: any) {
          logger.error("Lỗi kết nối Ghost Server:", error.message);
          if (this.onThinkingEnd) this.onThinkingEnd();
          if (this.onSpokenResponse) {
             this.onSpokenResponse(`❌ Văng Native AI: ${error.message}`);
          }
        }
      },
    });
  }
}
