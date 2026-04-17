import * as lancedb from "@lancedb/lancedb";
import * as path from "path";

async function run() {
  const dbDir = path.join(process.cwd(), "data", "lancedb_test");
  const db = await lancedb.connect(dbDir);
  const data = [{ vector: [1,2], text: "hello", type: "SUCCESS" }];
  const tableRaw = await db.createTable("test_table", data, { existOk: true });
  console.log("Vector Search Output:", typeof tableRaw.vectorSearch);
  const results = await tableRaw.vectorSearch([1,2]).limit(1).toArray();
  console.log("Vector Search Results:", results);

  const filterResults = await tableRaw.query().where("type != 'AXIOM'").toArray();
  console.log("Filter Results:", filterResults);
}
run().catch(console.error);
