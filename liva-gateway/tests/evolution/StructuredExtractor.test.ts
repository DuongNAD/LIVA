import { describe, it, expect } from 'vitest';
import { 
    extractXMLPatches, 
    extractAndValidate, 
    PopulationSchema, 
    QualityAssessmentSchema 
} from '../../src/evolution/StructuredExtractor';

describe('StructuredExtractor', () => {
    describe('extractXMLPatches', () => {
        it('should extract valid XML patches', () => {
            const rawText = `
                <candidate id="test-1">
                    <patch filePath="src/main.ts">
                        console.log("hello");
                    </patch>
                    <patch filePath="src/util.ts">
                        export const x = 1;
                    </patch>
                </candidate>
                <candidate id="test-2">
                    <patch filePath="src/index.ts">
                        init();
                    </patch>
                </candidate>
            `;
            const result = extractXMLPatches(rawText);
            expect(result.success).toBe(true);
            expect(result.method).toBe('xml_regex');
            expect(result.data?.population).toHaveLength(2);
            expect(result.data?.population[0].id).toBe('test-1');
            expect(result.data?.population[0].mutations).toHaveLength(2);
            expect(result.data?.population[0].mutations[0].filePath).toBe('src/main.ts');
            expect(result.data?.population[0].mutations[0].code).toBe('console.log("hello");');
        });

        it('should return failed result when no valid tags are present', () => {
            const rawText = `Some random text without candidate tags`;
            const result = extractXMLPatches(rawText);
            expect(result.success).toBe(false);
            expect(result.method).toBe('failed');
            expect(result.errors).toContain('Không tìm thấy thẻ <candidate> và <<<< SEARCH hợp lệ.');
        });
    });

    describe('extractAndValidate', () => {
        it('should extract and validate JSON from markdown fences', () => {
            const rawText = `
                Here is the response:
                \`\`\`json
                {
                    "pass": true,
                    "feedback": "Looks good"
                }
                \`\`\`
            `;
            const result = extractAndValidate(rawText, QualityAssessmentSchema);
            expect(result.success).toBe(true);
            expect(result.method).toBe('json_fence');
            expect(result.data?.pass).toBe(true);
            expect(result.data?.feedback).toBe('Looks good');
        });

        it('should strip <think> blocks before processing', () => {
            const rawText = `
                <think>
                    { "some": "random thoughts that break brace matching" }
                </think>
                \`\`\`json
                {
                    "pass": false,
                    "feedback": "Bad code"
                }
                \`\`\`
            `;
            const result = extractAndValidate(rawText, QualityAssessmentSchema);
            expect(result.success).toBe(true);
            expect(result.data?.pass).toBe(false);
        });

        it('should fallback to brace matching if no fences', () => {
            const rawText = `
                Some intro text...
                {
                    "population": [
                        {
                            "id": "c1",
                            "mutations": [
                                {
                                    "type": "modify",
                                    "filePath": "a.ts",
                                    "code": "a=1"
                                }
                            ]
                        }
                    ]
                }
                Some outro text...
            `;
            const result = extractAndValidate(rawText, PopulationSchema);
            expect(result.success).toBe(true);
            expect(result.method).toBe('brace_match');
            expect(result.data?.population).toHaveLength(1);
        });

        it('should use jsonrepair for slightly broken JSON', () => {
            const rawText = `
                {
                    pass: true,
                    feedback: "Missing quotes"
                }
            `; // Keys without quotes
            const result = extractAndValidate(rawText, QualityAssessmentSchema);
            expect(result.success).toBe(true);
            expect(result.method).toBe('jsonrepair');
            expect(result.data?.pass).toBe(true);
        });

        it('should fail when brace matching finds invalid JSON that cannot be repaired', () => {
            const rawText = `
                {
                    "pass": invalid_value,,,,,
                    "feedback": "Bad"
                }
            `;
            const result = extractAndValidate(rawText, QualityAssessmentSchema);
            expect(result.success).toBe(false);
            expect(result.method).toBe('failed');
            expect(result.errors.length).toBeGreaterThan(0);
            expect(result.errors.some(e => e.includes('Zod validation failed') || e.includes('JSON.parse failed') || e.includes('jsonrepair'))).toBe(true);
        });

        it('should fail when valid JSON does not match schema', () => {
            const rawText = `
                \`\`\`json
                {
                    "wrong_field": true
                }
                \`\`\`
            `;
            const result = extractAndValidate(rawText, QualityAssessmentSchema);
            expect(result.success).toBe(false);
            expect(result.method).toBe('failed');
            expect(result.errors.some(e => e.includes('Zod validation failed'))).toBe(true);
        });

        it('should handle strings for boolean fields via Zod preprocessing', () => {
            const rawText = `
                {
                    "pass": "true",
                    "feedback": "Test string true"
                }
            `;
            const result = extractAndValidate(rawText, QualityAssessmentSchema);
            expect(result.success).toBe(true);
            expect(result.data?.pass).toBe(true);

            const rawTextFalse = `
                {
                    "pass": "false",
                    "feedback": "Test string false"
                }
            `;
            const resultFalse = extractAndValidate(rawTextFalse, QualityAssessmentSchema);
            expect(resultFalse.success).toBe(true);
            expect(resultFalse.data?.pass).toBe(false);
        });
        
        it('should ignore escaped braces or string braces in brace matching', () => {
            const rawText = `
                intro
                {
                    "feedback": "a { string } with braces",
                    "pass": true
                }
                outro
            `;
            const result = extractAndValidate(rawText, QualityAssessmentSchema);
            expect(result.success).toBe(true);
            expect(result.method).toBe('brace_match');
            expect(result.data?.feedback).toBe('a { string } with braces');
        });
    });
});
