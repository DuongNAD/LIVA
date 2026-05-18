import { safeRename } from '../utils/FileUtils';
import { promises as fsp } from "node:fs";
import * as path from "node:path";

/**
 * @module TurboQuantStore
 * Hệ thống quản lý Tensor đa chiều với khả năng tự chữa lành (Self-Healing) 
 * và cơ chế bảo mật Zero-Trust Orchestration thông qua Non-Forgeable Tokens.
 */

/**
 * @type Branded Type - QuantToken
 * Một token được đúc bởi Authority Layer để xác định quyền truy cập vào các phân đoạn bộ nhớ quantized.
 * Sử dụng TypeScript 5.x Branded Authority để ngăn chặn việc ép kiểu trái phép tại compile-time.
 */
export type QuantToken<T extends string> = T & { __auth_brand: "CoreKernelAuthority" };

/**
 * @type Branded Type - QuantHandle
 * Ngăn chặn việc giả mạo dữ liệu tensor bằng cách gắn nhãn định danh không thể xóa bỏ (Non-Forgeable).
 */
export type QuantHandle<T extends Float32Array> = T & { __brand: "QuantumBrandedTensor" };

/**
 * @interface TemporalMetadata
 * Tích hợp thông tin thời gian và mức độ ưu tiên vào cấu trúc Tensor.
 */
export interface TemporalMetadata {
  timestamp: number;
  priority: number; // 0 (low) to 1 (high)
  ttl: number;      // Time-to-live (ms)
}

/**
 * @interface ECCResidual
 * Cơ chế Error Correction Code (ECC). Lưu trữ phần dư của tensor để hiệu chỉnh trôi số.
 */
export interface ECCResidual {
  correctionVector: QuantHandle<Float32Array>; 
  driftMagnitude: number;     // Độ lớn của sai lệch đã ghi nhận
}

/**
 * @interface SelfHealingTensorEntry
 * Thực thể Tensor đa chiều có khả năng tự chữa lành.
 */
export interface SelfHealingTensorEntry {
  role: string;
  content: string;
  temporal: TemporalMetadata;
  compressedTensor: QuantHandle<Float32Array>; 
  ecc: ECCResidual;          
}

/**
 * @class CoreKernel
 * AUTHORITY LAYER - Đóng vai trò là thực thể duy nhất có quyền cấp phát AuthToken.
 * Sử dụng Private Members (#) để bảo vệ Secret Key và Authorized Roles khỏi sự can thiệp bên ngoài.
 */
export class CoreKernel {
  #secretKey: string;
  #authorizedRoles: Set<string>;

  constructor(roles: string[]) {
    this.#secretKey = Math.random().toString(36).substring(2) + Math.random().toString(36).substring(2); // NOSONAR
    this.#authorizedRoles = new Set(roles);
  }

  /**
   * Đúc một AuthToken mới cho một Role cụ thể.
   * Token này là Non-Forgeable nhờ vào sự kết hợp giữa SecretKey và Branded Type.
   */
  public mintAuthToken<T extends string>(role: string): QuantToken<T> | null {
    if (!this.#authorizedRoles.has(role)) return null;
    // Tạo chuỗi token duy nhất gắn với secret key
    const rawToken = `${role}_${this.#secretKey}`;
    return rawToken as unknown as QuantToken<T>;
  }

  /**
   * Xác thực tính hợp lệ của Token dựa trên cấu trúc nội bộ.
   */
  public validateToken(token: string, requiredRole: string): boolean {
    return token === `${requiredRole}_${this.#secretKey}`;
  }

  /**
   * Tạo một Temporal Integrity Proof dựa trên timestampt và secret key.
   */
  public generateTemporalProof(timestampt: number): string {
    const normalizedTime = Math.floor(timestampt / 1000);
    return `${normalizedTime}_${this.#secretKey}`;
  }

  /**
   * Kiểm tra xem một proof có hợp lệ trong cửa sổ thời gian cho phép hay không (Drift Compensation).
   */
  public verifyTemporalProof(proof: string, currentTimestamp: number): boolean {
    const parts = proof.split("_");
    if (parts.length < 2) return false;
    
    const proofTimeStr = parts[0];
    // Re-join in case secret key contains underscores
    const proofSecret = parts.slice(1).join("_");

    const proofTime = Number.parseInt(proofTimeStr, 10);
    const currentTime = Math.floor(currentTimestamp / 1000);

    // Chấp nhận sai số trong vòng 2 giây (drift compensation) để đảm bảo tính ổn định mạng
    return proofSecret === this.#secretKey && Math.abs(currentTime - proofTime) <= 2;
  }
}

/**
 * @class SelfHealingTensorStore
 * Engine xử lý toán học cho Tensor với cơ chế bảo vệ nội bộ bằng Private Members (#).
 */
export class SelfHealingTensorStore {
  #projectionMatrix: Float32Array[] | null = null;
  #targetDims: number;
  #inputDims: number;
  #authority: CoreKernel;
  
  // Advanced O(1) Caching Layer cho QuantHandle để giảm thiểu overhead tính toán Tensor
  #handleCache: Map<string, { tensor: QuantHandle<Float32Array>; ecc: ECCResidual; timestamp: number }> = new Map();
  #gcInterval: NodeJS.Timeout;

  constructor(authority: CoreKernel, targetDims: number = 256, inputDims: number = 512) {
    this.#authority = authority;
    this.#targetDims = targetDims;
    this.#inputDims = inputDims;
    // Garbage Collection chạy mỗi 60s để dọn dẹp bộ nhớ đệm chống rò rỉ RAM
    this.#gcInterval = setInterval(() => this.#sweepHandleCache(), 60000);
    this.#gcInterval.unref();
  }

  /**
   * Dập tắt vòng lặp ngầm và giải phóng rác ngay lập tức
   */
  public dispose() {
      if (this.#gcInterval) clearInterval(this.#gcInterval);
      this.#handleCache.clear();
  }

  /**
   * Dọn dẹp bộ nhớ đệm (Cache) Tensor sau mỗi 5 phút tĩnh (TTL)
   */
  #sweepHandleCache() {
      const now = Date.now();
      for (const [key, value] of this.#handleCache) {
          if (now - value.timestamp > 300000) { // 5 phút TTL
              this.#handleCache.delete(key);
          }
      }
  }

  #initializeMatrix() {
    if (this.#projectionMatrix) return;
    this.#projectionMatrix = Array.from({ length: this.#targetDims }, () =>
      new Float32Array(Array.from({ length: this.#inputDims }, () => this.#randomGaussian())),
    );
  }

  #randomGaussian(): number {
    let u = 0, v = 0;
    while (u === 0) u = Math.random(); // NOSONAR
    while (v === 0) v = Math.random(); // NOSONAR
    return Math.sqrt(-2.0 * Math.log(u)) * Math.cos(2.0 * Math.PI * v);
  }

  /**
   * Thực hiện chiếu vector và tạo ECC với sự xác thực Zero-Trust.
   */
  public projectAndGenerateECC(
    vector: number[] | Float32Array, 
    token: string, 
    role: string,
    temporalProof: string
  ): { tensor: QuantHandle<Float32Array>; ecc: ECCResidual } {
    // Zero-Trust Validation Layer
    if (!this.#authority.validateToken(token, role)) {
      throw new Error("Zero-Trust Violation: Unauthorized access to Projection Matrix.");
    }

    if (!this.#authority.verifyTemporalProof(temporalProof, Date.now())) {
        throw new Error("Temporal Integrity Violation: Proof expired or invalid.");
    }

    this.#initializeMatrix();
    if (!this.#projectionMatrix) throw new Error("CoreKernel Failure: Matrix not initialized.");

    // Màng lọc Nullish / Corrupt Validation (Chống lỗi Crash Runtime)
    if (!vector || (!Array.isArray(vector) && !(vector instanceof Float32Array))) {
        throw new Error("Self-Healing Block: Invalid Tensor Vector type. Must be pure array or Float32Array.");
    }

    // 🔒 [Audit Fix M-4] Base64 cache key (was: vector.join(",") = O(N) 2KB string per lookup)
    // Base64 uses C++ native Buffer encoding — fast, compact, value-based comparison
    const cacheKey = Buffer.from(new Float32Array(vector).buffer).toString("base64") + "_" + role;
    if (this.#handleCache.has(cacheKey)) {
        return this.#handleCache.get(cacheKey)!;
    }

    // Typed Array Optimization: Vector math using Float32Array for CPU/RAM density
    const projected = new Float32Array(this.#targetDims);
    for (let i = 0; i < this.#targetDims; i++) {
       const row = this.#projectionMatrix[i];
       let sum = 0;
       for (let j = 0; j < this.#inputDims; j++) {
           sum += (row[j] * (vector[j] || 0));
       }
       projected[i] = sum;
    }

    // Branded Type Casting cho Tensor Integrity
    const compressedTensor = new Float32Array(this.#targetDims) as unknown as QuantHandle<Float32Array>;
    const correctionVector = new Float32Array(this.#targetDims) as unknown as QuantHandle<Float32Array>;

    for (let i = 0; i < this.#targetDims; i++) {
        const val = projected[i];
        compressedTensor[i] = val > 0 ? 1 : -1;
        // ECC Residual: Lưu sai lệch thực sự giữa giá trị gốc và giá trị lượng tử hóa (±1)
        // Công thức cũ `val - sign(val)*|val|` luôn = 0. Đúng phải là `val - quantized`.
        correctionVector[i] = val - compressedTensor[i];
    }

    const driftMagnitude = correctionVector.reduce((a, b) => a + Math.abs(b), 0) / this.#targetDims;

    const result = {
      tensor: compressedTensor,
      ecc: { correctionVector, driftMagnitude },
      timestamp: Date.now()
    };
    
    // Lưu vào Cache
    this.#handleCache.set(cacheKey, result);
    // Garbage collection tự động cho Cache dự phòng (Hard-limit 2000 phần tử)
    if (this.#handleCache.size > 2000) {
        const firstKey = this.#handleCache.keys().next().value;
        if (firstKey) this.#handleCache.delete(firstKey);
    }

    return result;
  }

  /**
   * Tính toán độ tương đồng Cosine với cơ chế tự chữa lành (Self-Healing).
   */
  public healedCosineSimilarity(
    q1: QuantHandle<Float32Array>, 
    q2: QuantHandle<Float32Array>, 
    ecc1: ECCResidual, 
    ecc2: ECCResidual
  ): number {
    if (q1.length !== q2.length) return 0;

    // Self-Healing Step: Áp dụng correction vectors trước khi tính toán similarity bằng Mảng Định Tuyến
    const len = q1.length;
    const h1 = new Float32Array(len);
    const h2 = new Float32Array(len);
    for(let i = 0; i < len; i++) {
        h1[i] = q1[i] + ecc1.correctionVector[i];
        h2[i] = q2[i] + ecc2.correctionVector[i];
    }

    let dotProduct = 0;
    let normA = 0;
    let normB = 0;

    for (let i = 0; i < h1.length; i++) {
      dotProduct += h1[i] * h2[i];
      normA += h1[i] * h1[i];
      normB += h2[i] * h2[i];
    }

    if (normA === 0 || normB === 0) return 0;
    return dotProduct / (Math.sqrt(normA) * Math.sqrt(normB));
  }
}

/**
 * @class QuantizedMemoryStore
 * Hệ thống quản lý bộ nhớ Tensor đa chiều, tích hợp Authority Layer và cơ chế Self-Healing Isolation.
 */
export class QuantizedMemoryStore {
  /**
   * O(1) Role-based Indexing: Nâng cấp lưu trữ mảng tuyến tính sang Map<K, V> O(1)
   * Giúp việc searchSimilar bỏ qua hàng triệu bản ghi không cùng Role.
   */
  #entries: Map<string, Map<string, SelfHealingTensorEntry>> = new Map();
  #tensorEngine: SelfHealingTensorStore;
  #authority: CoreKernel;
  #filePath: string;
  #gcInterval: NodeJS.Timeout;

  // LIVA-UHM: Write-Behind Cache Properties
  #isDirty: boolean = false;
  #flushInterval: NodeJS.Timeout;

  constructor(authority: CoreKernel, filePath: string) {
    this.#authority = authority;
    this.#tensorEngine = new SelfHealingTensorStore(authority, 256, 512);
    this.#filePath = filePath;
    this.loadAsync().catch(() => {});
    
    // Background Garbage Collection (Chống rò rỉ RAM) - Chạy mỗi 5 phút
    // Đã tiến hóa để kiểm soát chặt chẽ tránh gây gián đoạn CPU quá mức
    this.#gcInterval = setInterval(() => this.#sweepGarbage(), 300000);
    this.#gcInterval.unref();
    
    // Write-Behind Cache Flush (Xả dữ liệu mỗi 5 giây nếu có thay đổi)
    this.#flushInterval = setInterval(() => this.#flushToDisk(), 5000);
    this.#flushInterval.unref();
  }

  /**
   * Tắt toàn bộ Daemon dọn rác nền để tránh rò rỉ tiến trình Event Loop.
   * Thực hiện async flush cuối cùng để chống Data Loss.
   */
  public async dispose() {
      if (this.#gcInterval) clearInterval(this.#gcInterval);
      if (this.#flushInterval) clearInterval(this.#flushInterval);
      this.#tensorEngine.dispose();
      await this.#forceFlushAsync(); // Async flush chống Data Loss
  }

  /**
   * O(1) Sweep Mechanism: Lọc và dọn dẹp các Entry đã hết hạn TTL
   */
  async #sweepGarbage() {
      const now = Date.now();
      let changed = false;
      for (const [role, roleMap] of this.#entries) {
          const initialSize = roleMap.size;
          for (const [entryId, entry] of roleMap) {
              if (now >= entry.temporal.timestamp + entry.temporal.ttl) {
                  roleMap.delete(entryId);
              }
          }
          if (roleMap.size < initialSize) {
              changed = true;
          }
      }
      if (changed) {
          this.#isDirty = true;
      }
  }

  async #flushToDisk() {
      if (!this.#isDirty) return;
      this.#isDirty = false;
      try {
          await this.save();
      } catch (e) {
          this.#isDirty = true;
      }
  }

  /**
   * [v4.0] Non-blocking final flush — replaces forceFlushSync().
   * Uses fsp for all I/O to avoid blocking the Event Loop during shutdown.
   */
  async #forceFlushAsync() {
      if (!this.#isDirty) return;
      this.#isDirty = false;
      try {
          const dir = path.dirname(this.#filePath);
          await fsp.mkdir(dir, { recursive: true });
          const allEntries = Array.from(this.#entries.values())
              .flatMap(roleMap => Array.from(roleMap.values()))
              .sort((a, b) => a.temporal.timestamp - b.temporal.timestamp);
          const data = allEntries.map((e) => {
             const safeEntry = { ...e };
             safeEntry.compressedTensor = Array.from(e.compressedTensor) as unknown as QuantHandle<Float32Array>;
             safeEntry.ecc = { ...e.ecc, correctionVector: Array.from(e.ecc.correctionVector) as unknown as QuantHandle<Float32Array> };
             return JSON.stringify(safeEntry);
          }).join("\n");
          const tmpPath = `${this.#filePath}.tmp`;
          await fsp.writeFile(tmpPath, data, "utf-8");
          await safeRename(tmpPath, this.#filePath);
      } catch {
          // Ignore errors on shutdown flush
      }
  }

  /**
   * Thêm bộ nhớ mới vào hệ thống với sự xác thực đầy đủ.
   */
  public async addMemory(
    role: string,
    content: string,
    originalEmbedding: number[],
    authToken: string, 
    priority: number = 0.5,
    ttl: number = 86400000 
  ) {
    const now = Date.now();
    const proof = this.#authority.generateTemporalProof(now);

    // Thực hiện Projection với Zero-Trust validation
    const { tensor, ecc } = this.#tensorEngine.projectAndGenerateECC(originalEmbedding, authToken, role, proof);

    const entry: SelfHealingTensorEntry = {
      role,
      content,
      temporal: {
        timestamp: now,
        priority,
        ttl
      },
      compressedTensor: tensor,
      ecc: ecc
    };

    if (!this.#entries.has(role)) {
        this.#entries.set(role, new Map<string, SelfHealingTensorEntry>());
    }
    // Sử dụng timestamp + random để tạo EntryID duy nhất tránh trùng lặp trong cùng 1 role
    const entryId = `${now}_${Math.random().toString(36).substring(2)}`; // NOSONAR
    this.#entries.get(role)!.set(entryId, entry);
    
    // Write-Behind: Không gọi await this.append() trực tiếp, mà đánh dấu dirty
    this.#isDirty = true;
  }

  /**
   * [Audit Fix H-2] Cập nhật vector thật từ Xenova embedding cho entry gần nhất của role.
   * Gọi từ background callback sau khi Xenova tính xong real embedding.
   */
  public updateLastVector(role: string, realEmbedding: number[], authToken: string) {
    const roleMap = this.#entries.get(role);
    if (!roleMap || roleMap.size === 0) return;

    // Lấy entry mới nhất (cuối cùng trong Map)
    let lastKey: string | undefined;
    for (const k of roleMap.keys()) { lastKey = k; }
    if (!lastKey) return;

    const entry = roleMap.get(lastKey)!;
    const now = Date.now();
    const proof = this.#authority.generateTemporalProof(now);

    try {
      const { tensor, ecc } = this.#tensorEngine.projectAndGenerateECC(realEmbedding, authToken, role, proof);
      entry.compressedTensor = tensor;
      entry.ecc = ecc;
      this.#isDirty = true;
    } catch (e) {
      // Nếu token hết hạn hoặc lỗi projection, giữ nguyên dummy vector
    }
  }

  /**
   * Tìm kiếm các entry tương tự với cơ chế Self-Healing Similarity.
   */
  public searchSimilar(
    queryEmbedding: number[],
    role: string,
    authToken: string,
    topK: number = 3,
    minPriority: number = 0
  ): SelfHealingTensorEntry[] {
    const now = Date.now();
    const proof = this.#authority.generateTemporalProof(now);

    // Tạo Query Tensor với cùng các ràng buộc bảo mật
    const { tensor: queryTensor, ecc: queryEcc } = this.#tensorEngine.projectAndGenerateECC(queryEmbedding, authToken, role, proof);

    const roleMap = this.#entries.get(role);
    if (!roleMap) return [];

    // Chuyển đổi Map sang Array để thực hiện filter/map (vẫn đảm bảo logic cũ)
    const roleCandidates = Array.from(roleMap.values());

    const results = roleCandidates
      .filter(entry => entry.temporal.priority >= minPriority)
      .filter(entry => now < entry.temporal.timestamp + entry.temporal.ttl)
      .map((entry) => {
        const score = this.#tensorEngine.healedCosineSimilarity(
          queryTensor,
          entry.compressedTensor,
          queryEcc,
          entry.ecc
        );
        return { entry, score };
      });

    results.sort((a, b) => b.score - a.score);
    return results.slice(0, topK).map((r) => r.entry);
  }

  public static async create(authority: CoreKernel, filePath: string): Promise<QuantizedMemoryStore> {
      const store = new QuantizedMemoryStore(authority, filePath);
      await store.loadAsync();
      return store;
  }

  public async loadAsync() {
    try {
      await fsp.access(this.#filePath);
      const data = await fsp.readFile(this.#filePath, "utf-8");
      this.#entries.clear();
      const parsedEntries = data
        .split("\n")
        .filter((line) => line.trim())
        .map((line) => {
          try {
            const entry = JSON.parse(line);
            if (!entry || typeof entry !== 'object' || !entry.compressedTensor || !entry.ecc) return null; // Corrupt Data Guard
            // Re-hydrate TypedArrays từ JSON thuần tuý (vì lúc lưu đã bị mỏng hóa thành mảng)
            const hydratedTensor = new Float32Array(Object.values(entry.compressedTensor));
            const hydratedCorrection = new Float32Array(Object.values(entry.ecc.correctionVector));
            entry.compressedTensor = hydratedTensor as unknown as QuantHandle<Float32Array>;
            entry.ecc.correctionVector = hydratedCorrection as unknown as QuantHandle<Float32Array>;
            return entry as SelfHealingTensorEntry;
          } catch {
            return null;
          }
        })
        .filter((e): e is SelfHealingTensorEntry => e !== null);
      
      for (const e of parsedEntries) {
          if (!this.#entries.has(e.role)) {
              this.#entries.set(e.role, new Map<string, SelfHealingTensorEntry>());
          }
          // Khi load từ file phẳng, ta tạo ID dựa trên timestamp để tái cấu trúc Map
          const entryId = `${e.temporal.timestamp}_${Math.random().toString(36).substring(2)}`; // NOSONAR
          this.#entries.get(e.role)!.set(entryId, e);
      }
    } catch {
        // File doesn't exist or unreadable, ignore
    }
  }

  public async save() {
    const dir = path.dirname(this.#filePath);
    // [v4.0] Non-blocking: fsp.mkdir with recursive handles existence check internally
    await fsp.mkdir(dir, { recursive: true });
    const allEntries = Array.from(this.#entries.values())
        .flatMap(roleMap => Array.from(roleMap.values()))
        .sort((a, b) => a.temporal.timestamp - b.temporal.timestamp);
    const data = allEntries.map((e) => {
       // Ép kiểu Float32Array về dạng mảng thường trước khi stringify để giữ cấu trúc mảng thuần
       const safeEntry = { ...e };
       safeEntry.compressedTensor = Array.from(e.compressedTensor) as unknown as QuantHandle<Float32Array>;
       safeEntry.ecc = { ...e.ecc, correctionVector: Array.from(e.ecc.correctionVector) as unknown as QuantHandle<Float32Array> };
       return JSON.stringify(safeEntry);
    }).join("\n");
    // 🔒 [Audit Fix M-2] Atomic Write: .tmp + rename to prevent corrupt file
    const tmpPath = `${this.#filePath}.tmp`;
    await fsp.writeFile(tmpPath, data, "utf-8");
    await safeRename(tmpPath, this.#filePath);
  }
}