//! Field-of-view computation, tile visibility, and awareness system.
//!
//! Provides symmetric shadowcasting for computing which tiles a player can see,
//! a three-state visibility model (Unexplored / Remembered / Visible), and an
//! awareness framework that can be extended with additional sensory sources
//! (e.g. sound) in the future.

use crate::ecs::{EntityID, EntityMap, Point};
use bitcode::{Decode, Encode};
use rustc_hash::{FxHashMap, FxHashSet};
use std::collections::HashSet;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Default field-of-view radius in tiles.
pub const DEFAULT_FOV_RADIUS: i32 = 12;

/// Extra margin (in tiles) beyond the FOV radius used when filtering entities
/// for network transmission. Avoids visual popping at FOV edges.
pub const FOV_NETWORK_MARGIN: i32 = 2;

// ---------------------------------------------------------------------------
// Tile visibility
// ---------------------------------------------------------------------------

/// Three-state visibility for a single tile from one player's perspective.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
pub enum TileVisibility {
    /// Never been seen by this player.
    Unexplored,
    /// Previously visible but currently outside FOV.
    Remembered,
    /// Currently within the player's FOV.
    Visible,
}

impl Default for TileVisibility {
    fn default() -> Self {
        Self::Unexplored
    }
}

// ---------------------------------------------------------------------------
// Visibility grid  (per-player, persistent)
// ---------------------------------------------------------------------------

/// Per-player persistent visibility state.
///
/// Tiles promote Unexplored → Remembered → Visible but never go back to
/// Unexplored. When a tile leaves FOV it is demoted from Visible → Remembered.
#[derive(Debug, Clone, Default, Encode, Decode)]
pub struct VisibilityGrid {
    tiles: FxHashMap<(i32, i32), TileVisibility>,
}

impl VisibilityGrid {
    /// Returns the visibility state of the given tile.
    pub fn get(&self, pos: Point) -> TileVisibility {
        self.tiles
            .get(&(pos.x, pos.y))
            .copied()
            .unwrap_or(TileVisibility::Unexplored)
    }

    /// Update the grid after a new FOV calculation.
    ///
    /// * Tiles in `fov_set` become [`TileVisibility::Visible`].
    /// * Tiles previously `Visible` but no longer in the set become
    ///   [`TileVisibility::Remembered`].
    /// * Everything else stays as-is.
    pub fn update(&mut self, fov_set: &FxHashSet<(i32, i32)>) {
        // Demote previously visible tiles that are no longer in FOV.
        // Iteration order doesn't matter — we only mutate values, not keys.
        #[expect(clippy::iter_over_hash_type, reason = "order-independent mutation")]
        for vis in self.tiles.values_mut() {
            if *vis == TileVisibility::Visible {
                *vis = TileVisibility::Remembered;
            }
        }

        // Promote tiles that are currently in FOV.
        #[expect(clippy::iter_over_hash_type, reason = "order-independent insertion")]
        for &pos in fov_set {
            self.tiles.insert(pos, TileVisibility::Visible);
        }
    }
}

// ---------------------------------------------------------------------------
// Awareness system  (extensible for future sensory sources)
// ---------------------------------------------------------------------------

/// Describes *how* a player became aware of an entity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
pub enum AwarenessSource {
    /// Full visual awareness from FOV — render the entity normally.
    Sight,
    /// Future: auditory awareness — render as directional indicator only.
    Sound,
}

/// An entity the player is aware of, along with how they learned about it.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct AwareEntity {
    pub entity_id: EntityID,
    pub position: Point,
    pub source: AwarenessSource,
}

/// Build the awareness set for a single player.
///
/// Currently only FOV (sight) feeds into awareness. The design allows adding
/// additional sources (e.g. `Sound`) without changing the FOV system.
pub fn build_awareness(
    fov_set: &FxHashSet<(i32, i32)>,
    entities: &EntityMap,
    margin: i32,
    player_pos: Point,
) -> Vec<AwareEntity> {
    let mut aware = Vec::new();
    #[expect(clippy::iter_over_hash_type, reason = "order not significant for awareness set")]
    for (&eid, entity) in entities {
        let p = entity.position;
        // Check if entity is within FOV + margin.
        let dx = (p.x - player_pos.x).abs();
        let dy = (p.y - player_pos.y).abs();
        let in_fov = fov_set.contains(&(p.x, p.y));
        let in_margin = dx <= DEFAULT_FOV_RADIUS + margin
            && dy <= DEFAULT_FOV_RADIUS + margin;
        if in_fov || in_margin {
            aware.push(AwareEntity {
                entity_id: eid,
                position: p,
                source: AwarenessSource::Sight,
            });
        }
    }
    aware
}

// ---------------------------------------------------------------------------
// Symmetric shadowcasting — FOV computation
// ---------------------------------------------------------------------------

/// Compute which tiles are visible from `origin` within `radius`, considering
/// opaque tiles determined by `is_opaque`.
///
/// Returns a set of `(x, y)` coordinates that are visible. The origin itself
/// is always visible. Opaque tiles (e.g. trees) are themselves visible but
/// block sight to tiles behind them.
pub fn compute_fov<F>(origin: Point, radius: i32, is_opaque: F) -> FxHashSet<(i32, i32)>
where
    F: Fn(i32, i32) -> bool,
{
    let mut visible: FxHashSet<(i32, i32)> = FxHashSet::default();
    visible.insert((origin.x, origin.y));

    // Process all eight octants.
    for octant in 0..8 {
        let params = ShadowcastParams {
            ox: origin.x,
            oy: origin.y,
            radius,
            row: 1,
            start_slope: 1.0,
            end_slope: 0.0,
            octant,
        };
        cast_light(&mut visible, &is_opaque, params);
    }

    visible
}

/// Parameters for a single recursive shadowcast invocation.
#[derive(Clone, Copy)]
struct ShadowcastParams {
    ox: i32,
    oy: i32,
    radius: i32,
    row: i32,
    start_slope: f64,
    end_slope: f64,
    octant: u8,
}

/// Recursive shadowcasting for a single octant.
///
/// Uses the standard recursive approach where each octant maps rows/columns
/// via a transformation function.
fn cast_light<F>(
    visible: &mut FxHashSet<(i32, i32)>,
    is_opaque: &F,
    params: ShadowcastParams,
) where
    F: Fn(i32, i32) -> bool,
{
    let ShadowcastParams {
        ox,
        oy,
        radius,
        row,
        mut start_slope,
        end_slope,
        octant,
    } = params;

    if start_slope < end_slope || row > radius {
        return;
    }

    let mut prev_blocked = false;
    let mut next_start_slope = start_slope;

    for j in row..=radius {
        let dy = -j;
        let mut blocked = false;

        let col_min = ((-j as f64) * start_slope + 0.5).round() as i32;

        // Walk columns from most-negative to zero.
        let mut dx = col_min;
        while dx <= 0 {
            // Transform octant-local (dx, dy) into world coordinates.
            let (mx, my) = transform_octant(dx, dy, octant);
            let wx = ox + mx;
            let wy = oy + my;

            let l_slope = (dx as f64 - 0.5) / (dy as f64 + 0.5);
            let r_slope = (dx as f64 + 0.5) / (dy as f64 - 0.5);

            if start_slope < r_slope {
                dx += 1;
                continue;
            }
            if end_slope > l_slope {
                break;
            }

            // Check if within circular radius.
            let dist_sq = dx * dx + dy * dy;
            if dist_sq <= radius * radius {
                visible.insert((wx, wy));
            }

            if prev_blocked {
                if is_opaque(wx, wy) {
                    next_start_slope = r_slope;
                    dx += 1;
                    continue;
                } else {
                    prev_blocked = false;
                    start_slope = next_start_slope;
                }
            } else if is_opaque(wx, wy) && j < radius {
                blocked = true;
                cast_light(
                    visible,
                    is_opaque,
                    ShadowcastParams {
                        ox,
                        oy,
                        radius,
                        row: j + 1,
                        start_slope,
                        end_slope: l_slope,
                        octant,
                    },
                );
                next_start_slope = r_slope;
            }

            dx += 1;
        }

        if blocked {
            break;
        }
        prev_blocked = blocked;
    }
}

/// Map octant-local `(col, row)` offsets into world `(dx, dy)`.
fn transform_octant(col: i32, row: i32, octant: u8) -> (i32, i32) {
    match octant {
        0 => (col, row),
        1 => (row, col),
        2 => (row, -col),
        3 => (col, -row),
        4 => (-col, -row),
        5 => (-row, -col),
        6 => (-row, col),
        _ => (-col, row),
    }
}

// ---------------------------------------------------------------------------
// Helper: build the opaque set from entity map
// ---------------------------------------------------------------------------

/// Collect positions of all entities that block line of sight.
pub fn opaque_positions(entities: &EntityMap) -> HashSet<(i32, i32)> {
    entities
        .values()
        .filter(|e| e.entity_type.blocks_sight())
        .map(|e| (e.position.x, e.position.y))
        .collect()
}

/// Compute FOV for a player at `origin` using the entity map to determine
/// which tiles are opaque.
pub fn compute_fov_from_entities(
    origin: Point,
    radius: i32,
    entities: &EntityMap,
) -> FxHashSet<(i32, i32)> {
    let opaque = opaque_positions(entities);
    compute_fov(origin, radius, |x, y| opaque.contains(&(x, y)))
}

// ---------------------------------------------------------------------------
// Per-player FOV state
// ---------------------------------------------------------------------------

/// Per-player FOV state that persists across ticks.
#[derive(Debug, Clone, Default, Encode, Decode)]
pub struct PlayerFov {
    pub visibility: VisibilityGrid,
    pub current_fov: FxHashSet<(i32, i32)>,
    pub fov_radius: i32,
}

impl PlayerFov {
    pub fn new(radius: i32) -> Self {
        Self {
            visibility: VisibilityGrid::default(),
            current_fov: FxHashSet::default(),
            fov_radius: radius,
        }
    }

    /// Recompute FOV from the given origin and update the visibility grid.
    pub fn recompute(&mut self, origin: Point, entities: &EntityMap) {
        self.current_fov = compute_fov_from_entities(origin, self.fov_radius, entities);
        self.visibility.update(&self.current_fov);
    }
}

/// Map from player entity ID to their FOV state.
pub type PlayerFovMap = FxHashMap<EntityID, PlayerFov>;

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ecs::{Entity, EntityType};

    #[test]
    fn origin_always_visible() {
        let origin = Point { x: 5, y: 5 };
        let fov = compute_fov(origin, 10, |_, _| false);
        assert!(
            fov.contains(&(5, 5)),
            "origin should always be visible"
        );
    }

    #[test]
    fn fov_respects_radius() {
        let origin = Point { x: 0, y: 0 };
        let fov = compute_fov(origin, 3, |_, _| false);
        // A tile 4 steps away on an axis should not be visible.
        assert!(
            !fov.contains(&(4, 0)),
            "tile beyond radius should not be visible"
        );
        // A tile 3 steps away on an axis should be visible.
        assert!(
            fov.contains(&(3, 0)),
            "tile at radius boundary should be visible"
        );
    }

    #[test]
    fn opaque_tile_visible_but_blocks_behind() {
        // Place an opaque wall at (2, 0). It should be visible, but (3, 0)
        // should not be.
        let origin = Point { x: 0, y: 0 };
        let fov = compute_fov(origin, 10, |x, y| x == 2 && y == 0);
        assert!(
            fov.contains(&(2, 0)),
            "opaque tile itself should be visible"
        );
        assert!(
            !fov.contains(&(3, 0)),
            "tile behind opaque should be blocked"
        );
    }

    #[test]
    fn visibility_grid_promotes_and_demotes() {
        let mut grid = VisibilityGrid::default();
        let pos = Point { x: 1, y: 2 };

        // Initially unexplored.
        assert_eq!(grid.get(pos), TileVisibility::Unexplored);

        // Tile enters FOV → Visible.
        let mut fov = FxHashSet::default();
        fov.insert((1, 2));
        grid.update(&fov);
        assert_eq!(grid.get(pos), TileVisibility::Visible);

        // Tile leaves FOV → Remembered.
        let empty_fov = FxHashSet::default();
        grid.update(&empty_fov);
        assert_eq!(grid.get(pos), TileVisibility::Remembered);

        // Tile never goes back to Unexplored.
        grid.update(&empty_fov);
        assert_eq!(grid.get(pos), TileVisibility::Remembered);
    }

    #[test]
    fn compute_fov_from_entities_blocks_trees() {
        let mut entities = EntityMap::default();
        // Place a tree at (3, 0).
        entities.insert(
            EntityID(1),
            Entity {
                position: Point { x: 3, y: 0 },
                name: None,
                entity_type: EntityType::Tree,
            },
        );

        let origin = Point { x: 0, y: 0 };
        let fov = compute_fov_from_entities(origin, 10, &entities);

        assert!(fov.contains(&(3, 0)), "tree tile should be visible");
        assert!(!fov.contains(&(4, 0)), "tile behind tree should be blocked");
    }

    #[test]
    fn player_fov_recompute_updates_visibility() {
        let entities = EntityMap::default();
        let mut pfov = PlayerFov::new(5);

        let origin = Point { x: 0, y: 0 };
        pfov.recompute(origin, &entities);

        assert_eq!(
            pfov.visibility.get(Point { x: 0, y: 0 }),
            TileVisibility::Visible
        );
        assert!(pfov.current_fov.contains(&(0, 0)));
    }

    #[test]
    fn awareness_includes_entities_in_fov() {
        let mut entities = EntityMap::default();
        entities.insert(
            EntityID(1),
            Entity {
                position: Point { x: 1, y: 0 },
                name: None,
                entity_type: EntityType::Player,
            },
        );

        let mut fov = FxHashSet::default();
        fov.insert((1, 0));

        let aware = build_awareness(&fov, &entities, 0, Point { x: 0, y: 0 });
        assert_eq!(aware.len(), 1);
        assert_eq!(aware.first().expect("non-empty").entity_id, EntityID(1));
        assert_eq!(aware.first().expect("non-empty").source, AwarenessSource::Sight);
    }
}
