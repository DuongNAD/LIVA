import * as fs from 'node:fs';
import * as readline from 'node:readline';
import path from "node:path";
import { logger } from "@utils/logger";

export const metadata = {
  name: "analyze_structured_data",
  search_keywords: ["CSV", "phân tích dữ liệu", "data analysis", "thống kê", "cột", "dòng", "TXT", "dữ liệu có cấu trúc"],
  description: "[AUTO_RUN] Analyze large data files (CSV/TXT) via Stream (Zero-Blocking) to return structural stats (rows, columns, null counts, top 5 rows) without filling RAM/VRAM.",
  kit: "DATA_KIT",
  parameters: {
    type: "object",
    properties: {
      filePath: {
        type: "string",
        description: "Path to CSV file (e.g., 'Path2/Nop_Bai/Hien/8_Customer_Tier_Data.csv').",
      },
      delimiter: {
        type: "string",
        description: "Delimiter character (default: ',').",
      }
    },
    required: ["filePath"],
  },
};

export const execute = async (args: { filePath: string; delimiter?: string }): Promise<string> => {
    const { filePath, delimiter = ',' } = args;
    const targetPath = path.resolve(process.cwd(), filePath);
    
    logger.info(`[StructuredDataAnalyzer] Bắt đầu stream file (Zero-Blocking): ${targetPath}`);

    return new Promise((resolve, reject) => {
        let totalRows = 0;
        const headRows: string[] = [];
        let headers: string[] = [];
        const nullCounts: Record<string, number> = {};

        // Numeric profiling accumulators (single-pass Welford's algorithm)
        const numericStats: Record<string, { count: number; sum: number; min: number; max: number; m2: number; mean: number }> = {};

        try {
            // Sử dụng fs.createReadStream để tránh load nguyên file vào RAM
            const fileStream = fs.createReadStream(targetPath, { encoding: 'utf-8' });
            
            // Dùng readline phân tích dòng cuốn chiếu off-event-loop
            const rl = readline.createInterface({
                input: fileStream,
                crlfDelay: Infinity // Xử lý được cả \r\n (Windows) và \n (Linux)
            });

            rl.on('line', (line) => {
                // Split thô cho tối ưu hiệu năng (Zero-Dependency)
                // Phù hợp xử lý siêu nhanh hàng triệu dòng.
                const cols = line.split(delimiter);
                
                if (totalRows === 0) {
                    // Xử lý dòng tiêu đề (Header)
                    headers = cols.map(c => c.trim().replace(/^"|"$/g, ''));
                    headers.forEach(h => {
                        if (h) {
                            nullCounts[h] = 0;
                            numericStats[h] = { count: 0, sum: 0, min: Infinity, max: -Infinity, m2: 0, mean: 0 };
                        }
                    });
                } else {
                    // Lấy 5 dòng đầu làm Preview
                    if (totalRows <= 5) {
                        headRows.push(line);
                    }
                    // Thống kê Null counts + Numeric profiling
                    cols.forEach((col, idx) => {
                        const h = headers[idx];
                        if (h) {
                            const val = col.trim().replace(/^"|"$/g, '');
                            if (!val || val === 'null' || val === 'NaN' || val === '""' || val === '-') {
                                nullCounts[h]++;
                            } else {
                                // Welford's online algorithm for variance (single-pass, O(1) memory)
                                const num = Number(val);
                                if (!isNaN(num) && val !== '') {
                                    const stat = numericStats[h];
                                    stat.count++;
                                    stat.sum += num;
                                    if (num < stat.min) stat.min = num;
                                    if (num > stat.max) stat.max = num;
                                    const delta = num - stat.mean;
                                    stat.mean += delta / stat.count;
                                    const delta2 = num - stat.mean;
                                    stat.m2 += delta * delta2;
                                }
                            }
                        }
                    });
                }
                totalRows++;
            });

            rl.on('close', () => {
                logger.info(`[StructuredDataAnalyzer] Hoàn tất quét stream ${totalRows} dòng.`);
                const actualDataRows = totalRows > 0 ? totalRows - 1 : 0;

                // Compile numeric stats — only include columns that have numeric data
                const numericProfile: Record<string, { count: number; min: number; max: number; mean: number; std: number }> = {};
                for (const [col, stat] of Object.entries(numericStats)) {
                    if (stat.count >= 2) {
                        numericProfile[col] = {
                            count: stat.count,
                            min: Math.round(stat.min * 1000) / 1000,
                            max: Math.round(stat.max * 1000) / 1000,
                            mean: Math.round(stat.mean * 1000) / 1000,
                            std: Math.round(Math.sqrt(stat.m2 / (stat.count - 1)) * 1000) / 1000,
                        };
                    }
                }
                
                const summary = {
                    file_name: path.basename(targetPath),
                    total_data_rows: actualDataRows,
                    columns: headers,
                    null_counts: nullCounts,
                    numeric_profile: Object.keys(numericProfile).length > 0 ? numericProfile : undefined,
                    preview_first_5_rows: headRows
                };

                resolve(`[DATA ANALYSIS SUMMARY - STREAM WORKER]\n\n\`\`\`json\n${JSON.stringify(summary, null, 2)}\n\`\`\`\n\n(Lưu ý: Chỉ trả về siêu dữ liệu cấu trúc do file quá lớn. Đừng cố đọc toàn bộ file bằng read_local_file.)`);
            });

            rl.on('error', (err) => {
                logger.error(`[StructuredDataAnalyzer] Lỗi đọc stream: ${err.message}`);
                reject(new Error(`Lỗi đọc file Stream: ${err.message}`));
            });
            
            fileStream.on('error', (err) => {
                if ((err as NodeJS.ErrnoException).code === 'ENOENT') {
                    reject(new Error(`Không tìm thấy file: ${targetPath}`));
                } else {
                    reject(new Error(`Lỗi mở file: ${err.message}`));
                }
            });

        } catch (err: unknown) {
        const errMsg = err instanceof Error ? err.message : String(err);
            reject(new Error(`Lỗi khởi tạo Stream Worker: ${errMsg}`));
        }
    });
};
