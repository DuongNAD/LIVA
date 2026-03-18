import * as fs from 'fs';
import * as path from 'path';
import { logger } from './utils/logger';

export interface AgentSkill {
    name: string;
    description: string;
    parameters: any; // JSON Schema cho tham số
    execute: (args: any) => Promise<any>;
}

export class SkillRegistry {
    private skills: Map<string, AgentSkill> = new Map();

    constructor() {
        this.registerBuiltInSkills();
    }

    public async registerLocalSkills() {
        const skillsDir = path.join(__dirname, 'skills');
        if (!fs.existsSync(skillsDir)) {
            logger.warn(`[SkillRegistry] Thư mục kỹ năng không tồn tại: ${skillsDir}`);
            return;
        }
        
        const files = fs.readdirSync(skillsDir);
        for (const file of files) {
            if (file.endsWith('.ts') || file.endsWith('.js')) {
                const skillPath = path.join(skillsDir, file);
                try {
                    // Try dynamic import or require depending on environment
                    const module = await import(`file://${skillPath.replace(/\\/g, '/')}`);
                    if (module.metadata && module.execute) {
                        this.registerSkill({
                            name: module.metadata.name,
                            description: module.metadata.description,
                            parameters: module.metadata.parameters,
                            execute: module.execute
                        });
                    }
                } catch (error) {
                    // Fallback to require if dynamic import fails (common in TS Node environments)
                    try {
                        const module = require(skillPath);
                        if (module.metadata && module.execute) {
                            this.registerSkill({
                                name: module.metadata.name,
                                description: module.metadata.description,
                                parameters: module.metadata.parameters,
                                execute: module.execute
                            });
                        }
                    } catch (err) {
                        logger.error(`[SkillRegistry] Lỗi tải kỹ năng từ ${file}:`, err);
                    }
                }
            }
        }
        logger.info(`[SkillRegistry] Đã quét và nạp xong các kỹ năng trong thư mục local.`);
    }

    public registerSkill(skill: AgentSkill) {
        this.skills.set(skill.name, skill);
        logger.info(`[SkillRegistry] Đã đăng ký kỹ năng: ${skill.name}`);
    }

    public getSkill(name: string): AgentSkill | undefined {
        return this.skills.get(name);
    }

    public getAllSkills(): AgentSkill[] {
        return Array.from(this.skills.values());
    }

    public async executeSkill(name: string, args: any): Promise<any> {
        const skill = this.skills.get(name);
        if (!skill) {
            throw new Error(`Kỹ năng '${name}' không tồn tại.`);
        }
        logger.info(`[SkillRegistry] Đang thực thi kỹ năng: ${name} với tham số:`, args);
        return await skill.execute(args);
    }

    private registerBuiltInSkills() {
        // Kỹ năng 1: Xem giờ hệ thống
        this.registerSkill({
            name: 'get_current_time',
            description: 'Lấy thời gian hiện tại của hệ thống.',
            parameters: {
                type: 'object',
                properties: {
                    timezone: { type: 'string', description: 'Múi giờ (vd: Asia/Ho_Chi_Minh). Tùy chọn.' }
                }
            },
            execute: async (args: any) => {
                const date = new Date();
                if (args.timezone) {
                     return date.toLocaleString('vi-VN', { timeZone: args.timezone });
                }
                
                // Tự động lấy Múi giờ chuẩn của thiết bị đang chạy (VD: Asia/Ho_Chi_Minh hoặc khu vực khác)
                const localTimeZone = Intl.DateTimeFormat().resolvedOptions().timeZone;
                return date.toLocaleString('vi-VN', { timeZone: localTimeZone });
            }
        });

        // Kỹ năng 2: Đọc nội dung tệp tin
        this.registerSkill({
            name: 'read_file',
            description: 'Đọc nội dung của một tệp tin trên hệ thống (Local).',
            parameters: {
                type: 'object',
                properties: {
                    path: { type: 'string', description: 'Đường dẫn tuyệt đối hoặc tương đối tới tệp tin.' }
                },
                required: ['path']
            },
            execute: async (args: any) => {
                try {
                    const content = fs.readFileSync(args.path, 'utf8');
                    return content;
                } catch (error: any) {
                    return `Lỗi khi đọc tệp: ${(error as Error).message}`;
                }
            }
        });
    }
}
