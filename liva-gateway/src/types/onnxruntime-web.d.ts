/**
 * Minimal type declarations for onnxruntime-web.
 * Only covers the subset used by SmartTurnVAD.ts (InferenceSession).
 * Full types are not available on DefinitelyTyped — this stub prevents tsc errors
 * while preserving runtime dynamic import() behavior.
 */
declare module "onnxruntime-web" {
    export interface InferenceSessionOptions {
        executionProviders?: string[];
        graphOptimizationLevel?: "disabled" | "basic" | "extended" | "all";
    }

    export interface OnnxValue {
        data: Float32Array | Int32Array | BigInt64Array | Uint8Array;
        dims: number[];
        type: string;
    }

    export interface RunOptions {
        logSeverityLevel?: number;
    }

    export interface InferenceSession {
        run(
            feeds: Record<string, OnnxValue>,
            options?: RunOptions
        ): Promise<Record<string, OnnxValue>>;
        release(): Promise<void>;
        readonly inputNames: string[];
        readonly outputNames: string[];
    }

    export const InferenceSession: {
        create(
            path: string | ArrayBuffer,
            options?: InferenceSessionOptions
        ): Promise<InferenceSession>;
    };

    export class Tensor {
        constructor(
            type: string,
            data: Float32Array | Int32Array | BigInt64Array | Uint8Array | number[],
            dims?: number[]
        );
        readonly data: Float32Array | Int32Array | BigInt64Array | Uint8Array;
        readonly dims: number[];
        readonly type: string;
    }
    export const env: {
        wasm: {
            numThreads: number;
            simd: boolean;
            proxy: boolean;
            wasmPaths?: string;
        };
        logLevel?: string;
    };
}
