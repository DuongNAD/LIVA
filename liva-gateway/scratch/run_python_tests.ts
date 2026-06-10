import { spawn } from "child_process";
import fs from "fs";
import path from "path";

const pythonTestPath = path.resolve("../liva-ai-engine");
const venvPython = path.join(pythonTestPath, "venv", "bin", "python");

console.log("Running native engine fixes test file via:", venvPython);

const proc = spawn(venvPython, ["-m", "pytest", "tests/test_native_engine_fixes.py", "-vv"], {
  cwd: pythonTestPath,
  env: { ...process.env }
});

let stdout = "";
let stderr = "";

proc.stdout.on("data", (data) => {
  stdout += data.toString();
  process.stdout.write(data);
});

proc.stderr.on("data", (data) => {
  stderr += data.toString();
  process.stderr.write(data);
});

proc.on("close", (code) => {
  const output = `Exit Code: ${code}\n\nSTDOUT:\n${stdout}\n\nSTDERR:\n${stderr}`;
  fs.writeFileSync("scratch/python_test_results.txt", output);
  console.log("\nDone! Output written to scratch/python_test_results.txt");
});
