use bvh_region::TrivialHeuristic;
use evenio::{
    entity::EntityId,
    event::Receiver,
    fetch::{Fetcher, Single},
    query::{Query, With},
};
use tracing::instrument;

use crate::{
    components::{FullEntityPose, Player},
    event::Gametick,
    singleton::player_aabb_lookup::{LookupData, PlayerBoundingBoxes},
};

#[derive(Query, Debug)]
pub(crate) struct EntityQuery<'a> {
    id: EntityId,
    pose: &'a FullEntityPose,
    _player: With<&'static Player>,
}

#[instrument(skip_all, level = "trace")]
pub fn rebuild_player_location(
    _: Receiver<Gametick>,
    mut lookup: Single<&mut PlayerBoundingBoxes>,
    entities: Fetcher<EntityQuery>,
) {
    let elements: Vec<_> = entities
        .iter()
        .map(|query| LookupData {
            id: query.id,
            aabb: query.pose.bounding,
        })
        .collect();

    let bvh = bvh_region::Bvh::build::<TrivialHeuristic>(elements);

    lookup.query = bvh;
}
