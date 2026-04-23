import * as dotenv from "dotenv";
import { EvolutionPipeline } from "./evolution/EvolutionPipeline";

dotenv.config();

// Bật Quả Tim Vĩnh Cửu (Singularity Daemon)
const pipeline = new EvolutionPipeline();
pipeline.startInfiniteSingularity().catch(err => {
    console.error("Fatal Error in Singularity Daemon:", err);
    process.exit(1);
});
