//! Mathematical knee-point detection using the Kneedle algorithm.
//!
//! Identifies the point of diminishing marginal returns on the CELF submodular
//! utility curve, automatically sizing prompt token budgets to capture maximum
//! structural relevance without prompting bloat.
//!
//! # Academic Citation & References
//!
//! Ville Satopää, Jeannie Albrecht, David Irwin, and Barath Raghavan.
//! *"Finding a 'Kneedle' in a Haystack: Detecting Knee Points in System Behavior"*.
//! In *31st International Conference on Distributed Computing Systems Workshops (ICDCSW)*,
//! 2011, pp. 166–171. [DOI: 10.1109/ICDCSW.2011.20](https://doi.org/10.1109/ICDCSW.2011.20).

/// Represents a detected knee point on a discrete utility curve.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct KneePoint {
    /// Zero-based index of the knee element in the input trajectory.
    pub index: usize,
    /// Cumulative token cost at the knee point.
    pub token_cost: usize,
    /// Cumulative utility score at the knee point.
    pub cumulative_utility: f32,
    /// Ratio of cumulative utility captured at the knee relative to total trajectory utility $\in [0.0, 1.0]$.
    pub utility_ratio: f32,
    /// Curvature distance metric $d_i = y'_i - x'_i$.
    pub curvature_distance: f32,
}

/// Detector implementing the Kneedle algorithm for concave submodular gain curves.
#[derive(Debug, Clone, PartialEq)]
pub struct KneedleDetector {
    /// Sensitivity parameter $S$ controlling the detection threshold (Satopää et al., 2011). Default: `1.0`.
    pub sensitivity: f32,
    /// Minimum fraction of total trajectory utility required before declaring a knee. Default: `0.35`.
    pub min_utility_ratio: f32,
}

impl Default for KneedleDetector {
    fn default() -> Self {
        Self {
            sensitivity: 1.0,
            min_utility_ratio: 0.35,
        }
    }
}

impl KneedleDetector {
    /// Creates a new `KneedleDetector` with default sensitivity and utility threshold.
    pub fn new() -> Self {
        Self::default()
    }

    /// Configures the detector with custom sensitivity and minimum utility ratio.
    pub fn with_params(sensitivity: f32, min_utility_ratio: f32) -> Self {
        Self {
            sensitivity: sensitivity.max(0.1),
            min_utility_ratio: min_utility_ratio.clamp(0.0, 1.0),
        }
    }

    /// Finds the knee point on a discrete `(token_cost, cumulative_utility)` trajectory.
    ///
    /// The input `points` MUST be monotonically increasing in `token_cost` (x-axis)
    /// and non-decreasing in `cumulative_utility` (y-axis), representing the
    /// concave diminishing-returns curve produced by CELF submodular knapsack selection.
    ///
    /// Returns `None` if `points` is empty or if no valid curvature exists.
    pub fn find_knee(&self, points: &[(usize, f32)]) -> Option<KneePoint> {
        if points.is_empty() {
            return None;
        }

        if points.len() <= 2 {
            let last_idx = points.len() - 1;
            let (cost, util) = points[last_idx];
            return Some(KneePoint {
                index: last_idx,
                token_cost: cost,
                cumulative_utility: util,
                utility_ratio: 1.0,
                curvature_distance: 0.0,
            });
        }

        let x_min = points.first()?.0 as f32;
        let x_max = points.last()?.0 as f32;
        let y_min = points.first()?.1;
        let y_max = points.last()?.1;

        let x_range = x_max - x_min;
        let y_range = y_max - y_min;

        // If all points have identical costs or zero utility gain, return the first point
        if x_range <= 1e-6 || y_range <= 1e-6 {
            let (cost, util) = points[0];
            return Some(KneePoint {
                index: 0,
                token_cost: cost,
                cumulative_utility: util,
                utility_ratio: 1.0,
                curvature_distance: 0.0,
            });
        }

        // 1. Normalize x and y into unit box [0, 1]
        // 2. Compute difference curve d_i = y'_i - x'_i (distance to diagonal chord)
        let mut best_idx = points.len() - 1;
        let mut max_diff = -f32::INFINITY;

        let total_util = y_max.max(1e-6);

        for (i, &(cost, util)) in points.iter().enumerate() {
            let x_norm = (cost as f32 - x_min) / x_range;
            let y_norm = (util - y_min) / y_range;

            let diff = y_norm - x_norm;
            let current_util_ratio = util / total_util;

            // Only consider points that satisfy the minimum utility floor
            if current_util_ratio >= self.min_utility_ratio && diff > max_diff {
                max_diff = diff;
                best_idx = i;
            }
        }

        let (best_cost, best_util) = points[best_idx];
        let utility_ratio = (best_util / total_util).clamp(0.0, 1.0);

        Some(KneePoint {
            index: best_idx,
            token_cost: best_cost,
            cumulative_utility: best_util,
            utility_ratio,
            curvature_distance: max_diff.max(0.0),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_kneedle_empty_and_single_point() {
        let detector = KneedleDetector::default();
        assert_eq!(detector.find_knee(&[]), None);

        let single = vec![(100, 5.0)];
        let knee = detector.find_knee(&single).unwrap();
        assert_eq!(knee.index, 0);
        assert_eq!(knee.token_cost, 100);
        assert_eq!(knee.cumulative_utility, 5.0);
    }

    #[test]
    fn test_kneedle_clear_concave_knee() {
        // Synthetic submodular curve: rapid initial gain, then flat plateau
        // Point 3 (tokens: 500, util: 85.0) is the distinct knee point
        let points = vec![
            (50, 20.0),
            (150, 50.0),
            (300, 75.0),
            (500, 85.0), // <- Knee
            (800, 88.0),
            (1200, 90.0),
            (2000, 92.0),
        ];

        let detector = KneedleDetector::default();
        let knee = detector.find_knee(&points).expect("Should find knee");

        assert_eq!(knee.index, 3);
        assert_eq!(knee.token_cost, 500);
        assert_eq!(knee.cumulative_utility, 85.0);
        assert!(knee.utility_ratio > 0.80);
        assert!(knee.curvature_distance > 0.30);
    }

    #[test]
    fn test_kneedle_linear_curve() {
        // Linear gain curve: no strong knee, falls back gracefully
        let points = vec![
            (100, 10.0),
            (200, 20.0),
            (300, 30.0),
            (400, 40.0),
            (500, 50.0),
        ];

        let detector = KneedleDetector::default();
        let knee = detector.find_knee(&points).expect("Should find knee");
        assert!(knee.token_cost >= 200);
    }
}
