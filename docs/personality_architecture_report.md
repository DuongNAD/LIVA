# LIVA Dynamic Evolving Personality Architecture Report

## 1. Architectural Approach
LIVA implements a multi-dimensional emotional and style coordinate system (personality state) mapped to five main metrics:
*   **Valence (-1.0 to 1.0)**: Measures emotional positivity. Reflects happy/pleased vs. sad/guarded states.
*   **Arousal (0.0 to 1.0)**: Measures emotional activation level. Reflects calm vs. excited/frustrated states.
*   **Friendliness (0.0 to 1.0)**: Measures warmness. Reflects cold/reserved vs. warm/nurturing attitudes.
*   **Verbosity (0.0 to 1.0)**: Measures detail preference. Reflects concise vs. elaborate output preferences.
*   **Assertiveness (0.0 to 1.0)**: Measures confidence/dominance. Reflects passive vs. assertive/firm behavior.

### Hybrid Storage Pattern
To ensure high performance and non-blocking I/O, the personality state is persisted in an SQLite database (`personality_state` table) using a hybrid read/write pattern:
1.  **Zero-Latency Synchronous Read**: Prompt construction requires instant access to the personality state. Therefore, it is fetched synchronously from the main thread SQLite instance (`this.db`).
2.  **Asynchronous Non-Blocking Write**: When updating the personality state after a turn, the changes are processed asynchronously via the `DatabaseWorkerBridge` thread to avoid blocking the main Event Loop.

### Evolving Personality Transitions
Personality states evolve deterministically based on user interactions:
*   **Interaction Keyword Extraction**: The user's input is checked for specific friendly or toxic keywords (supporting English and Vietnamese) to adjust state values.
*   **Sentiment/Intent Context**: Standard NLP-derived sentiment or intent classifications are mapped to precise coordinates.
*   **Strict Bounds clamping**: Coordinates are clamped to their respective bounds (`[0, 1]` or `[-1, 1]`) to prevent value divergence.

---

## 2. Open-Source References & Academic Inspiration
Our approach draws from proven academic and open-source models:
*   **PAD Emotional State Model**: Originally developed by Albert Mehrabian and James A. Russell (1974), the Pleasure-Arousal-Dominance (PAD) model represents emotional states. We adapted this into Valence (Pleasure), Arousal (Arousal), and Assertiveness (Dominance), adding Friendliness and Verbosity to explicitly govern agent persona style.
*   **Virtual Agent Persona Architectures**: Standard implementations of game AI and virtual assistants (like ALICE or modern LLM character frameworks) use similar coordinate systems to update state variables incrementally, ensuring personality consistency.

---

## 3. Why it is the Best Fit for LIVA
This design is ideal for LIVA's lightweight gateway architecture:
*   **KV Cache Efficiency**: Modifying the system prompt based on style guidelines is packaged inside structured `<TONE_CONSTRAINTS>` tags.
*   **No Main-Thread Blocking**: All heavy write/update operations run inside the SQLite worker thread.
*   **Highly Controllable & Deterministic**: Unlike prompting the LLM to "be friendly", adjusting state coordinates based on keywords and sentiment yields highly predictable behavior and prevents style drift.
