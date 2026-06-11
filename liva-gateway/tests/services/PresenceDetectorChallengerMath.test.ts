import { describe, it, expect } from "vitest";
import { exec } from "child_process";
import { promisify } from "util";

const execAsync = promisify(exec);

describe("PresenceDetector C# Math Challenger Tests", () => {
  it.skipIf(process.platform !== "win32")("should verify C# unsigned integer subtraction behavior under wrap-around via PowerShell", async () => {
    const psCommand = `powershell -NoProfile -NonInteractive -Command "Add-Type -TypeDefinition 'using System; public class MathTest2 { public static void Run() { uint tickCountNormal = 2147483647; uint dwTimeNormal = 2147483547; Console.WriteLine(tickCountNormal - dwTimeNormal); int neg = -2147483648; uint tickCountWrap1 = unchecked((uint)neg); uint dwTimeWrap1 = 2147483647; Console.WriteLine(tickCountWrap1 - dwTimeWrap1); uint tickCountWrap2 = 10; uint dwTimeWrap2 = 4294967290; try { uint result = tickCountWrap2 - dwTimeWrap2; Console.WriteLine(result); } catch (OverflowException) { Console.WriteLine(-1); } } }'; [MathTest2]::Run();"`;

    const { stdout } = await execAsync(psCommand);
    
    expect(stdout).toContain("100");
    expect(stdout).toContain("1");
    expect(stdout).toContain("16");
  });
});
