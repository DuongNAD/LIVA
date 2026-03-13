import { exec } from 'child_process';
import { promisify } from 'util';

const execAsync = promisify(exec);

export const metadata = {
    name: "execute_command",
    description: "Thực thi một lệnh trên Terminal/Command Prompt của hệ điều hành. Dùng để chạy script, kiểm tra mạng, hoặc khởi chạy các công cụ phân tích.",
    parameters: {
        type: "object",
        properties: {
            command: {
                type: "string",
                description: "Câu lệnh CLI cần thực thi (CLI command to execute)."
            }
        },
        required: ["command"]
    }
};

export const execute = async (args: { command: string }): Promise<string> => {
    try {
        console.log(`[Skill: execute_command] Đang chạy lệnh (Executing): ${args.command}`);
        const { stdout, stderr } = await execAsync(args.command);
        
        if (stderr && stderr.trim() !== '') {
            console.warn(`[Cảnh báo - Warning] Có thông báo từ luồng lỗi (Standard error stream):\n${stderr}`);
        }
        
        return `Kết quả thực thi (Execution output):\n${stdout}`;
    } catch (error: any) {
        return `Thực thi thất bại (Execution failed): ${error.message}\nOutput: ${error.stdout || ''}`;
    }
};