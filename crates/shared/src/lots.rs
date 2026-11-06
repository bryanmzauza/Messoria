//! Farm lots as every peer knows them: where they lie and who may work
//! them, and their fences in the way of whoever walks.
//!
//! The lots and their fences are laid out by the valley
//! (`messoria_worldgen::lots`), the same for every world. Who owns a lot is
//! a replicated `LotClaim`, one per claimed lot, which the server keeps and
//! saves. Ground is worked (dug, tilled, planted, built on) only on a lot by
//! its owner or the players they invited; outside the lots nobody works the
//! ground. Fences become obstacles as the columns they stand on load, on
//! the server and on every client alike, so movement collides with them
//! the same way everywhere.

use bevy::{ecs::system::SystemParam, prelude::*};
use messoria_worldgen::column_of;
pub use messoria_worldgen::{
    FENCE_SECTION, FenceRun, GATE_WIDTH, LOT_COUNT, LOT_SIZE, Lot, LotId, fence_distance,
    gate_distance, homestead_distance, lot, lot_at, lots,
};

use crate::{
    obstacles::{Blocker, Obstacles},
    protocol::{LotClaim, Notice},
    valley::{ColumnLoaded, ColumnUnloaded, Valley},
};

/// How thick a fence is to walk against, and how tall.
const FENCE_THICKNESS: f32 = 0.2;
pub const FENCE_HEIGHT: f32 = 1.2;
/// How far below the ground a fence reaches, so a dip under it does not
/// let anyone through.
const FENCE_FOOTING: f32 = 0.5;
/// Radius of a gatepost, and its height.
const GATEPOST_RADIUS: f32 = 0.18;
pub const GATEPOST_HEIGHT: f32 = 2.8;
/// Ground kept on each side of a fence, which nobody digs or tills.
pub const FENCE_GROUND: f32 = 0.6;

pub(crate) struct LotsPlugin;

impl Plugin for LotsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            PreUpdate,
            (clear_fences, raise_fences)
                .chain()
                .after(crate::terrain::LoadColumns),
        );
    }
}

/// The claimed lots, for telling who may work where.
#[derive(SystemParam)]
pub struct Claims<'w, 's> {
    claims: Query<'w, 's, &'static LotClaim>,
}

impl Claims<'_, '_> {
    /// The claim on `lot`, if someone owns it.
    pub fn on(&self, lot: LotId) -> Option<&LotClaim> {
        self.claims.iter().find(|claim| claim.lot == lot)
    }

    /// Every claimed lot.
    pub fn iter(&self) -> impl Iterator<Item = &LotClaim> {
        self.claims.iter()
    }

    /// How many lots the player `key` names owns.
    pub fn owned_by(&self, key: &str) -> usize {
        self.claims
            .iter()
            .filter(|claim| claim.owner == key)
            .count()
    }

    /// The lot the ground at `point` may be worked on by the player `key`
    /// names.
    ///
    /// # Errors
    ///
    /// The notice saying why it may not: the ground is on no lot or on an
    /// unclaimed one, or the lot is somebody else's.
    pub fn may_work(&self, key: &str, point: Vec2) -> Result<LotId, Notice> {
        let lot = lot_at(point).ok_or(Notice::OnlyOnYourLot)?;
        match self.on(lot) {
            Some(claim) if claim.allows(key) => Ok(lot),
            Some(_) => Err(Notice::NotYourLot),
            None => Err(Notice::OnlyOnYourLot),
        }
    }

    /// Whether the player `key` names may work the ground at `point`.
    pub fn allows(&self, key: &str, point: Vec2) -> bool {
        self.may_work(key, point).is_ok()
    }
}

/// Whether a disc of `radius` around `point` reaches the ground a fence
/// keeps.
pub fn fence_keeps(point: Vec2, radius: f32) -> bool {
    fence_distance(point) < FENCE_GROUND + radius
}

/// The fence sections whose middles lie in `column`, as the point each
/// starts at and the direction it runs in, and the gateposts there.
pub fn fences_in_column(column: IVec2) -> (Vec<(Vec2, Vec2)>, Vec<Vec2>) {
    #[expect(clippy::cast_precision_loss, reason = "the chunk size is small")]
    let size = messoria_voxel::CHUNK_SIZE as f32;
    let low = column.as_vec2() * size;
    let high = low + Vec2::splat(size);
    // A lot reaching the column, with room for a section sticking in.
    let near = |lot: &Lot| {
        lot.max.cmpge(low - FENCE_SECTION).all() && lot.min.cmple(high + FENCE_SECTION).all()
    };
    let mut sections = Vec::new();
    let mut posts = Vec::new();
    for lot in lots().filter(near) {
        for run in lot.fences() {
            for (start, direction) in run.sections() {
                if column_of(start + direction * (FENCE_SECTION / 2.0)) == column {
                    sections.push((start, direction));
                }
            }
        }
        posts.extend(
            lot.gateposts()
                .into_iter()
                .filter(|&post| column_of(post) == column),
        );
    }
    (sections, posts)
}

/// Puts the fences of columns that loaded in the way.
fn raise_fences(
    mut loaded: MessageReader<ColumnLoaded>,
    valley: Option<Res<Valley>>,
    mut obstacles: ResMut<Obstacles>,
) {
    let Some(valley) = valley else {
        loaded.clear();
        return;
    };
    for &ColumnLoaded(column) in loaded.read() {
        let owner = Blocker::Fences(column);
        let (sections, posts) = fences_in_column(column);
        for (start, direction) in sections {
            let end = start + direction * FENCE_SECTION;
            let (from, to) = (valley.height(start), valley.height(end));
            obstacles.block_with_fence(
                owner,
                start,
                end,
                FENCE_THICKNESS,
                from.min(to) - FENCE_FOOTING,
                from.max(to) + FENCE_HEIGHT,
            );
        }
        for post in posts {
            let ground = valley.height(post);
            obstacles.block_with_post(
                owner,
                post,
                GATEPOST_RADIUS,
                ground - FENCE_FOOTING,
                ground + GATEPOST_HEIGHT,
            );
        }
    }
}

fn clear_fences(mut unloaded: MessageReader<ColumnUnloaded>, mut obstacles: ResMut<Obstacles>) {
    for &ColumnUnloaded(column) in unloaded.read() {
        obstacles.clear(Blocker::Fences(column));
    }
}

#[cfg(test)]
mod tests {
    use bevy::ecs::system::SystemState;

    use super::*;

    #[test]
    fn every_section_of_fence_belongs_to_one_column() {
        let lot = lot(LotId(3));
        let expected: usize = lot.fences().iter().map(|run| run.sections().count()).sum();
        let (low, high) = (
            column_of(lot.min) - IVec2::ONE,
            column_of(lot.max) + IVec2::ONE,
        );
        let mut found = 0;
        let mut posts = 0;
        for z in low.y..=high.y {
            for x in low.x..=high.x {
                let (sections, gateposts) = fences_in_column(IVec2::new(x, z));
                found += sections
                    .iter()
                    .filter(|(start, _)| lot.distance(*start) < 0.01)
                    .count();
                posts += gateposts
                    .iter()
                    .filter(|post| lot.gate.distance(**post) < GATE_WIDTH)
                    .count();
            }
        }
        assert_eq!(found, expected);
        assert_eq!(posts, 2);
    }

    #[test]
    fn lots_are_worked_by_their_owners_and_guests_only() {
        let mut app = App::new();
        app.world_mut().spawn(LotClaim {
            lot: LotId(2),
            owner: "alice".to_owned(),
            invited: vec!["bob".to_owned()],
        });
        let inside = lot(LotId(2)).center();
        let unclaimed = lot(LotId(3)).center();
        let mut state: SystemState<Claims> = SystemState::new(app.world_mut());
        let claims = state.get(app.world()).expect("a plain query");
        assert_eq!(claims.may_work("alice", inside), Ok(LotId(2)));
        assert_eq!(claims.may_work("bob", inside), Ok(LotId(2)));
        assert_eq!(claims.may_work("carol", inside), Err(Notice::NotYourLot));
        assert_eq!(
            claims.may_work("alice", unclaimed),
            Err(Notice::OnlyOnYourLot)
        );
        assert_eq!(
            claims.may_work("alice", Vec2::ZERO),
            Err(Notice::OnlyOnYourLot)
        );
        assert_eq!(claims.owned_by("alice"), 1);
        assert!(fence_keeps(lot(LotId(2)).min, 0.0));
        assert!(!fence_keeps(inside, 1.5));
    }
}
