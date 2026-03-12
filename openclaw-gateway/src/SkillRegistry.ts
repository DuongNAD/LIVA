import * as fs from 'fs';

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

    public registerSkill(skill: AgentSkill) {
        this.skills.set(skill.name, skill);
        console.log(`[SkillRegistry] Đã đăng ký kỹ năng: ${skill.name}`);
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
        console.log(`[SkillRegistry] Đang thực thi kỹ năng: ${name} với tham số:`, args);
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
                return date.toISOString();
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
