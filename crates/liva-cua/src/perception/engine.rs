//! Unified Dual-Targeting Perception Engine with automatic visual ROI fallback.

use super::indexer::{SoMIndexer, SoMIndexerConfig};
use super::types::PerceptionResult;
use crate::traits::CuaDriverTrait;
use crate::types::{CuaError, CuaRect, WindowStateVerdict};
use crate::vision::traits::ScreenCapturerTrait;

/// Unified perception engine coordinating semantic hierarchy extraction and visual ROI fallback.
pub struct PerceptionEngine {
    indexer_config: SoMIndexerConfig,
}

impl Default for PerceptionEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl PerceptionEngine {
    /// Creates a new `PerceptionEngine` with default configuration.
    pub fn new() -> Self {
        Self {
            indexer_config: SoMIndexerConfig::default(),
        }
    }

    /// Creates a new `PerceptionEngine` with custom SoM indexer configuration.
    pub fn with_config(indexer_config: SoMIndexerConfig) -> Self {
        Self { indexer_config }
    }

    /// Perceives a target window by attempting semantic UI tree inspection first,
    /// automatically falling back to localized visual ROI capture when the semantic tree
    /// is empty or contains zero interactive elements.
    pub async fn perceive(
        &self,
        hwnd: u64,
        driver: &dyn CuaDriverTrait,
        capturer: Option<&dyn ScreenCapturerTrait>,
    ) -> Result<PerceptionResult, CuaError> {
        // Step 1: Query semantic UI tree from the driver
        let mut tree = driver.inspect_ui_tree(hwnd).await?;

        // Step 2: Index the tree to detect interactive controls
        let som_count = SoMIndexer::index_tree_with_config(&mut tree, &self.indexer_config);

        // Step 3: If interactive elements are found, return Semantic result
        if som_count > 0 {
            return Ok(PerceptionResult::Semantic { tree, som_count });
        }

        // Step 4: Semantic tree is empty/insufficient; trigger automatic visual ROI fallback
        let window_bounds = match driver.inspect_window(hwnd).await {
            Ok(WindowStateVerdict::Valid { bounds, .. }) => bounds,
            Ok(WindowStateVerdict::Minimized { bounds, .. }) => bounds,
            _ => CuaRect::default(),
        };

        let frame = if let Some(cap) = capturer {
            if !window_bounds.is_empty() && !window_bounds.is_minimized_sentinel() {
                cap.capture_roi(&window_bounds).await.ok()
            } else {
                cap.capture().await.ok()
            }
        } else {
            None
        };

        Ok(PerceptionResult::VisualFallback {
            window_bounds,
            frame,
            reason:
                "Zero interactive semantic elements found; falling back to visual ROI perception"
                    .to_string(),
        })
    }
}
