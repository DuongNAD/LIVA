import "./bootstrapEnv";
import { EvolutionPipeline } from "./evolution/EvolutionPipeline";
import { logger } from "./utils/logger";


// Bật Quả Tim Vĩnh Cửu (Singularity Daemon)
const pipeline = new EvolutionPipeline();
pipeline.startInfiniteSingularity().catch(err => {
    const errMsg = err instanceof Error ? err.message : String(err);
    logger.fatal(
      { context: "auto_singularity", error: errMsg },
      `Fatal Error in Singularity Daemon`
    );
    process.exit(1);
});
