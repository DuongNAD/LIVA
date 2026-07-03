pub mod capture;
pub mod diff;

use capture::{ScreenCapturer, Frame};
use diff::{ScreenRegion, RegionDiffResult, DiffEngine};
use std::sync::Arc;
use serde::{Serialize, Deserialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VisionConfig {
    pub color_tolerance: u8,
    pub max_regions: usize,
}

impl Default for VisionConfig {
    fn default() -> Self {
        Self {
            color_tolerance: 5,
            max_regions: 64,
        }
    }
}

pub struct VisionManager {
    capturer: Arc<dyn ScreenCapturer>,
    regions: Vec<ScreenRegion>,
    last_frame: Option<Frame>,
    config: VisionConfig,
}

impl VisionManager {
    pub fn new(capturer: Arc<dyn ScreenCapturer>, config: VisionConfig) -> Self {
        Self {
            capturer,
            regions: Vec::new(),
            last_frame: None,
            config,
        }
    }

    pub fn set_config(&mut self, config: VisionConfig) {
        self.config = config;
    }

    pub fn add_region(&mut self, region: ScreenRegion) -> Result<(), String> {
        if self.regions.len() >= self.config.max_regions {
            return Err(format!("Max region limit ({}) reached", self.config.max_regions));
        }
        if region.width == 0 || region.height == 0 {
            return Err("Region width and height must be greater than zero".to_string());
        }
        if region.threshold.is_nan() || region.threshold < 0.0 || region.threshold > 1.0 {
            return Err("Region threshold must be a valid float between 0.0 and 1.0".to_string());
        }
        if self.regions.iter().any(|r| r.id == region.id) {
            return Err(format!("Region with ID '{}' already registered", region.id));
        }
        self.regions.push(region);
        Ok(())
    }

    pub fn remove_region(&mut self, id: &str) -> Result<(), String> {
        let idx = self.regions.iter().position(|r| r.id == id)
            .ok_or_else(|| format!("Region with ID '{}' not found", id))?;
        self.regions.remove(idx);
        Ok(())
    }

    pub fn list_regions(&self) -> &[ScreenRegion] {
        &self.regions
    }

    pub fn capturer(&self) -> Arc<dyn ScreenCapturer> {
        self.capturer.clone()
    }

    pub fn last_frame(&self) -> Option<Frame> {
        self.last_frame.clone()
    }

    pub fn update_last_frame(&mut self, frame: Frame) {
        self.last_frame = Some(frame);
    }

    pub fn regions(&self) -> Vec<ScreenRegion> {
        self.regions.clone()
    }

    pub fn color_tolerance(&self) -> u8 {
        self.config.color_tolerance
    }

    pub fn capture_screen(&mut self) -> Result<Frame, String> {
        let frame = self.capturer.capture().map_err(|e| e.to_string())?;
        self.last_frame = Some(frame.clone());
        Ok(frame)
    }

    pub fn detect_changes(&mut self) -> Result<Vec<RegionDiffResult>, String> {
        let current_frame = self.capturer.capture().map_err(|e| e.to_string())?;
        let results = self.detect_changes_against_frame(&current_frame)?;
        self.last_frame = Some(current_frame);
        Ok(results)
    }

    pub fn detect_changes_against_frame(&self, current_frame: &Frame) -> Result<Vec<RegionDiffResult>, String> {
        let prev_frame = match &self.last_frame {
            Some(f) => f,
            None => {
                // Return baseline showing total change on first frame
                return Ok(self.regions.iter().map(|r| RegionDiffResult {
                    region_id: r.id.clone(),
                    name: r.name.clone(),
                    difference: 1.0,
                    is_changed: true,
                }).collect());
            }
        };

        let mut results = Vec::with_capacity(self.regions.len());
        for region in &self.regions {
            let res = DiffEngine::diff_region(
                prev_frame,
                current_frame,
                region,
                self.config.color_tolerance,
            )?;
            results.push(res);
        }
        Ok(results)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vision::capture::MockScreenCapturer;
    use crate::vision::capture::PixelFormat;

    fn make_mgr() -> VisionManager {
        let capturer = Arc::new(MockScreenCapturer::new(10, 10, PixelFormat::Rgb));
        let config = VisionConfig {
            color_tolerance: 5,
            max_regions: 2,
        };
        VisionManager::new(capturer, config)
    }

    #[test]
    fn test_manager_add_and_remove_regions() {
        let mut mgr = make_mgr();
        
        let r1 = ScreenRegion {
            id: "r1".to_string(),
            name: "Region 1".to_string(),
            x: 0, y: 0, width: 2, height: 2,
            threshold: 0.1,
        };
        
        assert!(mgr.add_region(r1.clone()).is_ok());
        // Duplicate detection
        assert!(mgr.add_region(r1.clone()).is_err());
        
        assert_eq!(mgr.list_regions().len(), 1);
        assert!(mgr.remove_region("r1").is_ok());
        assert_eq!(mgr.list_regions().len(), 0);
    }

    #[test]
    fn test_manager_region_limits() {
        let mut mgr = make_mgr(); // Max regions set to 2
        let r1 = ScreenRegion { id: "r1".to_string(), name: "1".to_string(), x: 0, y: 0, width: 1, height: 1, threshold: 0.1 };
        let r2 = ScreenRegion { id: "r2".to_string(), name: "2".to_string(), x: 0, y: 0, width: 1, height: 1, threshold: 0.1 };
        let r3 = ScreenRegion { id: "r3".to_string(), name: "3".to_string(), x: 0, y: 0, width: 1, height: 1, threshold: 0.1 };

        assert!(mgr.add_region(r1).is_ok());
        assert!(mgr.add_region(r2).is_ok());
        // Capacity limit violation
        assert!(mgr.add_region(r3).is_err());
    }

    #[test]
    fn test_manager_first_capture_behavior() {
        let mut mgr = make_mgr();
        let r1 = ScreenRegion {
            id: "r1".to_string(),
            name: "Region 1".to_string(),
            x: 0, y: 0, width: 5, height: 5,
            threshold: 0.1,
        };
        mgr.add_region(r1).unwrap();

        // Detect changes with last_frame = None
        let diffs = mgr.detect_changes().unwrap();
        assert_eq!(diffs.len(), 1);
        assert_eq!(diffs[0].difference, 1.0); // Baseline trigger
        assert!(diffs[0].is_changed);
    }

    #[test]
    fn test_manager_add_region_invalid_threshold() {
        let mut mgr = make_mgr();
        
        let r_nan = ScreenRegion {
            id: "r_nan".to_string(),
            name: "NaN threshold".to_string(),
            x: 0, y: 0, width: 2, height: 2,
            threshold: f32::NAN,
        };
        assert!(mgr.add_region(r_nan).is_err());

        let r_neg = ScreenRegion {
            id: "r_neg".to_string(),
            name: "Negative threshold".to_string(),
            x: 0, y: 0, width: 2, height: 2,
            threshold: -0.1,
        };
        assert!(mgr.add_region(r_neg).is_err());

        let r_large = ScreenRegion {
            id: "r_large".to_string(),
            name: "Too large threshold".to_string(),
            x: 0, y: 0, width: 2, height: 2,
            threshold: 1.1,
        };
        assert!(mgr.add_region(r_large).is_err());
    }
}
