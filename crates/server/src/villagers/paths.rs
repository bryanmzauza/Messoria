//! Routes along the village's paths: from wherever a villager stands to
//! the nearest path, along the paths to the one nearest the destination,
//! and from there to it, passing the spots on the way in and out.

use std::collections::{BinaryHeap, HashMap};

use bevy::math::Vec2;
use messoria_content::Paths;

use messoria_shared::village;

/// The points to walk through from `from` to `to`: `out` first (the way
/// out of the spot the walk starts at), then the paths, then `via` (the
/// way in to the destination), then `to` itself.
pub(super) fn route(paths: &Paths, from: Vec2, out: &[Vec2], via: &[Vec2], to: Vec2) -> Vec<Vec2> {
    let mut points: Vec<Vec2> = out.to_vec();
    let start = points.last().copied().unwrap_or(from);
    let end = via.first().copied().unwrap_or(to);
    let nodes: Vec<Vec2> = paths
        .nodes
        .iter()
        .map(|&at| village::CENTER + Vec2::from(at))
        .collect();
    if let (Some(a), Some(b)) = (nearest(&nodes, start), nearest(&nodes, end)) {
        let along = shortest(paths, &nodes, a, b);
        // A path that only leads away from the destination is not worth
        // taking: walk straight there instead.
        let through = nodes[a].distance(start) + nodes[b].distance(end);
        if through < start.distance(end) + 1.0 || along.len() > 2 {
            points.extend(along.into_iter().map(|index| nodes[index]));
        }
    }
    points.extend_from_slice(via);
    points.push(to);
    points
}

fn nearest(nodes: &[Vec2], point: Vec2) -> Option<usize> {
    (0..nodes.len()).min_by(|&a, &b| {
        nodes[a]
            .distance_squared(point)
            .total_cmp(&nodes[b].distance_squared(point))
    })
}

/// The nodes along the shortest way from node `from` to node `to`, both
/// included; just `from` if nothing joins them.
fn shortest(paths: &Paths, nodes: &[Vec2], from: usize, to: usize) -> Vec<usize> {
    let mut neighbors: HashMap<usize, Vec<usize>> = HashMap::new();
    for &(a, b) in &paths.edges {
        neighbors.entry(a).or_default().push(b);
        neighbors.entry(b).or_default().push(a);
    }
    let mut best: HashMap<usize, f32> = HashMap::from([(from, 0.0)]);
    let mut came_from: HashMap<usize, usize> = HashMap::new();
    let mut open = BinaryHeap::from([Open {
        cost: nodes[from].distance(nodes[to]),
        node: from,
    }]);
    while let Some(Open { node, .. }) = open.pop() {
        if node == to {
            let mut way = vec![to];
            let mut at = to;
            while let Some(&previous) = came_from.get(&at) {
                way.push(previous);
                at = previous;
            }
            way.reverse();
            return way;
        }
        let so_far = best[&node];
        for &next in neighbors.get(&node).into_iter().flatten() {
            let cost = so_far + nodes[node].distance(nodes[next]);
            if best.get(&next).is_none_or(|&known| cost < known) {
                best.insert(next, cost);
                came_from.insert(next, node);
                open.push(Open {
                    cost: cost + nodes[next].distance(nodes[to]),
                    node: next,
                });
            }
        }
    }
    vec![from]
}

/// A node to look at, ordered so the heap gives the cheapest first.
#[derive(PartialEq)]
struct Open {
    cost: f32,
    node: usize,
}

impl Eq for Open {}

impl PartialOrd for Open {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Open {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        other.cost.total_cmp(&self.cost)
    }
}

#[cfg(test)]
mod tests {
    use messoria_shared::content::load_content;

    use super::*;

    #[test]
    fn routes_follow_the_streets_around_the_fountain() {
        let content = load_content().expect("the shipped content is valid");
        let paths = &content.village().paths;
        let north = village::CENTER + Vec2::new(0.0, -40.0);
        let south = village::CENTER + Vec2::new(0.0, 40.0);
        let way = route(paths, north, &[], &[], south);
        assert_eq!(
            way.first(),
            Some(&(village::CENTER + Vec2::new(0.0, -44.0)))
        );
        assert_eq!(way.last(), Some(&south));
        // Never through the fountain in the middle of the square.
        for pair in way.windows(2) {
            let (a, b) = (pair[0] - village::CENTER, pair[1] - village::CENTER);
            let t = (-a.dot(b - a) / (b - a).length_squared()).clamp(0.0, 1.0);
            let nearest = a + (b - a) * t;
            assert!(
                nearest.length() > 4.0,
                "the way passes {nearest} from the middle"
            );
        }
        assert!(way.len() >= 8, "the way has {} points", way.len());
    }

    #[test]
    fn the_way_in_and_out_passes_the_door() {
        let content = load_content().expect("the shipped content is valid");
        let paths = &content.village().paths;
        let door = village::CENTER + Vec2::new(13.8, -9.0);
        let counter = village::CENTER + Vec2::new(19.25, -9.55);
        let square = village::CENTER + Vec2::new(0.0, 9.0);
        let in_ = route(paths, square, &[], &[door], counter);
        assert_eq!(&in_[in_.len() - 2..], &[door, counter]);
        let out = route(paths, counter, &[door], &[], square);
        assert_eq!(out[0], door);
        assert_eq!(out.last(), Some(&square));
    }
}
