//! Host-side geometry utilities that do not depend on retail Skate 3 data.
//!
//! The rail finder is adapted from the proven arbitrary-map detector used by
//! chasmlol/2010-rust-rewrite-mashup, but uses SkyCraft's Y-up coordinate space
//! and metre-ish block units.

pub use glam::Vec3;
use std::collections::{HashMap, HashSet};

pub const STAIR_HELPER_FLAG: u32 = 1 << 0;
pub const GHOST_FLAG: u32 = 1 << 2;

/// A face this upward is eligible to contribute grindable lips.
pub const UPWARD_Y: f32 = 0.65;
/// Horizontal grid size used to accelerate short collision probes.
pub const GRID_CELL: f32 = 2.0;
/// Distance outside a candidate lip to inspect, in SkyCraft blocks (~metres).
pub const PROBE_OUT: f32 = 0.06;
/// Ground must fall at least this far past the lip.
pub const MIN_DROP: f32 = 0.12;
/// Height above the lip used to reject an outside wall.
pub const WALL_PROBE_UP: f32 = 0.12;
/// Smallest rail retained after merging/chaining.
pub const MIN_RAIL: f32 = 0.55;
/// Maximum rise/run accepted for an edge.
pub const MAX_SLOPE: f32 = 0.70;
/// Chained rail segments may turn by roughly 35 degrees.
pub const MIN_TURN_COS: f32 = 0.82;

#[derive(Clone, Copy, Debug)]
pub struct Triangle {
    pub p: [Vec3; 3],
    pub flags: u32,
}

impl Triangle {
    pub fn finite(self) -> bool {
        self.p.iter().all(|v| v.is_finite())
            && (self.p[1] - self.p[0])
                .cross(self.p[2] - self.p[0])
                .length_squared()
                > 1.0e-10
    }
}

#[derive(Clone, Debug, Default)]
pub struct RailCensus {
    pub input_triangles: usize,
    pub walkable_edges: usize,
    pub lips: usize,
    pub runs: usize,
    pub rails: usize,
}

#[derive(Clone, Debug, Default)]
pub struct RailResult {
    pub rails: Vec<Vec<Vec3>>,
    pub census: RailCensus,
}

/// Infer grind rails from arbitrary solid collision.
///
/// Ghost triangles are excluded because SkyCraft uses them only to describe the
/// original surface behind dug-away geometry; they are not collision.
pub fn find_rails(source: &[Triangle]) -> RailResult {
    // SkyCraft regions intentionally overlap slightly, so the same Havok face can
    // be present in neighbouring region messages. Deduplicate before edge probing or
    // those copies can look like tiny seams/extra rail candidates.
    let mut seen = HashSet::new();
    let mut retained = Vec::new();
    for tri in source
        .iter()
        .copied()
        .filter(|t| t.flags & GHOST_FLAG == 0 && t.finite())
    {
        let mut key = tri
            .p
            .map(|v| v.to_array().map(|x| (x * 1000.0).round() as i32));
        key.sort();
        if seen.insert(key) {
            retained.push(tri);
        }
    }
    let tris: Vec<[Vec3; 3]> = retained.iter().map(|t| t.p).collect();

    let grid = Grid::build(&tris);
    let mut probe = Probe {
        tris: &tris,
        grid: &grid,
        stamp: vec![0; tris.len()],
        round: 0,
        scratch: Vec::new(),
    };

    let key = |v: Vec3| v.to_array().map(|x| (x * 1000.0).round() as i32);
    let mut edges: HashMap<([i32; 3], [i32; 3]), (Vec3, Vec3, Vec3, Vec3)> = HashMap::new();

    for (meta, tri) in retained.iter().zip(&tris) {
        // Skyrim's invisible stair helpers are useful as smooth contact geometry,
        // but their artificial side edges must never become visible grind rails.
        if meta.flags & STAIR_HELPER_FLAG != 0 {
            continue;
        }
        let normal = (tri[1] - tri[0]).cross(tri[2] - tri[0]).normalize_or_zero();
        if normal.y <= UPWARD_Y {
            continue;
        }
        let centroid = (tri[0] + tri[1] + tri[2]) / 3.0;
        for i in 0..3 {
            let (a, b) = (tri[i], tri[(i + 1) % 3]);
            let (ka, kb) = (key(a), key(b));
            let k = if ka < kb { (ka, kb) } else { (kb, ka) };
            edges.entry(k).or_insert((a, b, normal, centroid));
        }
    }

    let walkable_edges = edges.len();
    let mut lips = Vec::new();
    for (a, b, normal, centroid) in edges.into_values() {
        let along = b - a;
        let len = along.length();
        if len < 0.02 || along.y.abs() > MAX_SLOPE * len {
            continue;
        }

        let mid = (a + b) * 0.5;
        let mut out = along.cross(normal).normalize_or_zero();
        if out.dot(centroid - mid) > 0.0 {
            out = -out;
        }

        let samples: &[f32] = if len < 0.30 {
            &[0.5]
        } else {
            &[0.25, 0.5, 0.75]
        };
        let passing = samples
            .iter()
            .filter(|&&s| probe.is_lip(a + along * s, out))
            .count();

        if passing * 3 >= samples.len() * 2 {
            lips.push((a, b));
        }
    }

    let lip_count = lips.len();
    let runs = merge_collinear(lips);
    let run_count = runs.len();
    let mut rails = chain(runs);
    rails.retain(|r| polyline_len(r) >= MIN_RAIL);

    RailResult {
        census: RailCensus {
            input_triangles: tris.len(),
            walkable_edges,
            lips: lip_count,
            runs: run_count,
            rails: rails.len(),
        },
        rails,
    }
}

fn polyline_len(points: &[Vec3]) -> f32 {
    points.windows(2).map(|w| w[0].distance(w[1])).sum()
}

fn cell_of(v: f32) -> i32 {
    (v / GRID_CELL).floor() as i32
}

struct Grid {
    cells: HashMap<(i32, i32), Vec<u32>>,
}

impl Grid {
    fn build(tris: &[[Vec3; 3]]) -> Self {
        let mut cells: HashMap<(i32, i32), Vec<u32>> = HashMap::new();
        for (index, tri) in tris.iter().enumerate() {
            let min = tri[0].min(tri[1]).min(tri[2]);
            let max = tri[0].max(tri[1]).max(tri[2]);
            for x in cell_of(min.x)..=cell_of(max.x) {
                for z in cell_of(min.z)..=cell_of(max.z) {
                    cells.entry((x, z)).or_default().push(index as u32);
                }
            }
        }
        Self { cells }
    }
}

struct Probe<'a> {
    tris: &'a [[Vec3; 3]],
    grid: &'a Grid,
    stamp: Vec<u32>,
    round: u32,
    scratch: Vec<u32>,
}

impl Probe<'_> {
    fn is_lip(&mut self, p: Vec3, out: Vec3) -> bool {
        // If ground exists immediately outside within MIN_DROP, this is only a
        // tessellation/interior edge rather than a lip.
        let down_from = p + out * PROBE_OUT + Vec3::Y * 0.02;
        if self.hit(down_from, -Vec3::Y, MIN_DROP + 0.02) {
            return false;
        }

        // Reject an edge when solid geometry rises just outside it.
        let across_from = p - out * 0.02 + Vec3::Y * WALL_PROBE_UP;
        !self.hit(across_from, out, PROBE_OUT + 0.04)
    }

    fn hit(&mut self, origin: Vec3, dir: Vec3, len: f32) -> bool {
        let end = origin + dir * len;
        let min = origin.min(end);
        let max = origin.max(end);

        self.round = self.round.wrapping_add(1);
        if self.round == 0 {
            self.stamp.fill(0);
            self.round = 1;
        }
        self.scratch.clear();

        for x in cell_of(min.x)..=cell_of(max.x) {
            for z in cell_of(min.z)..=cell_of(max.z) {
                let Some(cell) = self.grid.cells.get(&(x, z)) else {
                    continue;
                };
                for &index in cell {
                    let seen = &mut self.stamp[index as usize];
                    if *seen != self.round {
                        *seen = self.round;
                        self.scratch.push(index);
                    }
                }
            }
        }

        self.scratch
            .iter()
            .any(|&i| ray_triangle(origin, dir, len, &self.tris[i as usize]))
    }
}

/// Möller-Trumbore segment/triangle test, both faces.
fn ray_triangle(origin: Vec3, dir: Vec3, len: f32, tri: &[Vec3; 3]) -> bool {
    let e1 = tri[1] - tri[0];
    let e2 = tri[2] - tri[0];
    let p = dir.cross(e2);
    let det = e1.dot(p);
    if det.abs() < 1.0e-8 {
        return false;
    }
    let inv = 1.0 / det;
    let s = origin - tri[0];
    let u = s.dot(p) * inv;
    if !(0.0..=1.0).contains(&u) {
        return false;
    }
    let q = s.cross(e1);
    let v = dir.dot(q) * inv;
    if v < 0.0 || u + v > 1.0 {
        return false;
    }
    let t = e2.dot(q) * inv;
    t > 1.0e-5 && t <= len
}

fn merge_collinear(lips: Vec<(Vec3, Vec3)>) -> Vec<(Vec3, Vec3)> {
    let mut lines: HashMap<([i32; 3], [i32; 3]), (Vec3, Vec3, Vec<(f32, f32)>)> =
        HashMap::new();

    for (mut a, mut b) in lips {
        let mut d = (b - a).normalize_or_zero();
        if d == Vec3::ZERO {
            continue;
        }

        let flip = d.x < -1.0e-4
            || (d.x.abs() <= 1.0e-4
                && (d.z < -1.0e-4 || (d.z.abs() <= 1.0e-4 && d.y < 0.0)));
        if flip {
            d = -d;
            std::mem::swap(&mut a, &mut b);
        }

        let o = a - d * a.dot(d);
        let k = (
            (d * 1000.0).to_array().map(|x| x.round() as i32),
            (o * 100.0).to_array().map(|x| x.round() as i32),
        );
        let line = lines.entry(k).or_insert((d, o, Vec::new()));
        let mut span = (a.dot(line.0), b.dot(line.0));
        if span.1 < span.0 {
            std::mem::swap(&mut span.0, &mut span.1);
        }
        line.2.push(span);
    }

    let mut runs = Vec::new();
    for (d, o, mut spans) in lines.into_values() {
        spans.sort_by(|x, y| x.0.total_cmp(&y.0));
        let Some((&first, rest)) = spans.split_first() else {
            continue;
        };
        let mut current = first;
        for &(t0, t1) in rest {
            if t0 <= current.1 + 0.03 {
                current.1 = current.1.max(t1);
            } else {
                runs.push((o + d * current.0, o + d * current.1));
                current = (t0, t1);
            }
        }
        runs.push((o + d * current.0, o + d * current.1));
    }
    runs
}

fn chain(runs: Vec<(Vec3, Vec3)>) -> Vec<Vec<Vec3>> {
    let node = |v: Vec3| v.to_array().map(|x| (x * 100.0).round() as i32);
    let mut at: HashMap<[i32; 3], Vec<(usize, bool)>> = HashMap::new();

    for (i, (a, b)) in runs.iter().enumerate() {
        at.entry(node(*a)).or_default().push((i, false));
        at.entry(node(*b)).or_default().push((i, true));
    }

    let mut used = vec![false; runs.len()];
    let far = |i: usize, from_end: bool| if from_end { runs[i].0 } else { runs[i].1 };
    let near = |i: usize, from_end: bool| if from_end { runs[i].1 } else { runs[i].0 };

    let next = |joint: Vec3, heading: Vec3, used: &[bool]| -> Option<(usize, bool)> {
        let there = at.get(&node(joint))?;
        if there.len() != 2 {
            return None;
        }
        let &(i, at_end) = there.iter().find(|(i, _)| !used[*i])?;
        let leave = (far(i, at_end) - near(i, at_end)).normalize_or_zero();
        (heading.dot(leave) >= MIN_TURN_COS).then_some((i, at_end))
    };

    let mut rails = Vec::new();
    for start in 0..runs.len() {
        if used[start] {
            continue;
        }

        used[start] = true;
        let (a, b) = runs[start];
        let mut points = std::collections::VecDeque::from([a, b]);

        let mut tip = b;
        let mut heading = (b - a).normalize_or_zero();
        while let Some((i, at_end)) = next(tip, heading, &used) {
            used[i] = true;
            let to = far(i, at_end);
            heading = (to - tip).normalize_or_zero();
            tip = to;
            points.push_back(to);
        }

        let mut tip = a;
        let mut heading = (a - b).normalize_or_zero();
        while let Some((i, at_end)) = next(tip, heading, &used) {
            used[i] = true;
            let to = far(i, at_end);
            heading = (to - tip).normalize_or_zero();
            tip = to;
            points.push_front(to);
        }

        rails.push(points.into_iter().collect());
    }

    rails
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tri(a: [f32; 3], b: [f32; 3], c: [f32; 3]) -> Triangle {
        Triangle {
            p: [Vec3::from(a), Vec3::from(b), Vec3::from(c)],
            flags: 0,
        }
    }

    #[test]
    fn platform_outer_edges_become_rails() {
        let a = [-1.0, 0.0, -1.0];
        let b = [-1.0, 0.0, 1.0];
        let c = [1.0, 0.0, 1.0];
        let d = [1.0, 0.0, -1.0];

        let result = find_rails(&[tri(a, b, c), tri(a, c, d)]);
        assert_eq!(result.census.input_triangles, 2);
        assert_eq!(result.rails.len(), 4, "{:?}", result.census);
        assert!(result.rails.iter().all(|r| polyline_len(r) > 1.9));
    }

    #[test]
    fn ghost_surfaces_are_not_rails() {
        let mut t = tri([-1.0, 0.0, -1.0], [-1.0, 0.0, 1.0], [1.0, 0.0, 1.0]);
        t.flags = GHOST_FLAG;
        let result = find_rails(&[t]);
        assert_eq!(result.census.input_triangles, 0);
        assert!(result.rails.is_empty());
    }

    #[test]
    fn stair_helpers_can_block_probes_but_do_not_author_rails() {
        let mut a = tri([-1.0, 0.0, -1.0], [-1.0, 0.0, 1.0], [1.0, 0.0, 1.0]);
        let mut b = tri([-1.0, 0.0, -1.0], [1.0, 0.0, 1.0], [1.0, 0.0, -1.0]);
        a.flags = STAIR_HELPER_FLAG;
        b.flags = STAIR_HELPER_FLAG;
        let result = find_rails(&[a, b]);
        assert_eq!(result.census.input_triangles, 2);
        assert_eq!(result.census.walkable_edges, 0);
        assert!(result.rails.is_empty());
    }
}
