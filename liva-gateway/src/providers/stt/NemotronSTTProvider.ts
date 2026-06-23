import { NemotronSTTService } from "../../services/NemotronSTTService";
import { ISTTProvider } from "../ISTTProvider";

/**
 * NemotronSTTProvider — Implements ISTTProvider by subclassing NemotronSTTService.
 */
export class NemotronSTTProvider extends NemotronSTTService implements ISTTProvider {
    constructor() {
        super();
    }
}
