import path from "path";
import { spawn, ChildProcess } from "child_process";
import treeKill from "tree-kill";
import axios from "axios";
import OpenAI from "openai";
import { MemoryManager } from "../MemoryManager";
import { SkillRegistry } from "../SkillRegistry";
import { logger } from "../utils/logger";
import { BASE_SYSTEM_PROMPT } from "../system_prompt";
import { SensoryManager } from "../memory/SensoryManager";

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

// Lớp điều phối vòng đời của tiến trình Llama.cpp rời rạc (Ghost Server)
class EngineOrchestrator {
  private currentProcess: ChildProcess | null = null;

  constructor() {
    const cleanup = () => {
      if (this.currentProcess?.pid) {
        treeKill(this.currentProcess.pid, "SIGKILL");
      }
    };
    process.on("exit", cleanup);
    process.on("SIGINT", () => {
      cleanup();
      process.exit(0);
    });
    process.on("SIGTERM", () => {
      cleanup();
      process.exit(0);
    });
  }

  public async startServer(
    modelName: string,
    port: number = 8000,
  ): Promise<void> {
    if (this.currentProcess) {
      await this.stopServer();
    }

    return new Promise((resolve, reject) => {
      const modelsDir = process.env.AI_MODELS_DIR || "E:\\AI_Models";
      const exePath = path.join(modelsDir, "llama_bin", "llama-server.exe");
      const modelPath = path.join(modelsDir, modelName);

      logger.info(
        `🔥 [Auto-Spawn] Đang đánh thức trùm cuối Llama Server với model: ${modelName}`,
      );
      const args = [
        "-m",
        modelPath,
        "--port",
        port.toString(),
        "-c",
        "8192",
        "-ngl",
        "99",
      ];

      this.currentProcess = spawn(exePath, args, { stdio: "ignore" });

      let isReady = false;
      // Liên tục ping qua cổng mạng ảo (hàng chờ 0.5s) để xem server đã ngốn xong VRAM chưa
      const healthCheckInterval = setInterval(async () => {
        try {
          const res = await axios.get(`http://127.0.0.1:${port}/v1/models`, {
            timeout: 1000,
          });
          if (res.status === 200) {
            clearInterval(healthCheckInterval);
            clearTimeout(timeoutTimer);
            isReady = true;
            logger.info("✅ Máy chủ Llama Native Engine đã hoạt động ổn định!");
            resolve();
          }
        } catch (e) {}
      }, 500);

      const timeoutTimer = setTimeout(() => {
        if (!isReady) {
          clearInterval(healthCheckInterval);
          if (this.currentProcess && this.currentProcess.pid) {
             treeKill(this.currentProcess.pid, "SIGKILL");
             this.currentProcess = null;
          }
          reject(
            new Error(
              "Timeout (180s) khi khởi động Llama Server! Có thể thiếu RAM hoặc lỗi file GGUF.",
            ),
          );
        }
      }, 180000);

      this.currentProcess.on('exit', (code) => {
        if (!isReady) {
          clearInterval(healthCheckInterval);
          clearTimeout(timeoutTimer);
          this.currentProcess = null;
          reject(new Error(`Llama Server bị crash đột ngột với mã lỗi ${code} trước khi sẵn sàng.`));
        }
      });
    });
  }

  public async stopServer(): Promise<void> {
    return new Promise((resolve) => {
      if (this.currentProcess && this.currentProcess.pid) {
        logger.info(
          "🔪 Đang giật điện tiêu diệt Server cũ, giải phóng VRAM trả cho hệ thống...",
        );
        treeKill(this.currentProcess.pid, "SIGKILL", (err) => {
          this.currentProcess = null;
          logger.info("♻️ Đã xả VRAM hoàn tất!");
          setTimeout(() => resolve(), 1000); // Đợi 1 giây cho VRAM thực sự Clear
        });
      } else {
        resolve();
      }
    });
  }
}

export class AgentLoop {
  private orchestrator: EngineOrchestrator;
  private aiClient: OpenAI;
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
    this.orchestrator = new EngineOrchestrator();

    // Trỏ Client ngược về Ghost Server
    this.aiClient = new OpenAI({
      baseURL: "http://127.0.0.1:8000/v1",
      apiKey: "local-ghost", // Không cần key cho local
    });

    Object.values(TaskLane).forEach((lane) => {
      this.lanes.set(lane, []);
    });
    logger.info(
      "💻 [System] Kiến trúc Ghost Orchestrator (Auto-Spawn) đã kích hoạt.",
    );
  }

  public async initModels() {
    // Nạp sẵn con Router lúc vừa khởi động
    const routerName =
      process.env.ROUTER_MODEL_NAME || "gemma-4-E4B-it-Q4_K_M.gguf";
    try {
      await this.orchestrator.startServer(routerName);
    } catch (e: any) {
      logger.error("Lỗi khi mồi Ghost Server:", e.message);
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

        const userProfile = await this.memory.getUserProfile();
        if (userProfile) {
          userProfile.current_location = this.currentSystemLocation;
        }

        const sensoryPrompt =
          SensoryManager.getInstance().injectSensoryPrompt();
        const profileContext = userProfile
          ? `\n\nTHÔNG TIN NGƯỜI DÙNG HIỆN TẠI (User Profile):\n${JSON.stringify(userProfile, null, 2)}\n(Hãy sử dụng Tên, Khách xưng hô và Vị trí này để phục vụ người dùng)`
          : "";

        const finalContext = profileContext + sensoryPrompt;

        logger.info(`Đang suy luận (Inference Native) cùng ngữ cảnh...`);

        // Báo cáo Zalo Mid-flight khi bắt đầu nhận Job
        if (userText.includes("[Tin nhắn từ Zalo điện thoại]")) {
          try {
            await this.registry.executeSkill("send_zalo_bot", {
               message: "⚡ Dạ thưa sếp, LIVA đã tiếp nhận yêu cầu và đang đánh giá. Dự kiến mất 10-15s nếu là tìm kiếm mạng nhẹ, hoặc 1 phút nếu cần bóc file. Xin sếp chờ chút nha!"
            });
          } catch(e) {}
        }

        try {
          const shortTermHistory = await this.memory.getHybridContext(
            userText,
            6,
          );

          const toolsDef = this.registry.getAllSkills().map((skill) => ({
            name: skill.name,
            description: skill.description,
            parameters: skill.parameters,
          }));

          toolsDef.push({
            name: "handoff_to_expert",
            description:
              "Kích hoạt AI Chuyên Gia (26B) để giải quyết nhiệm vụ phức tạp, đọc phân tích văn bản dài hoặc lập trình. HÃY dùng lệnh này nếu người dùng yêu cầu task nặng/khó.",
            parameters: {
              type: "object",
              properties: {
                reason: {
                  type: "string",
                  description: "Lý do cần chuyển giao",
                },
              },
              required: ["reason"],
            },
          });

          // Fix KV-Cache Busting: Chỉ lấy Giờ:Phút để Hệ thống Llama.cpp cache lại được 4k tokens thay vì tính lại từ đầu do Lệch Giây
          const nowStr = new Date().toLocaleString("vi-VN", {
            timeZone: "Asia/Ho_Chi_Minh",
            dateStyle: "short",
            timeStyle: "short",
          });
          const customToolPrompt = `# Tools\n\nYou may call one or more functions to assist with the user query.\n\nYou are provided with function signatures within <tools></tools> XML tags:\n<tools>\n${JSON.stringify(toolsDef, null, 2)}\n</tools>\n\nFor each function call, return a json object with function name and arguments within <tool_call></tool_call> XML tags:\n<tool_call>\n{"name": <function-name>, "arguments": <args-json-object>}\n</tool_call>\n\nHƯỚNG DẪN THÊM:\n- HÃY GỌI MỘT TOOL NGAY NẾU BẠN CẦN LÀM NHIỆM VỤ THAY VÌ LUYÊN THUYÊN.\n- NẾU NHIỆM VỤ QUÁ LỚN: Sử dụng ngay 'handoff_to_expert'.\n- ĐẶT CÂU HỎI TRỰC TIẾP: Nếu yêu cầu của người dùng thiếu dữ liệu/file cần thiết, đừng tự bịa chuyện, hãy hỏi ngay người dùng.\n\nNGỮ CẢNH HỆ THỐNG:\n- Thời gian: ${nowStr}`;

          let aiMessages: any[] = [
            {
              role: "system",
              content: `${BASE_SYSTEM_PROMPT}\n\n${customToolPrompt}${finalContext}`,
            },
          ];
          for (const msg of shortTermHistory) {
            aiMessages.push({ role: msg.role, content: msg.content });
          }

          // Utils using OpenAI API hitting the Auto-spawned Ghost Server (Streaming)
          const generateText = async (
            msgs: any[],
            newQuery: string,
            maxTokens: number = 2500,
          ) => {
            const localMsgs = [...msgs, { role: "user", content: newQuery }];
            const stream = await this.aiClient.chat.completions.create({
              model: "local-ghost-model",
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
                  if (
                    buffer.includes("<tool") ||
                    buffer.includes('{"name":') ||
                    buffer.trim().startsWith("{")
                  ) {
                    isToolCallMode = true;
                    logger.info(
                      "[Stream Mute] 🤫 LIVA đang nhẩm tính lệnh Kỹ năng ngầm...",
                    );
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

          let isFinished = false;
          let turnCount = 0;
          let finalReply = "";
          let isExpertAwake = false;
          const allExecutedTools: string[] = [];
          let currentQuery = userText;

          while (!isFinished && turnCount < 4) {
            turnCount++;
            logger.info(
              `Đang đập cánh luồng Tư Duy bằng [${isExpertAwake ? "Expert Model 26B" : "Router Model E4B"}] (Vòng #${turnCount})...`,
            );

            const responseRawText = await generateText(
              aiMessages,
              currentQuery,
            );
            logger.debug(
              `RAW AI Response (Turn ${turnCount}):`,
              responseRawText,
            );

            let contentText = responseRawText || "";
            let parsedToolCalls: any[] = [];

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
            } else if (
              contentText.includes('{"name":') &&
              contentText.includes("}")
            ) {
              try {
                const match = contentText.match(
                  /(\{(?:[^{}]|(?!<)\{(?:[^{}]|(?!<)\{.*?\})*?\})*\})/,
                );
                if (match) {
                  const toolJson = JSON.parse(match[1].trim());
                  if (toolJson.name) parsedToolCalls = [toolJson];
                  contentText = contentText.replace(match[1], "").trim();
                }
              } catch (e) {}
            }

            if (parsedToolCalls.length > 0) {
              logger.info(
                `AI gọi ${parsedToolCalls.length} kỹ năng trong Turn ${turnCount}:`,
                parsedToolCalls,
              );
              let finalToolResults = "";

              aiMessages.push({ role: "user", content: currentQuery });
              aiMessages.push({ role: "assistant", content: responseRawText });

              for (const toolCall of parsedToolCalls) {
                const functionName = toolCall.name;

                // Logic Cascade Handoff
                if (functionName === "handoff_to_expert") {
                  logger.warn(
                    `🚀 [Cascade Routing] Router đã nhường quyền. Đang đánh thức mô hình Chuyên gia dày đặc...`,
                  );
                  // Báo cáo Zalo Mid-flight khi bật Chuyên gia
                  if (userText.includes("[Tin nhắn từ Zalo điện thoại]")) {
                    try {
                      await this.registry.executeSkill("send_zalo_bot", {
                         message: "🔥 LIVA: Yêu cầu này cần tư duy sâu và viết code nên em đang Gọi Não Chuyên Gia 26B thức dậy! Xin anh kiên nhẫn một chút đi pha ly cafe nhé, tiến trình tải não sấp xỉ 10 giây..."
                      });
                    } catch(e) {}
                  }
                  try {
                    const expertName =
                      process.env.EXPERT_MODEL_NAME ||
                      "gemma-4-26B-A4B-it-UD-Q4_K_M.gguf";
                    await this.orchestrator.startServer(expertName);
                    isExpertAwake = true;
                    finalToolResults += `[Hệ thống]: Đã chuyển giao ngữ cảnh thành công cho Mô hình Chuyên Gia (Expert 26B). Bạn hiện đang nắm quyền điều khiển. Dữ liệu gốc ở trên. Không cần báo cáo quy trình chuyển giao, hãy làm luôn tác vụ nhé.\n\n`;
                  } catch (ex: any) {
                    logger.error(`[AgentLoop] Lỗi load Expert: ${ex.message}`);
                    finalToolResults += `[Hệ thống Lỗi]: Handoff thất bại do không tải được Model Expert! Đang nạp lại Router Agent để tự xử lý tác vụ...\n\n`;
                    try {
                      const routerName =
                        process.env.ROUTER_MODEL_NAME || "gemma-4-E4B-it-Q4_K_M.gguf";
                      await this.orchestrator.startServer(routerName);
                    } catch (routerErr) {
                      logger.error("Không thể khôi phục Router Model!");
                    }
                  }
                  continue;
                }

                allExecutedTools.push(functionName);
                let functionArgs = {};
                try {
                  let argsStr = toolCall.arguments;
                  if (typeof argsStr === "string") {
                    argsStr = argsStr
                      .replace(/\n/g, "\\n")
                      .replace(/\r/g, "\\r")
                      .replace(/\t/g, "\\t");
                    functionArgs = JSON.parse(argsStr);
                  } else {
                    functionArgs = argsStr;
                  }
                } catch (e) {
                  logger.error(
                    `Lỗi Parse JSON Argument của kỹ năng ${functionName}:`,
                    e,
                  );
                }

                logger.info(`Đang chạy hàm: ${functionName}`, functionArgs);
                const result = await this.registry.executeSkill(
                  functionName,
                  functionArgs,
                );
                logger.info(`Kết quả chạy hàm ${functionName}:`, result);

                let resultStr =
                  typeof result === "string" ? result : JSON.stringify(result);

                const CHUNK_SIZE = 6000;
                if (resultStr.length > CHUNK_SIZE) {
                  logger.warn(
                    `Dữ liệu đầu ra từ công cụ quá lớn (${resultStr.length} ký tự). Tiến hành cắt bớt phần đuôi để bảo vệ Context Size...`,
                  );
                  resultStr =
                    resultStr.substring(0, CHUNK_SIZE) +
                    "\n\n[Hệ thống: Dữ liệu quá dài, bị cắt bớt phần đuôi để tăng tốc]";
                }
                finalToolResults += `[Hệ thống trả kết quả từ ${functionName}]:\n${resultStr}\n\n`;
              }

              let nextActionPrompt = `[DỮ LIỆU TỪ CÔNG CỤ VỪA CHẠY]:\n${finalToolResults}`;
              const executedTools = parsedToolCalls
                .map((t) => t.name)
                .join(", ");

              if (
                !executedTools.includes("zalo") &&
                turnCount < 4 &&
                userText.toLowerCase().includes("zalo")
              ) {
                nextActionPrompt += `\n[Gợi ý tự động]: Bạn vừa chạy xong công cụ [${executedTools}]. Kết quả đã có ở trên. Anh Dương có nhờ gửi qua Zalo, hãy gọi \`send_zalo_bot\` để gửi đi nhé.`;
              } else {
                nextActionPrompt += `\n[Hệ thống]: Dữ liệu công cụ đã có. Bạn hãy tổng hợp lại và trả lời người dùng.`;
              }

              currentQuery = nextActionPrompt;
            } else {
              aiMessages.push({ role: "user", content: currentQuery });
              aiMessages.push({ role: "assistant", content: responseRawText });

              const userRequestedZalo = userText.toLowerCase().includes("zalo");
              const isZaloExecuted = allExecutedTools.includes("send_zalo_bot");
              if (userRequestedZalo && !isZaloExecuted && turnCount < 4) {
                logger.warn(
                  `[Auto-Correction] Mất track send_zalo_bot. Nhắc nhở ở Turn ${turnCount + 1}`,
                );
                currentQuery = `[Nhắc nhở nhẹ]: Liva ơi, hãy gọi thẻ <tool_call> sử dụng \`send_zalo_bot\` để gửi thông tin qua Zalo nhé.`;
                continue;
              }

              isFinished = true;
              finalReply = contentText || "Xin lỗi Anh, em chưa rõ ý này ạ.";
              logger.info(
                `Liva phản hồi cuối (AI Final Response): "${finalReply}"`,
              );
            }
          }

          // Tối ưu VRAM: Gỡ bỏ Expert Model sau khi xong việc, kéo Router Model trở lại vùng an toàn
          if (isExpertAwake) {
            logger.info(
              "Hoàn tất tác vụ nặng. Đang thu dọn đội Chuyên Gia và kéo Router về làm cảnh vệ...",
            );
            const routerName =
              process.env.ROUTER_MODEL_NAME || "gemma-4-E4B-it-Q4_K_M.gguf";
            await this.orchestrator.startServer(routerName);
          }

          await this.memory.addMessage("user", userText);
          await this.memory.addMessage("assistant", finalReply);

          SensoryManager.getInstance().flush();

          if (this.onThinkingEnd) this.onThinkingEnd();
          if (this.onSpokenResponse) this.onSpokenResponse(finalReply);
        } catch (error: any) {
          logger.error("Lỗi kết nối Nội bộ Llama Ghost Server:", error.message);
          if (this.onThinkingEnd) this.onThinkingEnd();
          if (this.onSpokenResponse) {
            this.onSpokenResponse(
              "❌ Lỗi: Tiến trình Native AI bị crash (" +
                error.message +
                "). Giật điện mất kết nối!",
            );
          }
        }
      },
    });
  }
}
