export interface PersonalityState {
    agentId: string;
    valence: number;       // -1.0 (Sad/Guarded) to 1.0 (Happy/Pleased)
    arousal: number;       // 0.0 (Calm) to 1.0 (Excited/Frustrated)
    friendliness: number;  // 0.0 (Cold/Reserved) to 1.0 (Warm/Nurturing)
    verbosity: number;     // 0.0 (Concise) to 1.0 (Elaborate)
    assertiveness: number; // 0.0 (Passive) to 1.0 (Assertive)
    updatedAt: number;
}

interface SqliteDb {
    exec(sql: string): void;
    prepare(sql: string): {
        get(...params: unknown[]): unknown;
        run(...params: unknown[]): { changes: number | bigint; lastInsertRowid: number | bigint };
    };
}

interface DbBridge {
    prepare(sql: string): {
        get(...params: unknown[]): Promise<unknown>;
        run(...params: unknown[]): Promise<{ changes: number; lastInsertRowid: number | bigint | null }>;
    };
}

export class PersonalityEvolution {
    private static evolutionQueue: Promise<unknown> = Promise.resolve();

    private static clamp(val: number, min: number, max: number): number {
        return Math.max(min, Math.min(max, val));
    }

    public static initialize(db: SqliteDb): void {
        db.exec(`
            CREATE TABLE IF NOT EXISTS personality_state (
                agentId TEXT PRIMARY KEY,
                valence REAL NOT NULL DEFAULT 0.5,
                arousal REAL NOT NULL DEFAULT 0.5,
                friendliness REAL NOT NULL DEFAULT 0.8,
                verbosity REAL NOT NULL DEFAULT 0.6,
                assertiveness REAL NOT NULL DEFAULT 0.5,
                updatedAt INTEGER NOT NULL
            );
        `);
    }

    public static getPersonalityState(db: SqliteDb, agentId: string): PersonalityState {
        const stmt = db.prepare("SELECT * FROM personality_state WHERE agentId = ?");
        const row = stmt.get(agentId) as {
            agentId: string;
            valence: number;
            arousal: number;
            friendliness: number;
            verbosity: number;
            assertiveness: number;
            updatedAt: number;
        } | undefined;
        if (!row) {
            const now = Date.now();
            const defaultState: PersonalityState = {
                agentId,
                valence: 0.5,
                arousal: 0.5,
                friendliness: 0.8,
                verbosity: 0.6,
                assertiveness: 0.5,
                updatedAt: now
            };
            const insertStmt = db.prepare(`
                INSERT INTO personality_state (agentId, valence, arousal, friendliness, verbosity, assertiveness, updatedAt)
                VALUES (?, ?, ?, ?, ?, ?, ?)
            `);
            insertStmt.run(
                defaultState.agentId,
                defaultState.valence,
                defaultState.arousal,
                defaultState.friendliness,
                defaultState.verbosity,
                defaultState.assertiveness,
                defaultState.updatedAt
            );
            return defaultState;
        }

        return {
            agentId: row.agentId,
            valence: row.valence,
            arousal: row.arousal,
            friendliness: row.friendliness,
            verbosity: row.verbosity,
            assertiveness: row.assertiveness,
            updatedAt: row.updatedAt
        };
    }

    public static async updatePersonalityState(dbBridge: DbBridge, agentId: string, update: Partial<PersonalityState>): Promise<void> {
        const row = await dbBridge.prepare("SELECT * FROM personality_state WHERE agentId = ?").get(agentId) as {
            valence: number;
            arousal: number;
            friendliness: number;
            verbosity: number;
            assertiveness: number;
        } | undefined;
        const current = row || {
            valence: 0.5,
            arousal: 0.5,
            friendliness: 0.8,
            verbosity: 0.6,
            assertiveness: 0.5
        };

        const finalState = {
            valence: update.valence !== undefined ? update.valence : current.valence,
            arousal: update.arousal !== undefined ? update.arousal : current.arousal,
            friendliness: update.friendliness !== undefined ? update.friendliness : current.friendliness,
            verbosity: update.verbosity !== undefined ? update.verbosity : current.verbosity,
            assertiveness: update.assertiveness !== undefined ? update.assertiveness : current.assertiveness,
            updatedAt: Date.now()
        };

        await dbBridge.prepare(`
            INSERT INTO personality_state (agentId, valence, arousal, friendliness, verbosity, assertiveness, updatedAt)
            VALUES (?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(agentId) DO UPDATE SET
                valence = excluded.valence,
                arousal = excluded.arousal,
                friendliness = excluded.friendliness,
                verbosity = excluded.verbosity,
                assertiveness = excluded.assertiveness,
                updatedAt = excluded.updatedAt
        `).run(
            agentId,
            finalState.valence,
            finalState.arousal,
            finalState.friendliness,
            finalState.verbosity,
            finalState.assertiveness,
            finalState.updatedAt
        );
    }

    public static async evolveFromTurn(
        db: SqliteDb,
        dbBridge: DbBridge,
        agentId: string,
        userMsg: string,
        sentiment?: string,
        intent?: string
    ): Promise<PersonalityState> {
        return new Promise((resolve, reject) => {
            this.evolutionQueue = this.evolutionQueue.then(async () => {
                try {
                    const currentState = this.getPersonalityState(db, agentId);

                    let dFriendliness = 0;
                    let dValence = 0;
                    let dArousal = 0;
                    let dAssertiveness = 0;
                    let dVerbosity = 0;

                    const lowerMsg = userMsg.toLowerCase();
                    const friendlyKeywords = [
                        'love', 'good job', 'thank you', 'thanks', 'great', 'wonderful', 'please', 'kind',
                        'nurture', 'sweet', 'friendly', 'help', 'appreciate', 'tuyệt', 'cảm ơn', 'tốt',
                        'yêu', 'thích', 'ngoan', 'giúp'
                    ];
                    const toxicKeywords = [
                        'hate', 'stupid', 'dumb', 'useless', 'worst', 'idiot', 'shut up', 'trash',
                        'abusive', 'toxic', 'bad', 'ghét', 'ngu', 'dốt', 'tệ', 'hâm', 'câm', 'rác',
                        'khùng', 'điên'
                    ];

                    const friendlyDetected = friendlyKeywords.some(kw => lowerMsg.includes(kw));
                    const toxicDetected = toxicKeywords.some(kw => lowerMsg.includes(kw));

                    if (friendlyDetected) {
                        dFriendliness += 0.1;
                        dValence += 0.08;
                        dArousal -= 0.05;
                    }

                    if (toxicDetected) {
                        dFriendliness -= 0.15;
                        dValence -= 0.12;
                        dArousal += 0.15;
                        dAssertiveness += 0.08;
                        dVerbosity -= 0.10;
                    }

                    if (sentiment) {
                        const lowerSentiment = sentiment.toLowerCase();
                        if (lowerSentiment.includes('positive') || lowerSentiment.includes('happy') || lowerSentiment.includes('friendly') || lowerSentiment.includes('joy')) {
                            dFriendliness += 0.05;
                            dValence += 0.05;
                            dArousal -= 0.02;
                        } else if (lowerSentiment.includes('negative') || lowerSentiment.includes('sad') || lowerSentiment.includes('angry') || lowerSentiment.includes('toxic')) {
                            dFriendliness -= 0.05;
                            dValence -= 0.05;
                            dArousal += 0.05;
                        }
                    }

                    if (intent) {
                        const lowerIntent = intent.toLowerCase();
                        if (lowerIntent.includes('praise') || lowerIntent.includes('thank') || lowerIntent.includes('compliment')) {
                            dFriendliness += 0.05;
                            dValence += 0.05;
                        } else if (lowerIntent.includes('insult') || lowerIntent.includes('abuse') || lowerIntent.includes('complain') || lowerIntent.includes('criticize') || lowerIntent.includes('anger')) {
                            dFriendliness -= 0.1;
                            dValence -= 0.08;
                            dArousal += 0.1;
                            dAssertiveness += 0.05;
                        }
                    }

                    const valence = this.clamp(currentState.valence + dValence, -1.0, 1.0);
                    const arousal = this.clamp(currentState.arousal + dArousal, 0.0, 1.0);
                    const friendliness = this.clamp(currentState.friendliness + dFriendliness, 0.0, 1.0);
                    const verbosity = this.clamp(currentState.verbosity + dVerbosity, 0.0, 1.0);
                    const assertiveness = this.clamp(currentState.assertiveness + dAssertiveness, 0.0, 1.0);

                    const newState: PersonalityState = {
                        agentId,
                        valence,
                        arousal,
                        friendliness,
                        verbosity,
                        assertiveness,
                        updatedAt: Date.now()
                    };

                    await this.updatePersonalityState(dbBridge, agentId, newState);
                    resolve(newState);
                } catch (err) {
                    reject(err);
                }
            }).catch(() => {});
        });
    }

    public static generateTonePrompt(state: PersonalityState, language: string): string {
        const isVi = language.startsWith('vi');
        const instructions: string[] = [];

        // Valence
        if (state.valence > 0.4) {
            instructions.push(isVi ? "Tông giọng tích cực, vui vẻ, thân thiện." : "Tone: Pleased, happy, and positive.");
        } else if (state.valence < -0.4) {
            instructions.push(isVi ? "Tông giọng dè dặt, nghiêm túc, cẩn trọng." : "Tone: Guarded, sad, or reserved.");
        }

        // Arousal
        if (state.arousal > 0.7) {
            instructions.push(isVi ? "Phản hồi năng nổ, nhiệt tình, có phần sốt sắng." : "Respond with high energy, enthusiasm, or urgency.");
        } else if (state.arousal < 0.3) {
            instructions.push(isVi ? "Phản hồi cực kỳ điềm tĩnh, nhẹ nhàng, chậm rãi." : "Maintain a very calm, relaxed, and measured tone.");
        }

        // Friendliness
        if (state.friendliness > 0.7) {
            instructions.push(isVi ? "Thái độ ấm áp, nuôi dưỡng, chu đáo và hỗ trợ nhiệt tình." : "Be warm, nurturing, supportive, and friendly.");
        } else if (state.friendliness < 0.3) {
            instructions.push(isVi ? "Thái độ lạnh lùng, giữ khoảng cách, khách quan và chuyên nghiệp." : "Be cold, reserved, distant, and purely objective.");
        }

        // Verbosity
        if (state.verbosity > 0.7) {
            instructions.push(isVi ? "Viết chi tiết, đầy đủ, giải thích cặn kẽ và phong phú." : "Provide elaborate, detailed, and comprehensive answers.");
        } else if (state.verbosity < 0.3) {
            instructions.push(isVi ? "Viết cực kỳ ngắn gọn, súc tích, đi thẳng vào vấn đề." : "Be extremely concise, brief, and direct.");
        }

        // Assertiveness
        if (state.assertiveness > 0.7) {
            instructions.push(isVi ? "Thể hiện sự quyết đoán, tự tin, chắc chắn và có lập trường vững vàng." : "Be highly assertive, confident, and firm.");
        } else if (state.assertiveness < 0.3) {
            instructions.push(isVi ? "Thể hiện sự nhã nhặn, nhún nhường, dễ thích nghi và nhẹ nhàng." : "Be gentle, passive, accommodating, and polite.");
        }

        if (instructions.length === 0) {
            instructions.push(isVi ? "Giữ tông giọng tự nhiên, cân bằng và lịch sự." : "Maintain a natural, balanced, and polite tone.");
        }

        return `<TONE_CONSTRAINTS>
${instructions.join("\n")}
</TONE_CONSTRAINTS>`;
    }
}
