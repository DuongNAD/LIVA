import * as fs from 'fs/promises';
import * as path from 'path';

export const metadata = {
    name: "write_local_file",
    description: "Tạo một tệp tin mới hoặc ghi đè nội dung vào tệp tin đã có trên máy tính (Create or overwrite a file).",
    parameters: {
        type: "object",
        properties: {
            filePath: {
                type: "string",
                description: "Đường dẫn tuyệt đối hoặc tương đối tới tệp tin cần ghi. Ví dụ: 'logs/report.txt'"
            },
            content: {
                type: "string",
                description: "Nội dung văn bản (Text content) hoặc mã nguồn cần ghi vào tệp."
            }
        },
        required: ["filePath", "content"]
    }
};

export const execute = async (args: { filePath: string; content: string }): Promise<string> => {
    try {
        const targetPath = path.resolve(process.cwd(), args.filePath);
        console.log(`[Skill: write_local_file] Đang ghi dữ liệu (Writing data) vào: ${targetPath}`);
        
        // Lấy thư mục chứa tệp để đảm bảo nó tồn tại
        const dirName = path.dirname(targetPath);
        await fs.mkdir(dirName, { recursive: true });
        
        await fs.writeFile(targetPath, args.content, 'utf-8');
        return `Đã ghi tệp thành công (File written successfully) tại: ${targetPath}`;
    } catch (error: any) {
        return `Lỗi khi ghi tệp (File write error): ${error.message}`;
    }
};