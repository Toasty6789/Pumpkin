#[cfg(feature = "codegen")]
use proc_macro2::TokenStream;
#[cfg(feature = "codegen")]
use quote::{ToTokens, quote};
use serde::Deserialize;

use super::int_provider::IntProvider;

#[derive(Deserialize, Clone, Debug)]
pub struct Experience {
    /// The experience points, represented as an `IntProvider`.
    pub experience: IntProvider,
}

#[cfg(feature = "codegen")]
impl ToTokens for Experience {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let experience = self.experience.to_token_stream();

        tokens.extend(quote! {
            Experience { experience: #experience }
        });
    }
}

/// Returns the number of points required to progress within a specific level.
///
/// # Arguments
/// * `level` – The level to calculate points for.
#[must_use]
pub const fn points_in_level(level: i32) -> i32 {
    match level {
        0..=15 => 2 * level + 7,
        16..=30 => 5 * level - 38,
        _ => 9 * level - 158,
    }
}

/// Calculates the total points required to reach a given level.
///
/// # Arguments
/// * `level` – The target level.
#[must_use]
pub fn points_to_level(level: i32) -> i32 {
    match level {
        0..=16 => level * level + 6 * level,
        17..=31 => {
            (2.5f64.mul_add(f64::from(level * level), -(40.5 * f64::from(level))) + 360.0) as i32
        }
        _ => {
            (4.5f64.mul_add(f64::from(level * level), -(162.5 * f64::from(level))) + 2220.0) as i32
        }
    }
}

/// Converts total experience points into a level and points within that level.
///
/// # Arguments
/// * `total_points` – The total accumulated experience points.
///
/// # Returns
/// A tuple `(level, points_into_level)` representing the current level and
/// remaining points within that level.
#[must_use]
pub fn total_to_level_and_points(total_points: i32) -> (i32, i32) {
    let level = match total_points {
        0..=352 => ((f64::from(total_points) + 9.0).sqrt() - 3.0) as i32,
        353..=1507 => (8.1 + (0.4 * (f64::from(total_points) - (7839.0 / 40.0))).sqrt()) as i32,
        _ => {
            ((325.0 / 18.0) + (2.0 / 9.0 * (f64::from(total_points) - (54215.0 / 72.0))).sqrt())
                as i32
        }
    };
    let level_start = points_to_level(level);
    let points_into_level = total_points - level_start;

    (level, points_into_level)
}

/// Calculates the progress within a level as a value between 0.0 and 1.0.
///
/// # Arguments
/// * `points` – The points accumulated in the current level.
/// * `level` – The current level.
#[must_use]
pub fn progress_in_level(points: i32, level: i32) -> f32 {
    let max_points = points_in_level(level);
    let progress = (points as f32) / (max_points as f32);

    progress.clamp(0.0, 1.0)
}

/// Adds experience points to a (level, points) pair, carrying across level boundaries.
///
/// Instead of converting to total XP (which can overflow `i32`), this uses incremental
/// carry logic matching vanilla's `giveExperiencePoints`: when the points in the current
/// level exceed the threshold, the surplus carries to the next level.
///
/// # Arguments
/// * `level` – The current level.
/// * `points` – Points already earned in the current level (must be < `points_in_level(level)`).
/// * `added_points` – The number of additional experience points to add (must be >= 0).
///
/// # Returns
/// A tuple `(new_level, new_points)` with the updated level and points-in-level.
#[must_use]
pub fn add_points_to_level(level: i32, points: i32, added_points: i32) -> (i32, i32) {
    // Use i64 internally to avoid overflow when points + added_points > i32::MAX
    let mut remaining: i64 = (points as i64) + (added_points as i64);
    let mut new_level: i64 = level as i64;

    // Carry upward: level up when points exceed what's needed for the current level
    while remaining >= points_in_level(new_level as i32) as i64 && new_level < i64::from(i32::MAX) {
        remaining -= points_in_level(new_level as i32) as i64;
        new_level += 1;
    }

    // Safety cap at extreme levels
    if new_level >= i64::from(i32::MAX) {
        remaining = 0;
        new_level = i64::from(i32::MAX) - 1;
    }

    // Clamp negative remaining (shouldn't happen with non-negative added_points)
    if remaining < 0 {
        remaining = 0;
    }

    // Convert back to i32, saturating at reasonable bounds
    let final_level = if new_level > i32::MAX as i64 {
        i32::MAX - 1
    } else {
        new_level as i32
    };
    let final_points = if remaining > i32::MAX as i64 {
        i32::MAX
    } else {
        remaining as i32
    };

    (final_level, final_points)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_points_within_current_level() {
        // Level 0 needs 7 points. Start at 3 points, add 2 more = 5 points.
        let (lvl, pts) = add_points_to_level(0, 3, 2);
        assert_eq!(lvl, 0, "should stay at level 0");
        assert_eq!(pts, 5, "5 points in level 0");
    }

    #[test]
    fn add_points_carries_to_next_level() {
        // Level 0 needs 7 points. Start at 5 points, add 5 = 10.
        // 5 + 5 = 10, 10 >= 7 → level up: 10 - 7 = 3 points at level 1.
        let (lvl, pts) = add_points_to_level(0, 5, 5);
        assert_eq!(lvl, 1, "should carry to level 1");
        assert_eq!(pts, 3, "3 points remaining at level 1");
    }

    #[test]
    fn add_points_carries_multiple_levels() {
        // Level 0: 7 pts, Level 1: 9 pts, Level 2: 11 pts
        // Start at level 0 with 0 points, add 30.
        // 30 >= 7 → level 1, 23 remaining
        // 23 >= 9 → level 2, 14 remaining
        // 14 >= 11 → level 3, 3 remaining
        let (lvl, pts) = add_points_to_level(0, 0, 30);
        assert_eq!(lvl, 3, "30 points should reach level 3");
        assert_eq!(pts, 3, "3 points into level 3");
    }

    #[test]
    fn add_points_exact_boundary_at_level() {
        // Level 0 needs 7 points. Start at 0, add exactly 7 = level up with 0 remaining.
        let (lvl, pts) = add_points_to_level(0, 0, 7);
        assert_eq!(lvl, 1, "exact fill should level up");
        assert_eq!(pts, 0, "0 points remaining");
    }

    #[test]
    fn add_large_amount_does_not_overflow() {
        // Adding i32::MAX points starting from level 0 should still produce
        // a valid result without overflow/wrapping.
        let (lvl, pts) = add_points_to_level(0, 0, i32::MAX);
        // Level should be positive and sane (around 21863 for i32::MAX total XP)
        assert!(lvl > 0, "level should be positive");
        assert!(lvl <= 100_000, "level should be within practical bounds");
        assert!(pts >= 0, "points should be non-negative");
        assert!(
            pts < points_in_level(lvl),
            "points should be within the level"
        );
    }

    #[test]
    fn add_points_zero_is_noop() {
        let (lvl, pts) = add_points_to_level(5, 3, 0);
        assert_eq!(lvl, 5);
        assert_eq!(pts, 3);
    }

    #[test]
    fn total_consistency_after_addition() {
        // After adding points, the total XP (points_to_level(level) + points)
        // should equal the original total + added_points.
        let (orig_level, orig_points) = (10, 5);
        let original_total = points_to_level(orig_level) as i64 + orig_points as i64;
        let added: i32 = 500;
        let (new_level, new_points) = add_points_to_level(orig_level, orig_points, added);
        let new_total = points_to_level(new_level) as i64 + new_points as i64;
        assert_eq!(
            new_total,
            original_total + added as i64,
            "total XP should increase by exactly added_points"
        );
    }

    #[test]
    fn points_never_exceed_level_threshold() {
        // For any valid starting state, the result should always have
        // points < points_in_level(level).
        for level in 0..50 {
            let max_pts = points_in_level(level);
            if max_pts <= 0 {
                continue;
            }
            for pts in 0..max_pts {
                for added in [0, 1, 5, 50, 500, i32::MAX] {
                    let (new_lvl, new_pts) = add_points_to_level(level, pts, added);
                    if new_lvl < 100_000 {
                        assert!(
                            new_pts < points_in_level(new_lvl),
                            "points {} should be < max {} for level {} (start: {}+{})",
                            new_pts,
                            points_in_level(new_lvl),
                            new_lvl,
                            level,
                            added
                        );
                    }
                }
            }
        }
    }
}
