import { DatabaseSync } from "node:sqlite";
import path from "node:path";

const dbPath = path.resolve("data/global/structured_memory.sqlite");
console.log("Connecting to:", dbPath);
const db = new DatabaseSync(dbPath);

console.log("\nSearching in turn_layer_nodes...");
const turns = db.prepare("SELECT * FROM turn_layer_nodes WHERE userMsg LIKE '%Trời nay như nào%' OR aiReply LIKE '%trợ lý AI thông minh%'").all();
console.log("Matching turns:", JSON.stringify(turns, null, 2));

console.log("\nSearching in events...");
const events = db.prepare("SELECT * FROM events WHERE rawUserMsg LIKE '%Trời nay như nào%' OR rawAiReply LIKE '%trợ lý AI thông minh%'").all();
console.log("Matching events:", JSON.stringify(events, null, 2));

console.log("\nSearching in facts...");
const facts = db.prepare("SELECT * FROM facts WHERE value LIKE '%Trời nay như nào%' OR value LIKE '%trợ lý AI%' OR value LIKE '%Liva%'").all();
console.log("Matching facts:", JSON.stringify(facts, null, 2));

console.log("\nSearching in vectors_meta...");
const vectors = db.prepare("SELECT * FROM vectors_meta WHERE content LIKE '%Trời nay như nào%' OR content LIKE '%trợ lý AI thông minh%'").all();
console.log("Matching vectors:", JSON.stringify(vectors, null, 2));

db.close();
