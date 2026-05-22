import { logger } from "../utils/logger";
import { exec } from "node:child_process";
import { promisify } from "node:util";

const execAsync = promisify(exec);

// ─── Constants ───────────────────────────────────────────────────────────────

const PS_TIMEOUT_MS = 10_000; // 10 giây timeout cho mọi lệnh PowerShell
const PS_EXEC_OPTIONS = { timeout: PS_TIMEOUT_MS, maxBuffer: 1024 * 1024 };

// ─── PowerShell Helper — chạy script với timeout và error handling ────────────

async function runPowerShell(script: string): Promise<string> {
    // Escape single quotes trong script cho PowerShell -Command
    const encoded = Buffer.from(
        // PowerShell Unicode encoding (UTF-16LE) cho -EncodedCommand
        `\ufeff${script}`,
        "utf-16le"
    ).toString("base64");

    const { stdout } = await execAsync(
        `powershell -NoProfile -NonInteractive -EncodedCommand ${encoded}`,
        PS_EXEC_OPTIONS
    );

    return stdout.trim();
}

// ─── Structured Result Types ─────────────────────────────────────────────────

export interface CursorElementInfo {
    name: string;
    value: string;
    controlType: string;
    boundingRect: string;
    processId: number;
    error?: string;
}

export interface EditorContent {
    content: string;
    fileName: string;
    cursorLine: number;
    source: "vscode_bridge" | "uia_fallback";
}

export interface ScreenRegionResult {
    texts: string[];
    elementCount: number;
}

// ─── ScreenContextTelepathy Service ──────────────────────────────────────────

export class ScreenContextTelepathy {

    /**
     * Đọc thông tin UI element tại vị trí chuột hiện tại.
     * Sử dụng PowerShell UIAutomation COM — KHÔNG dùng native addon.
     */
    async getTextAtCursor(): Promise<CursorElementInfo> {
        const script = `
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
Add-Type -AssemblyName System.Windows.Forms

$cursor = [System.Windows.Forms.Cursor]::Position
$point = New-Object System.Windows.Point($cursor.X, $cursor.Y)

try {
    $automation = [System.Windows.Automation.AutomationElement]
    $element = $automation::FromPoint($point)

    if ($element -ne $null) {
        $name = $element.GetCurrentPropertyValue([System.Windows.Automation.AutomationElement]::NameProperty)
        $value = ''
        try {
            $valuePattern = $element.GetCurrentPattern([System.Windows.Automation.ValuePatternIdentifiers]::Pattern)
            $value = $valuePattern.Current.Value
        } catch { }

        $textValue = ''
        try {
            $textPattern = $element.GetCurrentPattern([System.Windows.Automation.TextPatternIdentifiers]::Pattern)
            $range = $textPattern.DocumentRange
            $textValue = $range.GetText(2000)
        } catch { }

        $finalValue = if ($value) { $value } elseif ($textValue) { $textValue } else { '' }

        $result = @{
            Name = if ($name) { $name } else { '' }
            Value = $finalValue
            ControlType = $element.Current.ControlType.ProgrammaticName
            BoundingRect = $element.Current.BoundingRectangle.ToString()
            ProcessId = $element.Current.ProcessId
        }
        $result | ConvertTo-Json -Compress
    } else {
        '{"error":"No element found at cursor position"}'
    }
} catch {
    $errObj = @{ error = $_.Exception.Message }
    $errObj | ConvertTo-Json -Compress
}
`;
        try {
            const raw = await runPowerShell(script);
            if (!raw) {
                return {
                    name: "",
                    value: "",
                    controlType: "",
                    boundingRect: "",
                    processId: 0,
                    error: "Empty response from PowerShell",
                };
            }

            const parsed = JSON.parse(raw);

            if (parsed.error) {
                logger.warn(`[ScreenTelepathy] getTextAtCursor error: ${parsed.error}`);
                return {
                    name: "",
                    value: "",
                    controlType: "",
                    boundingRect: "",
                    processId: 0,
                    error: parsed.error,
                };
            }

            const result: CursorElementInfo = {
                name: parsed.Name || "",
                value: parsed.Value || "",
                controlType: parsed.ControlType || "",
                boundingRect: parsed.BoundingRect || "",
                processId: parsed.ProcessId || 0,
            };

            logger.info(
                `[ScreenTelepathy] Cursor element: "${result.name}" [${result.controlType}] PID:${result.processId}`
            );
            return result;
        } catch (err: unknown) {
            const errMsg = err instanceof Error ? err.message : String(err);
            logger.error(`[ScreenTelepathy] getTextAtCursor lỗi: ${errMsg}`);
            return {
                name: "",
                value: "",
                controlType: "",
                boundingRect: "",
                processId: 0,
                error: errMsg,
            };
        }
    }

    /**
     * Đọc nội dung editor đang active.
     * Ưu tiên 1: VSCodeBridge WebSocket (nếu connected).
     * Ưu tiên 2: UIA fallback — đọc focused text control.
     */
    async getActiveEditorContent(): Promise<EditorContent> {
        // ──── Thử VSCode Bridge trước ────
        try {
            const bridge = globalThis.kernelInstance?.vscodeBridge;
            if (bridge && typeof bridge.executeCommand === "function") {
                const response = await Promise.race([
                    bridge.executeCommand("getActiveEditorContent"),
                    new Promise<null>((resolve) => setTimeout(() => resolve(null), 5000)),
                ]);

                if (response && typeof response === "object") {
                    const data = response as Record<string, unknown>;
                    if (data.content && typeof data.content === "string") {
                        logger.info(`[ScreenTelepathy] VSCode Bridge: got editor content (${data.fileName || "unknown"})`);
                        return {
                            content: data.content as string,
                            fileName: (data.fileName as string) || "unknown",
                            cursorLine: (data.cursorLine as number) || 0,
                            source: "vscode_bridge",
                        };
                    }
                }
            }
        } catch {
            // VSCodeBridge không khả dụng — fallback UIA
        }

        // ──── Fallback: UIA — đọc focused element ────
        const script = `
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes

try {
    $root = [System.Windows.Automation.AutomationElement]::FocusedElement

    if ($root -eq $null) {
        '{"error":"No focused element"}'
        return
    }

    $processId = $root.Current.ProcessId
    $proc = Get-Process -Id $processId -ErrorAction SilentlyContinue
    $procName = if ($proc) { $proc.ProcessName } else { '' }
    $windowTitle = if ($proc) { $proc.MainWindowTitle } else { '' }

    $value = ''
    try {
        $textPattern = $root.GetCurrentPattern([System.Windows.Automation.TextPatternIdentifiers]::Pattern)
        $range = $textPattern.DocumentRange
        $value = $range.GetText(5000)
    } catch {
        try {
            $valuePattern = $root.GetCurrentPattern([System.Windows.Automation.ValuePatternIdentifiers]::Pattern)
            $value = $valuePattern.Current.Value
        } catch {
            $value = $root.GetCurrentPropertyValue([System.Windows.Automation.AutomationElement]::NameProperty)
        }
    }

    $result = @{
        Content = if ($value) { $value } else { '' }
        FileName = $windowTitle
        ProcessName = $procName
        ControlType = $root.Current.ControlType.ProgrammaticName
    }
    $result | ConvertTo-Json -Compress
} catch {
    $errObj = @{ error = $_.Exception.Message }
    $errObj | ConvertTo-Json -Compress
}
`;
        try {
            const raw = await runPowerShell(script);
            if (!raw) {
                return {
                    content: "",
                    fileName: "unknown",
                    cursorLine: 0,
                    source: "uia_fallback",
                };
            }

            const parsed = JSON.parse(raw);
            if (parsed.error) {
                logger.warn(`[ScreenTelepathy] getActiveEditorContent UIA error: ${parsed.error}`);
                return {
                    content: "",
                    fileName: "unknown",
                    cursorLine: 0,
                    source: "uia_fallback",
                };
            }

            // Trích xuất tên file từ window title (VD: "index.ts - Visual Studio Code")
            const windowTitle: string = parsed.FileName || "";
            const fileNameMatch = windowTitle.match(/^(.+?)\s*[-–—●]\s*/);
            const fileName = fileNameMatch ? fileNameMatch[1].trim() : windowTitle;

            logger.info(
                `[ScreenTelepathy] UIA fallback: got content from "${parsed.ProcessName}" (${fileName})`
            );

            return {
                content: parsed.Content || "",
                fileName,
                cursorLine: 0, // UIA không cung cấp cursor line
                source: "uia_fallback",
            };
        } catch (err: unknown) {
            const errMsg = err instanceof Error ? err.message : String(err);
            logger.error(`[ScreenTelepathy] getActiveEditorContent lỗi: ${errMsg}`);
            return {
                content: "",
                fileName: "unknown",
                cursorLine: 0,
                source: "uia_fallback",
            };
        }
    }

    /**
     * Đọc tất cả text trong một vùng màn hình (bounding rectangle).
     * Enumerate UIA elements, thu thập Name và Value properties.
     */
    async getScreenRegionText(
        x: number,
        y: number,
        width: number,
        height: number
    ): Promise<ScreenRegionResult> {
        const script = `
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes

try {
    $root = [System.Windows.Automation.AutomationElement]::RootElement

    # Tạo bounding rect condition — UIA không hỗ trợ rect filter trực tiếp
    # nên ta dùng TreeWalker để duyệt tất cả element con và filter theo vị trí
    $walker = [System.Windows.Automation.TreeWalker]::ContentViewWalker

    $targetLeft = ${x}
    $targetTop = ${y}
    $targetRight = ${x} + ${width}
    $targetBottom = ${y} + ${height}

    $texts = [System.Collections.Generic.List[string]]::new()
    $maxElements = 200
    $count = 0

    function Traverse-Element {
        param($element, [int]$depth)

        if ($count -ge $maxElements -or $depth -gt 8) { return }

        try {
            $rect = $element.Current.BoundingRectangle
            if ($rect.IsEmpty) { return }

            # Kiểm tra overlap với target region
            $overlaps = ($rect.Left -lt $targetRight) -and ($rect.Right -gt $targetLeft) -and
                        ($rect.Top -lt $targetBottom) -and ($rect.Bottom -gt $targetTop)

            if ($overlaps) {
                $name = $element.GetCurrentPropertyValue([System.Windows.Automation.AutomationElement]::NameProperty)
                $value = ''
                try {
                    $vp = $element.GetCurrentPattern([System.Windows.Automation.ValuePatternIdentifiers]::Pattern)
                    $value = $vp.Current.Value
                } catch { }

                $text = if ($value) { $value } elseif ($name -and $name.Length -gt 0 -and $name.Length -lt 500) { $name } else { $null }

                if ($text -and $text.Trim().Length -gt 0) {
                    $texts.Add($text.Trim())
                    $script:count++
                }
            }
        } catch { }

        # Duyệt con
        try {
            $child = $walker.GetFirstChild($element)
            while ($child -ne $null -and $count -lt $maxElements) {
                Traverse-Element -element $child -depth ($depth + 1)
                $child = $walker.GetNextSibling($child)
            }
        } catch { }
    }

    # Bắt đầu từ root
    $firstChild = $walker.GetFirstChild($root)
    while ($firstChild -ne $null -and $count -lt $maxElements) {
        Traverse-Element -element $firstChild -depth 0
        $firstChild = $walker.GetNextSibling($firstChild)
    }

    # Deduplicate
    $uniqueTexts = $texts | Select-Object -Unique

    $result = @{
        Texts = @($uniqueTexts)
        ElementCount = $uniqueTexts.Count
    }
    $result | ConvertTo-Json -Compress -Depth 3
} catch {
    $errObj = @{ error = $_.Exception.Message; Texts = @(); ElementCount = 0 }
    $errObj | ConvertTo-Json -Compress
}
`;
        try {
            const raw = await runPowerShell(script);
            if (!raw) {
                return { texts: [], elementCount: 0 };
            }

            const parsed = JSON.parse(raw);

            if (parsed.error) {
                logger.warn(`[ScreenTelepathy] getScreenRegionText error: ${parsed.error}`);
                return { texts: [], elementCount: 0 };
            }

            const texts: string[] = Array.isArray(parsed.Texts) ? parsed.Texts : [];
            const elementCount: number = parsed.ElementCount || texts.length;

            logger.info(
                `[ScreenTelepathy] Screen region (${x},${y} ${width}x${height}): ${elementCount} text elements found`
            );

            return { texts, elementCount };
        } catch (err: unknown) {
            const errMsg = err instanceof Error ? err.message : String(err);
            logger.error(`[ScreenTelepathy] getScreenRegionText lỗi: ${errMsg}`);
            return { texts: [], elementCount: 0 };
        }
    }

    /**
     * Stateless service — không cần cleanup tài nguyên.
     */
    dispose(): void {
        logger.debug("[ScreenTelepathy] Disposed (stateless — no cleanup needed).");
    }
}
