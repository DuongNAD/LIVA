//! Deterministic Set-of-Mark (SoM) Indexer with reading-order sorting and noise filtering.

use super::types::UiTree;

/// Configuration options for Set-of-Mark (SoM) element indexing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SoMIndexerConfig {
    /// Vertical tolerance in pixels to consider two adjacent controls on the same horizontal row band.
    pub band_tolerance_px: i32,
    /// Minimum width and height for a control to receive a mark.
    pub min_dimension: i32,
    /// Whether to force inclusion of static text labels.
    pub include_static_text: bool,
}

impl Default for SoMIndexerConfig {
    fn default() -> Self {
        Self {
            band_tolerance_px: 8,
            min_dimension: 2,
            include_static_text: false,
        }
    }
}

/// Set-of-Mark (SoM) Indexer assigning monotonic numeric marks [1..N] to interactive UI elements.
pub struct SoMIndexer;

impl SoMIndexer {
    /// Indexes the UI tree with default configuration, assigning monotonic marks [1..N].
    /// Returns the number of interactive elements marked.
    pub fn index_tree(tree: &mut UiTree) -> usize {
        Self::index_tree_with_config(tree, &SoMIndexerConfig::default())
    }

    /// Indexes the UI tree using custom configuration parameters.
    pub fn index_tree_with_config(tree: &mut UiTree, config: &SoMIndexerConfig) -> usize {
        // Step 1: Clear all previous SoM indices across the entire tree
        tree.clear_som_indices();

        // Step 2: Collect all candidate element IDs and spatial coordinates with unique preorder indices
        struct Candidate {
            preorder_idx: usize,
            id: String,
            x: i32,
            y: i32,
        }

        let mut candidates: Vec<Candidate> = Vec::new();
        let mut preorder_counter: usize = 0;

        fn collect_candidates(
            elem: &crate::perception::UiElement,
            config: &SoMIndexerConfig,
            counter: &mut usize,
            candidates: &mut Vec<Candidate>,
        ) {
            let current_idx = *counter;
            *counter += 1;

            // Check noise filtering
            let skip_candidate = !elem.state.is_visible
                || !elem.state.is_enabled
                || elem.bounds.width < config.min_dimension
                || elem.bounds.height < config.min_dimension
                || elem.bounds.is_minimized_sentinel()
                || elem.bounds.x <= -10000
                || elem.bounds.y <= -10000
                || (elem.role.is_container()
                    && !elem.state.is_clickable
                    && !elem.state.is_checkable);

            if !skip_candidate {
                let is_interactive = elem.is_interactive()
                    || (config.include_static_text && elem.role == crate::perception::UiRole::Text);

                if is_interactive {
                    candidates.push(Candidate {
                        preorder_idx: current_idx,
                        id: elem.id.clone(),
                        x: elem.bounds.x,
                        y: elem.bounds.y,
                    });
                }
            }

            for child in &elem.children {
                collect_candidates(child, config, counter, candidates);
            }
        }

        collect_candidates(&tree.root, config, &mut preorder_counter, &mut candidates);

        if candidates.is_empty() {
            tree.recount();
            return 0;
        }

        // Step 3: Deterministic Reading-Order Sorting (top-to-bottom, left-to-right with row banding)
        // Sort initially by top (y) ascending, then left (x) ascending, tie-breaking by preorder_idx
        candidates.sort_by(|a, b| {
            a.y.cmp(&b.y)
                .then_with(|| a.x.cmp(&b.x))
                .then_with(|| a.preorder_idx.cmp(&b.preorder_idx))
        });

        // Group into horizontal row bands (cast coordinates to i64 to prevent subtraction overflow)
        let mut bands: Vec<Vec<Candidate>> = Vec::new();
        for item in candidates {
            if let Some(current_band) = bands.last_mut() {
                let band_anchor_y = current_band[0].y;
                let delta_y = (item.y as i64 - band_anchor_y as i64).abs();
                if delta_y <= config.band_tolerance_px as i64 {
                    current_band.push(item);
                    continue;
                }
            }
            // Start a new row band
            bands.push(vec![item]);
        }

        // Within each row band, sort strictly left-to-right (x ascending), tie-breaking by y, then id, then preorder_idx
        let mut ordered_candidates: Vec<Candidate> = Vec::new();
        for mut band in bands {
            band.sort_by(|a, b| {
                a.x.cmp(&b.x)
                    .then_with(|| a.y.cmp(&b.y))
                    .then_with(|| a.id.cmp(&b.id))
                    .then_with(|| a.preorder_idx.cmp(&b.preorder_idx))
            });
            ordered_candidates.extend(band);
        }

        // Step 4: Assign monotonic 1-based indices [1..N] via preorder index mapping
        use std::collections::HashMap;

        let mut mark_map: HashMap<usize, usize> = HashMap::with_capacity(ordered_candidates.len());
        for (idx, cand) in ordered_candidates.into_iter().enumerate() {
            let mark = idx + 1;
            mark_map.insert(cand.preorder_idx, mark);
        }

        let mut assign_counter: usize = 0;
        tree.root.for_each_mut(&mut |node| {
            if let Some(&mark) = mark_map.get(&assign_counter) {
                node.som_index = Some(mark);
            }
            assign_counter += 1;
        });

        tree.recount();
        tree.interactive_count
    }

    /// Formats a human-readable and token-efficient summary table of all marked elements.
    pub fn format_som_summary(tree: &UiTree) -> String {
        tree.format_som_prompt()
    }
}
