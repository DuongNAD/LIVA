//! Integration and E2E Acceptance Test Suite for Milestone U13:
//! Semantic L2 -> L3 Memory Consolidation & HippoRAG Multi-hop Knowledge Graph.

use liva_native_core::crypto::EncryptionEngine;
use liva_native_core::db::{
    DatabasePool, MEMORY_VECTOR_DIM, csr_graph::CsrGraph, persist_conversation_event_vector,
};
use liva_native_core::memory_consolidation::{consume_pending_once, process_pending_batch};

#[test]
fn test_end_to_end_semantic_consolidation_populates_l3_nodes_and_edges() {
    let pool = DatabasePool::new_in_memory().expect("in-memory db initialization");
    let conn = pool.writer.get().expect("writer connection");
    let crypto = EncryptionEngine::new("test-consolidation-suite-key-32b");

    // Seed conversation event 1 & its vector projection
    conn.execute(
        "INSERT INTO events (
            eventId, timestamp, consolidated, domain, category,
            consolidation_status, retry_count, agentId
         ) VALUES (
            'turn_1', 1000, 0, 'memory_owner:local',
            'conversation:default', 'pending', 0, 'liva_core'
         )",
        [],
    )
    .expect("seed event 1");

    conn.execute(
        "INSERT INTO vectors_meta (
            vec_id, type, content, domain, category,
            source_event_ids, created_at
         ) VALUES (
            'turn_1', 'conversation_turn',
            'User: Alice là bạn của Bob.\nAssistant: Đúng vậy, Bob làm việc tại Google.',
            'memory_owner:local', 'conversation:default',
            '[\"turn_1\"]', 1000
         )",
        [],
    )
    .expect("seed vector 1");

    // Seed conversation event 2 & its vector projection
    conn.execute(
        "INSERT INTO events (
            eventId, timestamp, consolidated, domain, category,
            consolidation_status, retry_count, agentId
         ) VALUES (
            'turn_2', 2000, 0, 'memory_owner:local',
            'conversation:default', 'pending', 0, 'liva_core'
         )",
        [],
    )
    .expect("seed event 2");

    conn.execute(
        "INSERT INTO vectors_meta (
            vec_id, type, content, domain, category,
            source_event_ids, created_at
         ) VALUES (
            'turn_2', 'conversation_turn',
            'User: Paris là thủ đô của nước Pháp.\nAssistant: Và Tháp Eiffel nằm ở Paris.',
            'memory_owner:local', 'conversation:default',
            '[\"turn_2\"]', 2000
         )",
        [],
    )
    .expect("seed vector 2");

    // 1. Run consolidation batch
    let batch_result = process_pending_batch(&conn, &crypto, "test-worker", 10)
        .expect("process_pending_batch should succeed");

    assert_eq!(batch_result.processed, 2);
    assert_eq!(batch_result.consolidated, 2);
    assert!(
        batch_result.l3_triples_extracted >= 4,
        "Expected at least 4 triples, got {}",
        batch_result.l3_triples_extracted
    );

    // 2. Verify SQLite l3_nodes contains extracted entities
    let node_count: i64 = conn
        .query_row("SELECT count(*) FROM l3_nodes", [], |r| r.get(0))
        .expect("count l3_nodes");
    assert!(
        node_count >= 5,
        "Expected at least 5 nodes, found {node_count}"
    );

    let alice_exists: bool = conn
        .query_row(
            "SELECT count(*) FROM l3_nodes WHERE id = 'Alice'",
            [],
            |r| r.get::<_, i64>(0).map(|c| c > 0),
        )
        .expect("Alice exists");
    assert!(alice_exists, "Node 'Alice' must exist in l3_nodes");

    let google_exists: bool = conn
        .query_row(
            "SELECT count(*) FROM l3_nodes WHERE id = 'Google'",
            [],
            |r| r.get::<_, i64>(0).map(|c| c > 0),
        )
        .expect("Google exists");
    assert!(google_exists, "Node 'Google' must exist in l3_nodes");

    // 3. Verify SQLite l3_edges contains extracted relationships
    let edge_count: i64 = conn
        .query_row("SELECT count(*) FROM l3_edges", [], |r| r.get(0))
        .expect("count l3_edges");
    assert!(
        edge_count >= 4,
        "Expected at least 4 edges, found {edge_count}"
    );

    let works_at_rel: bool = conn
        .query_row(
            "SELECT count(*) FROM l3_edges WHERE source = 'Bob' AND target = 'Google' AND relation = 'works_at'",
            [],
            |r| r.get::<_, i64>(0).map(|c| c > 0),
        )
        .expect("Bob -> Google edge exists");
    assert!(works_at_rel, "Edge Bob -> Google (works_at) must exist");

    let friend_of_rel: bool = conn
        .query_row(
            "SELECT count(*) FROM l3_edges WHERE source = 'Alice' AND target = 'Bob' AND relation = 'friend_of'",
            [],
            |r| r.get::<_, i64>(0).map(|c| c > 0),
        )
        .expect("Alice -> Bob edge exists");
    assert!(friend_of_rel, "Edge Alice -> Bob (friend_of) must exist");
}

#[test]
fn test_multihop_hipporag_personalized_pagerank() {
    let pool = DatabasePool::new_in_memory().expect("in-memory db");
    let conn = pool.writer.get().expect("writer");

    // Populate a 3-hop knowledge chain:
    // Alice --friend_of--> Bob --works_at--> Google --subsidiary_of--> Alphabet
    conn.execute(
        "INSERT INTO l3_nodes (id, label, properties) VALUES
            ('Alice', 'Alice', '{}'),
            ('Bob', 'Bob', '{}'),
            ('Google', 'Google', '{}'),
            ('Alphabet', 'Alphabet', '{}')",
        [],
    )
    .expect("insert nodes");

    conn.execute(
        "INSERT INTO l3_edges (source, target, relation, weight, obsolete) VALUES
            ('Alice', 'Bob', 'friend_of', 1.0, 0),
            ('Bob', 'Google', 'works_at', 1.0, 0),
            ('Google', 'Alphabet', 'subsidiary_of', 1.0, 0)",
        [],
    )
    .expect("insert edges");

    // Load graph into In-Memory CsrGraph
    let graph = CsrGraph::from_db(&conn).expect("load CsrGraph from DB");
    assert_eq!(graph.node_count(), 4);

    // Multi-hop Query: Start seed at Alice only
    let seeds = [("Alice", 1.0f32)];
    let ppr_ranking = graph.personalized_pagerank_readonly(&seeds, 3, 0.85, 4);

    // Verify all nodes along the chain receive activation probability
    assert_eq!(ppr_ranking.len(), 4);

    let node_scores: std::collections::HashMap<String, f32> = ppr_ranking.into_iter().collect();

    // Verify all 4 nodes along the multi-hop chain are activated
    assert!(node_scores.contains_key("Alice") && node_scores["Alice"] > 0.0);
    assert!(node_scores.contains_key("Bob") && node_scores["Bob"] > 0.0);
    assert!(node_scores.contains_key("Google") && node_scores["Google"] > 0.0);
    assert!(node_scores.contains_key("Alphabet") && node_scores["Alphabet"] > 0.0);

    // Multi-hop propagation: the distant 3-hop node (Alphabet) is successfully discovered and activated
    assert!(
        node_scores["Alphabet"] > 0.05,
        "Alphabet should receive significant multi-hop probability mass"
    );
}

#[tokio::test]
async fn test_async_consolidation_updates_in_memory_csr_cache() {
    let pool = DatabasePool::new_in_memory().expect("in-memory db");
    let crypto = EncryptionEngine::new("test-consolidation-suite-key-32b");

    {
        let conn = pool.writer.get().expect("writer");
        conn.execute(
            "INSERT INTO events (
                eventId, timestamp, consolidated, domain, category,
                consolidation_status, retry_count, agentId
             ) VALUES (
                'event_async_1', 1000, 0, 'memory_owner:local',
                'conversation:default', 'pending', 0, 'liva_core'
             )",
            [],
        )
        .expect("seed event");

        conn.execute(
            "INSERT INTO vectors_meta (
                vec_id, type, content, domain, category,
                source_event_ids, created_at
             ) VALUES (
                'event_async_1', 'conversation_turn',
                'User: Paris là thủ đô của nước Pháp.\nAssistant: Đúng, và Tháp Eiffel nằm ở Paris.',
                'memory_owner:local', 'conversation:default',
                '[\"event_async_1\"]', 1000
             )",
            [],
        )
        .expect("seed vector");
    }

    // Before consolidation: graph in pool should be empty
    {
        let graph = pool.csr_graph.read().expect("read lock");
        assert_eq!(graph.node_count(), 0);
    }

    // Run async consumer once
    let res = consume_pending_once(&pool, &crypto, 10)
        .await
        .expect("consume pending once");
    assert_eq!(res.consolidated, 1);
    assert!(res.l3_triples_extracted > 0);

    // After consolidation: CsrGraph in pool must be automatically populated and compiled!
    {
        let graph = pool.csr_graph.read().expect("read lock");
        assert!(
            graph.node_count() >= 3,
            "Expected >= 3 nodes in CsrGraph cache, found {}",
            graph.node_count()
        );

        // Test seed matching on query
        let matched_seeds = graph.find_seed_nodes("Tháp Eiffel có ở đâu?", 3);
        assert!(
            !matched_seeds.is_empty(),
            "Expected seed matching for 'Tháp Eiffel'"
        );
        assert!(matched_seeds.iter().any(|(id, _)| id == "Tháp Eiffel"));

        // Multi-hop retrieval on compiled CsrGraph
        let seed_refs: Vec<(&str, f32)> = matched_seeds
            .iter()
            .map(|(id, w)| (id.as_str(), *w))
            .collect();
        let ppr = graph.personalized_pagerank_readonly(&seed_refs, 3, 0.85, 3);
        assert!(!ppr.is_empty());
        // Paris must be activated
        assert!(ppr.iter().any(|(id, _)| id == "Paris"));
    }
}

#[tokio::test]
async fn test_encrypted_conversation_turn_consolidation_populates_l3_graph() {
    let pool = DatabasePool::new_in_memory().expect("in-memory db");
    let crypto = EncryptionEngine::new("test-encryption-key-for-l3-32b");

    // Persist a conversation turn using the REAL encrypted persistence pipeline
    {
        let conn = pool.writer.get().expect("writer connection");
        persist_conversation_event_vector(
            &conn,
            &crypto,
            "turn_crypto_1",
            "User: Satoshi Nakamoto là tác giả của Bitcoin.\nAssistant: Đúng, và Bitcoin là tiền mã hóa.",
            &vec![0.1f32; MEMORY_VECTOR_DIM],
            "memory_owner:local",
            "conversation:default",
        )
        .expect("persist encrypted turn");

        // Verify that vectors_meta.content is actually encrypted with v2 prefix
        let stored_content: String = conn
            .query_row(
                "SELECT content FROM vectors_meta WHERE vec_id = 'turn_crypto_1'",
                [],
                |r| r.get(0),
            )
            .expect("query stored content");
        assert!(
            stored_content.starts_with("v2:"),
            "Turn content must be encrypted with v2 prefix in production"
        );
    }

    // Run consolidation with crypto engine
    let res = consume_pending_once(&pool, &crypto, 10)
        .await
        .expect("consume pending once");

    assert_eq!(res.consolidated, 1);
    assert!(
        res.l3_triples_extracted > 0,
        "Decryption must succeed so triples can be extracted from encrypted turns"
    );

    // Verify L3 nodes and edges were actually populated from the decrypted text
    let conn = pool.writer.get().expect("writer connection");
    let satoshi_exists: bool = conn
        .query_row(
            "SELECT count(*) FROM l3_nodes WHERE id = 'Satoshi Nakamoto'",
            [],
            |r| r.get::<_, i64>(0).map(|c| c > 0),
        )
        .expect("Satoshi exists");
    assert!(
        satoshi_exists,
        "Node 'Satoshi Nakamoto' must exist in l3_nodes"
    );
}
