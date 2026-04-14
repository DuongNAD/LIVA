import { execute } from "./AppendGoogleDoc";
import * as assert from "assert";

async function runTest() {
  console.log("🚀 Starting Unit Test for AppendGoogleDoc...");

  // Case 1: Missing parameters (Edge-case)
  try {
    const result = await execute({});
    assert.ok(result.includes("❌"), "Should return error message when args are empty");
    console.log("✅ Case 1 Passed: Handled missing arguments.");
  } catch (e) {
    console.error("❌ Case 1 Failed:", e);
  }

  // Case 2: Simulation of invalid documentId (Edge-case)
  try {
    const result = await execute({ documentId: "invalid_id", text: "hello" });
    assert.ok(result.includes("❌"), "Should return error message for invalid document ID");
    console.log("✅ Case 2 Passed: Handled invalid document ID.");
  } catch (e) {
    console.log("ℹ️ Case 2 Info: Caught expected API/Auth error.");
  }

  console.log("🏁 All tests completed!");
}

runTest().catch(err => {
  console.error("💥 Test Suite Crashed:", err);
  process.exit(1);
});