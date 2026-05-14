import { z } from "zod";
import { logger } from "@utils/logger";
import { exec } from "node:child_process";
import { HITLGuard } from "@security/HITLGuard";

// Zod Schema cho Docker actions
const DockerSandboxSchema = z.object({
  image: z.string().min(1, "Thiếu tên image Docker"),
  script: z.string().min(1, "Thiếu script cần chạy"),
  timeoutSeconds: z.number().optional().default(30),
});

export const metadata = {
  name: "docker_sandbox_manager",
  description: "[AUTO_RUN] Run standalone code or analyze risky data inside a Docker Container (Zero-Trust Sandbox) with network isolation and memory limits.",
  kit: "DEVOPS_KIT",
  parameters: {
    type: "object",
    properties: {
      image: {
        type: "string",
        description: "Docker image name to run (e.g., 'python:3.10-alpine', 'node:18-alpine').",
      },
      script: {
        type: "string",
        description: "Source code or bash command to execute inside the container.",
      },
      timeoutSeconds: {
        type: "number",
        description: "Maximum timeout in seconds (default 30s).",
      }
    },
    required: ["image", "script"],
  },
};

export const execute = async (argsObj: any): Promise<string> => {
    try {
        const parsed = DockerSandboxSchema.parse(argsObj);
        const { image, script, timeoutSeconds } = parsed;

        // An toàn: Không cho phép override command line flags
        const safeImage = image.replace(/[&|;$><`\\"\n\r]/g, "");
        const encodedScript = Buffer.from(script).toString('base64');
        
        // Command bọc Sandbox khắt khe:
        // --rm: Xóa ngay sau khi xong
        // --network none: Cắt internet
        // --memory="512m": Chống tràn RAM
        // --cpus="1.0": Giới hạn CPU
        // --read-only: Hệ thống file chỉ đọc (trừ /tmp nếu map thêm)
        const dockerCmd = `docker run --rm --network none --memory="512m" --cpus="1.0" --read-only -i ${safeImage} sh -c "echo ${encodedScript} | base64 -d | sh"`;

        logger.info(`[DockerSandbox] Đang gọi Docker với script base64 ẩn... (Timeout: ${timeoutSeconds}s)`);

        // Yêu cầu HITL Guard nếu chạy mã không xác định trong Docker (ngay cả sandbox cũng cần duyệt)
        try {
            await HITLGuard.requestApproval({
                toolName: "docker_sandbox_manager",
                args: { image: safeImage, scriptPreview: script.substring(0, 100) },
                reason: `LIVA muốn chạy một đoạn mã ngoại vi trong Docker Sandbox (${safeImage}). Đoạn mã: ${script.substring(0, 100)}...`
            });
            logger.info(`[DockerSandbox] ✅ HITL Approved`);
        } catch (error: unknown) {
        const errMsg = error instanceof Error ? error.message : String(error);
            logger.warn(`[DockerSandbox] ❌ HITL Bị từ chối: ${errMsg}`);
            return `[DOCKER BLOCKED] Thao tác chạy Sandbox đã bị từ chối bởi người dùng: ${errMsg}`;
        }

        // Chạy Docker với AbortController
        return new Promise((resolve, reject) => {
            const abortController = new AbortController();
            const { signal } = abortController;
            
            const timeoutId = setTimeout(() => {
                abortController.abort();
                reject(new Error(`Quá thời gian thực thi (${timeoutSeconds}s). Container đã bị kill.`));
            }, timeoutSeconds * 1000);

            exec(dockerCmd, { signal, maxBuffer: 1024 * 1024 * 5 }, (error, stdout, stderr) => {
                clearTimeout(timeoutId);
                
                if (error && error.name === 'AbortError') {
                    // Đã xử lý ở setTimeout
                    return;
                }

                let output = stdout.trim();
                let errOut = stderr.trim();

                if (output.length > 3000) {
                    output = output.substring(0, 3000) + "\n... [TRUNCATED]";
                }

                if (errOut.length > 1000) {
                    errOut = errOut.substring(0, 1000) + "\n... [TRUNCATED]";
                }

                if (error) {
                    resolve(`[DOCKER FAILED] Exit Code: ${error.code}\n[STDERR]:\n${errOut}\n[STDOUT]:\n${output}`);
                } else {
                    resolve(`[DOCKER SUCCESS]\n[STDOUT]:\n${output}\n[STDERR]:\n${errOut || "None"}`);
                }
            });
        });

    } catch (error: unknown) {
    const errMsg = error instanceof Error ? error.message : String(error);
        logger.error(`[DockerSandbox] Lỗi: ${errMsg}`);
        if (error instanceof z.ZodError) {
            return `[DOCKER ERROR] Sai định dạng tham số: ${error.issues.map(e => e.message).join(", ")}`;
        }
        return `[DOCKER ERROR] Lệnh thất bại: ${errMsg}`;
    }
};
