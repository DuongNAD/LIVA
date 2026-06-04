import { z } from 'zod';

export type SkillCategory = 
  | 'core'      // Lệnh hệ thống, I/O cơ bản, dịch thuật, hệ điều hành
  | 'web'       // Duyệt web, tóm tắt, tải dữ liệu
  | 'devops'    // Chạy code, test mạng, đo đạc hệ thống
  | 'data'      // Xử lý ảnh, mã QR, định dạng dữ liệu
  | 'docs'      // PDF, báo cáo văn bản
  | 'personal'  // Sao lưu, chi tiêu, cá nhân hóa
  | 'social'    // Tin nhắn, email
  | 'agentic';  // Lập kế hoạch, suy luận sâu

export interface SkillMetadata {
    name: string;
    category?: SkillCategory;      // BẮT BUỘC dùng enum này theo chuẩn v19
    short_desc?: string;           // Tối đa 80 ký tự (Dùng cho RAG)
    description: string;
    semantic_tags?: string[];      // Từ khóa vector cho sqlite-vec
    search_keywords?: string[];    // Tương thích ngược keyword search
    requires_hitl?: boolean;       // Cờ bảo mật - Bắt buộc người dùng UI duyệt
    is_cpu_heavy?: boolean;        // Cờ hiệu năng - Cảnh báo khóa Event Loop
    isCoreSkill?: boolean;
    kit?: string;                  // Fallback for legacy dynamic gating
    parameters: any;
}

export interface AgentSkill {
  name: string;
  description: string;
  short_desc?: string;           // Tool Attention: mô tả siêu ngắn cho Filtered Full Schema
  category?: SkillCategory;      // BẮT BUỘC dùng enum này theo chuẩn v19
  semantic_tags?: string[];      // Từ khóa vector cho sqlite-vec
  kit?: import("../memory/SemanticRouter").SkillKit; // [Dynamic Gating]
  parameters: any; 
  search_keywords?: string[];
  isCoreSkill?: boolean;
  requiresApproval?: boolean;
  requires_hitl?: boolean;       // Cờ bảo mật - Bắt buộc người dùng UI duyệt
  is_cpu_heavy?: boolean;        // Cờ hiệu năng - Cảnh báo khóa Event Loop
  execute?: (args: any) => Promise<any>;
}

export interface BaseMetadata {
  name: string;
  description: string;
  version: string;
  category: string;
  strictMode: boolean;
}

export interface Manifest {
  jsonManifest: Record<string, any>;
  compiledAt: string;
}

export class SkillMetadataProcessor {
  private schemaCache: Map<string, z.ZodObject<any>>;
  private metadataRegistry: WeakMap<z.ZodTypeAny, BaseMetadata & Manifest>;

  constructor() {
    this.schemaCache = new Map();
    this.metadataRegistry = new WeakMap();
  }

  public preserveAndValidateSchema<T extends z.ZodRawShape>(
    meta: BaseMetadata,
    shape: T
  ): z.ZodObject<T> {
    const cacheKey = `${meta.name}_v${meta.version}`;
    
    if (this.schemaCache.has(cacheKey)) {
      return this.schemaCache.get(cacheKey) as z.ZodObject<T>;
    }

    const baseZodSchema = z.object(shape).passthrough();

    const preservedSchema = baseZodSchema.superRefine((data, ctx) => {
      if (meta.strictMode) {
        const inputKeys = Object.keys(data);
        const schemaKeys = Object.keys(shape);
        for (const key of inputKeys) {
          if (!schemaKeys.includes(key)) {
            ctx.addIssue({
              code: z.ZodIssueCode.custom,
              message: `Strict Mode Violation: Thuộc tính lạ '${key}' không được định nghĩa trong lược đồ của kĩ năng ${meta.name}.`,
              path: [key]
            });
          }
        }
      }
    });

    const manifestData = {
      ...meta,
      compiledAt: new Date().toISOString(),
      jsonManifest: this.generateJsonManifest(meta, shape)
    };

    // Store in WeakMap registry
    this.metadataRegistry.set(preservedSchema, manifestData);

    // Fallback: Define read-only property on schema instance for backward compatibility
    Object.defineProperty(preservedSchema, 'preservedMeta', {
      value: manifestData,
      writable: false,
      configurable: false,
      enumerable: true
    });

    this.schemaCache.set(cacheKey, preservedSchema as any);
    return preservedSchema as z.ZodObject<T>;
  }

  private generateJsonManifest(meta: BaseMetadata, shape: z.ZodRawShape): Record<string, any> {
    const properties: Record<string, any> = {};
    const required: string[] = [];

    for (const key in shape) {
      const zodType = shape[key] as any;
      let isOptional = false;

      if (typeof zodType.isOptional === 'function' && zodType.isOptional()) {
        isOptional = true;
      }
      
      let innerZodType = zodType;
      if (zodType._def && zodType._def.type === 'optional' && zodType._def.innerType) {
        innerZodType = zodType._def.innerType;
        isOptional = true;
      }

      let typeStr = 'string';
      if (innerZodType._def && typeof innerZodType._def.type === 'string') {
        typeStr = innerZodType._def.type;
      } else if (innerZodType.constructor && innerZodType.constructor.name) {
        typeStr = innerZodType.constructor.name.replace('Zod', '').toLowerCase();
      }

      properties[key] = {
        type: typeStr,
        description: zodType.description || (innerZodType as any).description || `Tham số định danh ${key}`
      };

      if (!isOptional) {
        required.push(key);
      }
    }

    return {
      type: 'function',
      function: {
        name: meta.name,
        description: meta.description,
        parameters: {
          type: 'object',
          properties,
          required
        }
      }
    };
  }

  public getPreservedMeta(schema: any): (BaseMetadata & Manifest) | null {
    if (!schema) return null;
    const registered = this.metadataRegistry.get(schema);
    if (registered) return registered;
    // Fallback for compatibility
    return schema.preservedMeta || null;
  }
}
