//! Semantic Entity-Relation Triple Extractor for L3 Knowledge Graph.
//!
//! Extracts structured knowledge triples `(Subject, Relation, Object, Weight)` from
//! conversation turns and notes without calling external LLM APIs, enabling zero-token
//! background consolidation into `l3_nodes` and `l3_edges`.

use std::collections::HashSet;

/// An extracted knowledge triple representing an edge in the L3 graph.
#[derive(Debug, Clone, PartialEq)]
pub struct ExtractedTriple {
    pub subject: String,
    pub relation: String,
    pub object: String,
    pub weight: f32,
}

/// Extracts knowledge triples from a block of text (conversation turn or note).
pub fn extract_triples(text: &str) -> Vec<ExtractedTriple> {
    let mut triples = Vec::new();
    let mut seen = HashSet::new();

    // 1. Extract Obsidian Wikilinks `[[Subject]] ... [[Object]]`
    extract_wikilink_triples(text, &mut triples, &mut seen);

    // 2. Extract pattern-based factual sentences (Vietnamese & English)
    for line in text.lines() {
        let trimmed_line = line.trim();
        if trimmed_line.is_empty() {
            continue;
        }

        // Clean user/assistant prefixes if present
        let clean_line = strip_role_prefix(trimmed_line);

        // Split sentences and clauses by '.', '!', '?', ';', ',', '\n'
        for sentence in clean_line.split(['.', '!', '?', ';', ',', '\n']) {
            let s = sentence.trim();
            if s.len() < 5 {
                continue;
            }
            extract_sentence_triples(s, &mut triples, &mut seen);
        }
    }

    triples
}

fn strip_role_prefix(line: &str) -> &str {
    if let Some((prefix, rest)) = line.split_once(':') {
        let p_lower = prefix.trim().to_lowercase();
        if matches!(
            p_lower.as_str(),
            "user" | "người dùng" | "assistant" | "liva" | "trợ lý"
        ) {
            return rest.trim();
        }
    }
    line
}

fn extract_wikilink_triples(
    text: &str,
    triples: &mut Vec<ExtractedTriple>,
    seen: &mut HashSet<(String, String, String)>,
) {
    let mut links = Vec::new();
    let mut cursor = text;
    while let Some(start) = cursor.find("[[") {
        if let Some(end) = cursor[start + 2..].find("]]") {
            let raw_link = &cursor[start + 2..start + 2 + end];
            // Split alias if present: [[Page|Alias]]
            let page_title = raw_link.split('|').next().unwrap_or(raw_link).trim();
            if !page_title.is_empty() && page_title.len() <= 80 {
                links.push(clean_entity(page_title));
            }
            cursor = &cursor[start + 2 + end + 2..];
        } else {
            break;
        }
    }

    // Connect adjacent wikilinks in the same paragraph
    for window in links.windows(2) {
        let (s, o) = (&window[0], &window[1]);
        if s != o {
            let key = (s.clone(), "links_to".to_string(), o.clone());
            if seen.insert(key) {
                triples.push(ExtractedTriple {
                    subject: s.clone(),
                    relation: "links_to".to_string(),
                    object: o.clone(),
                    weight: 1.0,
                });
            }
        }
    }
}

struct RelationPattern {
    keywords: &'static [&'static str],
    relation: &'static str,
}

const RELATION_PATTERNS: &[RelationPattern] = &[
    RelationPattern {
        keywords: &[
            " là bạn của ",
            " là bạn thân của ",
            " is a friend of ",
            " is friends with ",
        ],
        relation: "friend_of",
    },
    RelationPattern {
        keywords: &[
            " làm việc tại ",
            " làm việc ở ",
            " công tác tại ",
            " works at ",
            " works for ",
        ],
        relation: "works_at",
    },
    RelationPattern {
        keywords: &[
            " nằm ở ",
            " ở tại ",
            " sống ở ",
            " sống tại ",
            " toạ lạc tại ",
            " is located in ",
            " lives in ",
        ],
        relation: "located_in",
    },
    RelationPattern {
        keywords: &[" là thủ đô của ", " là thủ đô nước ", " is the capital of "],
        relation: "capital_of",
    },
    RelationPattern {
        keywords: &[
            " thuộc về ",
            " là một phần của ",
            " belongs to ",
            " is part of ",
        ],
        relation: "part_of",
    },
    RelationPattern {
        keywords: &[" thích ", " yêu thích ", " đam mê ", " likes ", " loves "],
        relation: "likes",
    },
    RelationPattern {
        keywords: &[
            " tạo ra ",
            " phát triển ",
            " sáng lập ",
            " created ",
            " founded ",
            " developed ",
        ],
        relation: "created",
    },
    RelationPattern {
        keywords: &[" là ", " is a ", " is an ", " is "],
        relation: "is_a",
    },
];

fn extract_sentence_triples(
    sentence: &str,
    triples: &mut Vec<ExtractedTriple>,
    seen: &mut HashSet<(String, String, String)>,
) {
    let s_lower = sentence.to_lowercase();

    for pat in RELATION_PATTERNS {
        for &kw in pat.keywords {
            if let Some(pos) = s_lower.find(kw) {
                let raw_sub = &sentence[..pos];
                let raw_obj = &sentence[pos + kw.len()..];

                let sub = clean_entity(raw_sub);
                let obj = clean_entity(raw_obj);

                if is_valid_entity(&sub)
                    && is_valid_entity(&obj)
                    && sub.to_lowercase() != obj.to_lowercase()
                {
                    let key = (sub.clone(), pat.relation.to_string(), obj.clone());
                    if seen.insert(key) {
                        triples.push(ExtractedTriple {
                            subject: sub,
                            relation: pat.relation.to_string(),
                            object: obj,
                            weight: 1.0,
                        });
                        return; // Match highest-precedence relation per clause
                    }
                }
            }
        }
    }
}

fn clean_entity(raw: &str) -> String {
    let mut cleaned = raw.trim();

    // If there is a leading clause separated by comma, take the entity after comma
    if let Some(pos) = cleaned.rfind(',') {
        let after = cleaned[pos + 1..].trim();
        if after.len() >= 2 {
            cleaned = after;
        }
    }

    // Strip common leading conversational phrases and conjunctions in a loop
    let prefixes = [
        "đúng vậy",
        "chính xác",
        "vâng",
        "ừ",
        "tất nhiên",
        "và ",
        "còn ",
        "nhưng ",
        "mà ",
        "and ",
        "but ",
        "tôi nghĩ rằng",
        "tôi biết rằng",
        "thực ra",
        "bạn có biết",
        "theo như tôi biết",
        "nói chung",
        "thực tế",
        "i think that",
        "actually",
        "as you know",
        "in fact",
        "yes",
        "yeah",
        "sure",
        "right",
        "well",
    ];

    loop {
        let mut matched = false;
        let lower = cleaned.to_lowercase();
        for p in prefixes {
            if lower.starts_with(p) {
                cleaned = cleaned[p.len()..].trim();
                matched = true;
                break;
            }
        }
        if !matched {
            break;
        }
    }

    // Strip surrounding punctuation & brackets
    cleaned = cleaned
        .trim_matches(|c: char| {
            c == '"'
                || c == '\''
                || c == '`'
                || c == '('
                || c == ')'
                || c == '['
                || c == ']'
                || c == ','
                || c == ':'
        })
        .trim();

    cleaned.to_string()
}

fn is_valid_entity(entity: &str) -> bool {
    let len = entity.chars().count();
    if !(2..=80).contains(&len) {
        return false;
    }

    // Filter out generic pronoun stops
    let stops = [
        "tôi",
        "tao",
        "mình",
        "bạn",
        "cậu",
        "anh",
        "chị",
        "em",
        "chúng tôi",
        "chúng nó",
        "nó",
        "họ",
        "đây",
        "đó",
        "kia",
        "gì",
        "ai",
        "nào",
        "sao",
        "i",
        "me",
        "you",
        "he",
        "she",
        "it",
        "we",
        "they",
        "this",
        "that",
        "what",
        "who",
    ];
    let lower = entity.to_lowercase();
    if stops.contains(&lower.as_str()) {
        return false;
    }

    // Must have at least one alphanumeric character
    entity.chars().any(|c| c.is_alphanumeric())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wikilink_extraction() {
        let text = "Hãy tham khảo [[Machine Learning]] và xem liên kết tới [[Deep Learning]].";
        let triples = extract_triples(text);
        assert_eq!(triples.len(), 1);
        assert_eq!(triples[0].subject, "Machine Learning");
        assert_eq!(triples[0].relation, "links_to");
        assert_eq!(triples[0].object, "Deep Learning");
    }

    #[test]
    fn test_factual_sentence_extraction_vietnamese() {
        let text = "Paris là thủ đô của nước Pháp. Tháp Eiffel nằm ở Paris.";
        let triples = extract_triples(text);
        assert!(triples.iter().any(|t| t.subject == "Paris"
            && t.relation == "capital_of"
            && t.object == "nước Pháp"));
        assert!(triples.iter().any(|t| t.subject == "Tháp Eiffel"
            && t.relation == "located_in"
            && t.object == "Paris"));
    }

    #[test]
    fn test_friend_and_work_extraction() {
        let text = "Alice là bạn của Bob. Bob làm việc tại Google.";
        let triples = extract_triples(text);
        assert!(
            triples
                .iter()
                .any(|t| t.subject == "Alice" && t.relation == "friend_of" && t.object == "Bob")
        );
        assert!(
            triples
                .iter()
                .any(|t| t.subject == "Bob" && t.relation == "works_at" && t.object == "Google")
        );
    }

    #[test]
    fn test_role_prefix_stripping() {
        let text = "User: Alice là bạn của Bob.\nAssistant: Đúng vậy, Bob làm việc tại Google.";
        let triples = extract_triples(text);
        assert!(
            triples
                .iter()
                .any(|t| t.subject == "Alice" && t.object == "Bob")
        );
        assert!(
            triples
                .iter()
                .any(|t| t.subject == "Bob" && t.object == "Google")
        );
    }

    #[test]
    fn test_pronoun_suppression() {
        let text = "Tôi là một người bình thường.";
        let triples = extract_triples(text);
        // "Tôi" is a stop pronoun, should be filtered out
        assert!(triples.is_empty());
    }
}
