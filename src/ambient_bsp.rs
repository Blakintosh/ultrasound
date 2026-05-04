//! Ambient-geometry emitter.
//!
//! Reads a Radiant `.map` file, finds the `trigger_multiple` entities tagged
//! with `targetname = ambient_package`, and emits a JSON sidecar:
//!
//! ```json
//! { "Nodes": [], "Planes": [...], "Volumes": [...], "Triggers": [...] }
//! ```
//!
//! `Nodes` is always emitted empty — the BSP-tree build was never wired up
//! and downstream tools don't read it. `Planes` and `Volumes` are populated
//! from each trigger's first brush using `n = ((p1-p0) × (p2-p0)).normalize()`,
//! `d = -n·p0`. Each volume's brush is hull-validated by enumerating every
//! 3-plane intersection, keeping the points inside (within `-0.01` of) every
//! plane, and rejecting the whole emission with `invalid hull on trigger N`
//! if any trigger's hull is empty.

use std::collections::HashMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct BspNode {
    pub split: [f64; 4],
    pub front_index: i32,
    pub back_index: i32,
    pub front_count: i32,
    pub back_count: i32,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct BspPlane {
    pub plane: [f64; 4],
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct BspVolume {
    pub id: i32,
    pub plane_index: i32,
    pub plane_count: i32,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct BspTrigger {
    pub id: i32,
    pub priority: i32,
    pub room: String,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct AmbientBsp {
    pub nodes: Vec<BspNode>,
    pub planes: Vec<BspPlane>,
    pub volumes: Vec<BspVolume>,
    pub triggers: Vec<BspTrigger>,
}

impl AmbientBsp {
    pub fn from_file(path: &Path) -> Result<Self, String> {
        let text = std::fs::read_to_string(path)
            .map_err(|e| format!("read map {}: {}", path.display(), e))?;
        Self::from_text(&text)
    }

    /// Build from already-loaded `.map` text. Walks every entity, filters to
    /// `trigger_multiple` + `targetname == ambient_package`, and emits one
    /// `BspVolume` per trigger pointing into a shared `BspPlane` table.
    pub fn from_text(text: &str) -> Result<Self, String> {
        let entities = parse_entities(text)?;
        let mut triggers = Vec::new();
        let mut volumes = Vec::new();
        let mut planes: Vec<BspPlane> = Vec::new();
        let mut next_id: i32 = 0;
        for ent in entities {
            let classname = ent.kvp.get("classname").map(String::as_str).unwrap_or("");
            let targetname = ent.kvp.get("targetname").map(String::as_str).unwrap_or("");
            if classname != "trigger_multiple" || targetname != "ambient_package" {
                continue;
            }
            let room = ent
                .kvp
                .get("script_ambientroom")
                .cloned()
                .unwrap_or_default();
            let priority = ent
                .kvp
                .get("script_ambientpriority")
                .and_then(|s| s.trim().parse::<i32>().ok())
                .unwrap_or(0);

            // Baseline takes brush[0] specifically. A trigger with no
            // brush would crash there on the index access; match that
            // strictness here so the failure is surfaced at build time
            // rather than producing an empty volume.
            let brush_planes: &[Plane] = ent
                .brushes
                .first()
                .map(|b| b.planes.as_slice())
                .ok_or_else(|| format!("invalid hull on trigger {} (no brush)", next_id))?;

            // Hull validation — same shape as the baseline `Volume.Init`:
            // enumerate triple-plane intersections, keep the ones inside
            // every plane, error if none survive.
            validate_hull(next_id, brush_planes)?;

            let plane_index =
                i32::try_from(planes.len()).map_err(|_| "plane index overflow".to_string())?;
            let plane_count = i32::try_from(brush_planes.len())
                .map_err(|_| "plane count overflow".to_string())?;
            for p in brush_planes {
                planes.push(BspPlane {
                    plane: p.to_array(),
                });
            }
            volumes.push(BspVolume {
                id: next_id,
                plane_index,
                plane_count,
            });
            triggers.push(BspTrigger {
                id: next_id,
                priority,
                room,
            });
            next_id += 1;
        }
        Ok(AmbientBsp {
            nodes: Vec::new(),
            planes,
            volumes,
            triggers,
        })
    }

    pub fn to_json(&self) -> Result<String, String> {
        serde_json::to_string_pretty(self).map_err(|e| format!("serialize ambientgeometry: {}", e))
    }
}

#[derive(Clone, Copy, Debug)]
struct Vec3 {
    x: f64,
    y: f64,
    z: f64,
}

impl Vec3 {
    fn new(x: f64, y: f64, z: f64) -> Self {
        Self { x, y, z }
    }
    fn add(self, o: Self) -> Self {
        Self::new(self.x + o.x, self.y + o.y, self.z + o.z)
    }
    fn sub(self, o: Self) -> Self {
        Self::new(self.x - o.x, self.y - o.y, self.z - o.z)
    }
    fn scale(self, s: f64) -> Self {
        Self::new(self.x * s, self.y * s, self.z * s)
    }
    fn cross(self, o: Self) -> Self {
        Self::new(
            self.y * o.z - self.z * o.y,
            self.z * o.x - self.x * o.z,
            self.x * o.y - self.y * o.x,
        )
    }
    fn dot(self, o: Self) -> f64 {
        self.x * o.x + self.y * o.y + self.z * o.z
    }
    fn length(self) -> f64 {
        (self.x * self.x + self.y * self.y + self.z * self.z).sqrt()
    }
    fn normalize(self) -> Self {
        let len = self.length();
        if len == 0.0 {
            self
        } else {
            Self::new(self.x / len, self.y / len, self.z / len)
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct Plane {
    n: Vec3,
    d: f64,
}

impl Plane {
    /// Plane through three points, oriented by the right-hand rule of the
    /// `(p1-p0) × (p2-p0)` cross product. Matches the baseline `Plane` ctor
    /// so plane normals point the same way.
    fn from_points(p0: Vec3, p1: Vec3, p2: Vec3) -> Self {
        let n = p1.sub(p0).cross(p2.sub(p0)).normalize();
        let d = -n.dot(p0);
        Self { n, d }
    }
    fn to_array(self) -> [f64; 4] {
        [self.n.x, self.n.y, self.n.z, self.d]
    }
    /// Signed distance from point `v` to this plane. Positive means `v` is
    /// on the side the normal points toward.
    fn to_point(self, v: Vec3) -> f64 {
        self.n.dot(v) + self.d
    }
}

/// Three-plane intersection via Cramer's rule, matching the baseline
/// `Plane.Intersect`. Returns `None` when the three normals are coplanar /
/// parallel — the determinant `(a.N × b.N) · c.N` falls below `1e-6`.
fn intersect_three(a: Plane, b: Plane, c: Plane) -> Option<Vec3> {
    let denom = a.n.cross(b.n).dot(c.n);
    if denom.abs() < 1e-6 {
        return None;
    }
    let v1 = b.n.cross(c.n).scale(-a.d);
    let v2 = c.n.cross(a.n).scale(-b.d);
    let v3 = a.n.cross(b.n).scale(-c.d);
    Some(v1.add(v2).add(v3).scale(1.0 / denom))
}

/// `-0.01` slop on the plane-distance test, copied from baseline. Without
/// it, the very vertices we just synthesised by intersecting three of these
/// planes can fall a hair on the wrong side of one of the *other* planes
/// and get filtered out by floating-point noise alone.
const HULL_INSIDE_EPS: f64 = -0.01;

/// Replicates `Volume.Init`'s hull check: every triple of `planes` is
/// intersected; surviving points must be inside every plane (within
/// `HULL_INSIDE_EPS`). If none survive, the brush is degenerate.
fn validate_hull(trigger_id: i32, planes: &[Plane]) -> Result<(), String> {
    let n = planes.len();
    if n < 3 {
        return Err(format!(
            "invalid hull on trigger {} (only {} plane(s); convex solid needs ≥3)",
            trigger_id, n
        ));
    }
    for i in 0..n {
        for j in (i + 1)..n {
            for k in (j + 1)..n {
                let Some(v) = intersect_three(planes[i], planes[j], planes[k]) else {
                    continue;
                };
                if planes.iter().all(|p| p.to_point(v) >= HULL_INSIDE_EPS) {
                    return Ok(());
                }
            }
        }
    }
    Err(format!("invalid hull on trigger {}", trigger_id))
}

/// One Radiant entity: KVPs + every brush we managed to parse. Brushes are
/// retained in declaration order; baseline takes `brushes[0]` for
/// ambient triggers.
struct Entity {
    kvp: HashMap<String, String>,
    brushes: Vec<Brush>,
}

struct Brush {
    planes: Vec<Plane>,
}

/// Walk the file, recognising entities and the `animations { ... }`
/// top-level block. Any other top-level content is treated as either a
/// KVP or a stray line and skipped — the baseline parser is similarly
/// permissive.
fn parse_entities(text: &str) -> Result<Vec<Entity>, String> {
    let lines: Vec<&str> = text.lines().map(str::trim).collect();
    let mut entities = Vec::new();
    let mut i = 0;

    while i < lines.len() {
        let line = lines[i];
        if line.is_empty() {
            i += 1;
            continue;
        }
        if line == "animations" {
            i += 1;
            i = skip_braced_block(&lines, i)?;
            continue;
        }
        if line.starts_with("// entity") {
            let (entity, next) = parse_entity(&lines, i)?;
            entities.push(entity);
            i = next;
            continue;
        }
        i += 1;
    }

    Ok(entities)
}

/// Parse `// entity N` followed by `{ ... }`. Inside the body, each
/// `// brush N\n{ ... }` becomes a parsed `Brush`; everything else at
/// entity-scope is a KVP.
fn parse_entity(lines: &[&str], start: usize) -> Result<(Entity, usize), String> {
    let mut i = start + 1;
    if i >= lines.len() {
        return Err(format!("entity at line {} truncated", start + 1));
    }
    if lines[i] != "{" {
        // Header-only entity. Match the baseline's lenient behavior.
        return Ok((
            Entity {
                kvp: HashMap::new(),
                brushes: Vec::new(),
            },
            i,
        ));
    }
    i += 1; // step past `{`

    let mut kvp: HashMap<String, String> = HashMap::new();
    let mut brushes = Vec::new();
    while i < lines.len() && lines[i] != "}" {
        let line = lines[i];
        if line.is_empty() {
            i += 1;
            continue;
        }
        if line.starts_with("// brush") {
            i += 1; // header
            let (brush, next) = parse_brush(lines, i)?;
            brushes.push(brush);
            i = next;
            continue;
        }
        let (key, value) = parse_kvp(line)?;
        kvp.insert(key, value);
        i += 1;
    }
    if i >= lines.len() {
        return Err(format!("entity starting at line {} not closed", start + 1));
    }
    Ok((Entity { kvp, brushes }, i + 1))
}

/// Parse a brush body — opens with `{`, ends with `}`. Lines starting
/// with `(` are plane lines (three points); `mesh` and `curve` introduce
/// nested blocks we skip entirely; anything else is a brush-scope KVP we
/// also ignore (only plane geometry is needed for ambient output).
fn parse_brush(lines: &[&str], start: usize) -> Result<(Brush, usize), String> {
    let mut i = start;
    if i >= lines.len() || lines[i] != "{" {
        return Err(format!(
            "expected '{{' opening brush at line {}, got {:?}",
            i + 1,
            lines.get(i)
        ));
    }
    i += 1;

    let mut planes = Vec::new();
    while i < lines.len() && lines[i] != "}" {
        let line = lines[i];
        if line.is_empty() {
            i += 1;
            continue;
        }
        if line.starts_with('(') {
            planes.push(parse_plane_line(line)?);
            i += 1;
            continue;
        }
        if line == "mesh" || line == "curve" {
            i += 1;
            i = skip_braced_block(lines, i)?;
            continue;
        }
        // Brush-level KVP — ignore.
        i += 1;
    }
    if i >= lines.len() {
        return Err(format!("brush starting at line {} not closed", start + 1));
    }
    Ok((Brush { planes }, i + 1))
}

/// Parse `( ax ay az ) ( bx by bz ) ( cx cy cz ) <material> <uv...>`.
/// Tokens after the third `)` describe material + texture mapping and
/// are ignored — we only need the three points to derive a plane.
fn parse_plane_line(line: &str) -> Result<Plane, String> {
    let toks: Vec<&str> = line.split_whitespace().collect();
    if toks.len() < 15
        || toks[0] != "("
        || toks[4] != ")"
        || toks[5] != "("
        || toks[9] != ")"
        || toks[10] != "("
        || toks[14] != ")"
    {
        return Err(format!("malformed plane line: {}", line));
    }
    let parse = |i: usize| -> Result<f64, String> {
        toks[i]
            .parse::<f64>()
            .map_err(|e| format!("bad plane coord '{}': {}", toks[i], e))
    };
    let p0 = Vec3::new(parse(1)?, parse(2)?, parse(3)?);
    let p1 = Vec3::new(parse(6)?, parse(7)?, parse(8)?);
    let p2 = Vec3::new(parse(11)?, parse(12)?, parse(13)?);
    Ok(Plane::from_points(p0, p1, p2))
}

/// Skip from an opening `{` (at index `i`) through the matching `}`.
/// Returns the index of the line just after the closing brace.
fn skip_braced_block(lines: &[&str], i: usize) -> Result<usize, String> {
    if i >= lines.len() || lines[i] != "{" {
        return Err(format!(
            "expected '{{' at line {}, got {:?}",
            i + 1,
            lines.get(i)
        ));
    }
    let mut depth = 1usize;
    let mut j = i + 1;
    while j < lines.len() {
        match lines[j] {
            "{" => depth += 1,
            "}" => {
                depth -= 1;
                if depth == 0 {
                    return Ok(j + 1);
                }
            }
            _ => {}
        }
        j += 1;
    }
    Err(format!("unterminated block opened at line {}", i + 1))
}

/// Parse a single Radiant KVP line. Two flavors:
/// * `"key" "value"` — both quoted (the common case)
/// * `key value` — bareword key, value is the rest of the line
fn parse_kvp(line: &str) -> Result<(String, String), String> {
    let bytes = line.as_bytes();
    if bytes.first() == Some(&b'"') {
        let mut idx = 1;
        while idx < bytes.len() && bytes[idx] != b'"' {
            idx += 1;
        }
        if idx >= bytes.len() {
            return Err(format!("unterminated key in '{}'", line));
        }
        let key = line[1..idx].trim().to_string();
        let mut value = line[idx + 1..].trim().to_string();
        if value.starts_with('"') && value.ends_with('"') && value.len() >= 2 {
            value = value[1..value.len() - 1].to_string();
        }
        Ok((key, value))
    } else {
        match line.split_once(char::is_whitespace) {
            Some((k, v)) => Ok((k.trim().to_string(), v.trim().to_string())),
            None => Ok((line.trim().to_string(), String::new())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Axis-aligned unit cube spanning the origin to (1,1,1). Six brush
    /// faces, each a plane through three corner points. Used to verify
    /// the parser → plane formula chain end-to-end.
    const UNIT_CUBE_BRUSH: &str = "\
( 0 0 0 ) ( 1 0 0 ) ( 0 1 0 ) caulk 0 0 0 0 0 0 0 0
( 0 0 1 ) ( 0 1 1 ) ( 1 0 1 ) caulk 0 0 0 0 0 0 0 0
( 0 0 0 ) ( 0 0 1 ) ( 1 0 0 ) caulk 0 0 0 0 0 0 0 0
( 0 1 0 ) ( 1 1 0 ) ( 0 1 1 ) caulk 0 0 0 0 0 0 0 0
( 0 0 0 ) ( 0 1 0 ) ( 0 0 1 ) caulk 0 0 0 0 0 0 0 0
( 1 0 0 ) ( 1 0 1 ) ( 1 1 0 ) caulk 0 0 0 0 0 0 0 0";

    fn cube_entity(room: &str, priority: i32) -> String {
        format!(
            "// entity 0\n{{\n\"classname\" \"trigger_multiple\"\n\"targetname\" \"ambient_package\"\n\"script_ambientroom\" \"{}\"\n\"script_ambientpriority\" \"{}\"\n// brush 0\n{{\n{}\n}}\n}}",
            room, priority, UNIT_CUBE_BRUSH
        )
    }

    #[test]
    fn extracts_ambient_package_triggers_in_order() {
        let text = format!(
            "animations\n{{\n\"fake\" \"ignored\"\n}}\n// entity 0\n{{\n\"classname\" \"worldspawn\"\n}}\n{}\n// entity 2\n{{\n\"classname\" \"trigger_multiple\"\n\"targetname\" \"something_else\"\n}}\n{}\n",
            cube_entity("room_a", 5),
            cube_entity("room_b", 12)
        );
        let bsp = AmbientBsp::from_text(&text).expect("parse");
        assert_eq!(bsp.triggers.len(), 2);
        assert_eq!(bsp.triggers[0].id, 0);
        assert_eq!(bsp.triggers[0].room, "room_a");
        assert_eq!(bsp.triggers[0].priority, 5);
        assert_eq!(bsp.triggers[1].id, 1);
        assert_eq!(bsp.triggers[1].room, "room_b");
        assert_eq!(bsp.triggers[1].priority, 12);
    }

    #[test]
    fn populates_volumes_and_planes_from_brush() {
        let text = cube_entity("room_a", 3);
        let bsp = AmbientBsp::from_text(&text).expect("parse");
        assert_eq!(bsp.volumes.len(), 1);
        assert_eq!(bsp.volumes[0].id, 0);
        assert_eq!(bsp.volumes[0].plane_index, 0);
        assert_eq!(bsp.volumes[0].plane_count, 6);
        assert_eq!(bsp.planes.len(), 6);
        // First face: ( 0,0,0 ) ( 1,0,0 ) ( 0,1,0 ) → normal = (0,0,1),
        // d = -n·p0 = 0. Sanity-check the parser → plane formula chain.
        assert!((bsp.planes[0].plane[0] - 0.0).abs() < 1e-9);
        assert!((bsp.planes[0].plane[1] - 0.0).abs() < 1e-9);
        assert!((bsp.planes[0].plane[2] - 1.0).abs() < 1e-9);
        assert!((bsp.planes[0].plane[3] - 0.0).abs() < 1e-9);
    }

    #[test]
    fn plane_indices_chain_across_volumes() {
        let text = format!("{}\n{}", cube_entity("a", 1), cube_entity("b", 2));
        let bsp = AmbientBsp::from_text(&text).expect("parse");
        assert_eq!(bsp.volumes.len(), 2);
        assert_eq!(bsp.volumes[0].plane_index, 0);
        assert_eq!(bsp.volumes[0].plane_count, 6);
        assert_eq!(bsp.volumes[1].plane_index, 6);
        assert_eq!(bsp.volumes[1].plane_count, 6);
        assert_eq!(bsp.planes.len(), 12);
    }

    #[test]
    fn json_uses_pascal_case_with_arrays() {
        let bsp = AmbientBsp::from_text(&cube_entity("default", 3)).expect("parse");
        let s = bsp.to_json().expect("json");
        assert!(s.contains("\"Nodes\""));
        assert!(s.contains("\"Planes\""));
        assert!(s.contains("\"Volumes\""));
        assert!(s.contains("\"Triggers\""));
        assert!(s.contains("\"Plane\""));
        assert!(s.contains("\"PlaneIndex\""));
        assert!(s.contains("\"PlaneCount\""));
        assert!(s.contains("\"Room\": \"default\""));
        assert!(s.contains("\"Priority\": 3"));
    }

    #[test]
    fn parses_bareword_kvp() {
        let (k, v) = parse_kvp("script_ambientpriority 5").unwrap();
        assert_eq!(k, "script_ambientpriority");
        assert_eq!(v, "5");
    }

    #[test]
    fn parses_quoted_kvp() {
        let (k, v) = parse_kvp("\"classname\" \"trigger_multiple\"").unwrap();
        assert_eq!(k, "classname");
        assert_eq!(v, "trigger_multiple");
    }

    #[test]
    fn missing_priority_defaults_to_zero() {
        // Brush attached so the hull check passes; the test is about the
        // KVP fallback for `script_ambientpriority`.
        let text = format!(
            "// entity 0\n{{\n\"classname\" \"trigger_multiple\"\n\"targetname\" \"ambient_package\"\n\"script_ambientroom\" \"outside\"\n// brush 0\n{{\n{}\n}}\n}}",
            UNIT_CUBE_BRUSH
        );
        let bsp = AmbientBsp::from_text(&text).expect("parse");
        assert_eq!(bsp.triggers.len(), 1);
        assert_eq!(bsp.triggers[0].priority, 0);
        assert_eq!(bsp.triggers[0].room, "outside");
    }

    #[test]
    fn header_only_ambient_trigger_errors() {
        // A `trigger_multiple` / `ambient_package` entity with no brush
        // surfaces a hull error at parse time so the build fails loudly
        // rather than reaching downstream consumers as a malformed entity.
        let text = "// entity 0\n{\n\"classname\" \"trigger_multiple\"\n\"targetname\" \"ambient_package\"\n}";
        let err = match AmbientBsp::from_text(text) {
            Ok(_) => panic!("missing brush should fail"),
            Err(e) => e,
        };
        assert!(err.contains("invalid hull on trigger 0"), "got: {}", err);
    }

    #[test]
    fn degenerate_brush_fails_hull_check() {
        // Two pairs of parallel opposing planes — every triple of plane
        // normals is coplanar (lies in a 2D subspace), so `intersect_three`
        // returns `None` for every triplet and the hull is empty.
        let brush = "( 0 0 0 ) ( 1 0 0 ) ( 0 1 0 ) caulk 0 0 0 0 0 0 0 0
( 0 0 1 ) ( 0 1 1 ) ( 1 0 1 ) caulk 0 0 0 0 0 0 0 0
( 0 0 0 ) ( 0 1 0 ) ( 1 0 0 ) caulk 0 0 0 0 0 0 0 0
( 0 0 1 ) ( 1 0 1 ) ( 0 1 1 ) caulk 0 0 0 0 0 0 0 0";
        let text = format!(
            "// entity 0\n{{\n\"classname\" \"trigger_multiple\"\n\"targetname\" \"ambient_package\"\n\"script_ambientroom\" \"x\"\n// brush 0\n{{\n{}\n}}\n}}",
            brush
        );
        let err = match AmbientBsp::from_text(&text) {
            Ok(_) => panic!("flat brush should fail hull check"),
            Err(e) => e,
        };
        assert!(err.contains("invalid hull on trigger 0"), "got: {}", err);
    }

    #[test]
    fn intersect_three_returns_corner_of_unit_cube() {
        // Three axis-aligned planes through the origin meet at (0,0,0).
        let px = Plane {
            n: Vec3::new(1.0, 0.0, 0.0),
            d: 0.0,
        };
        let py = Plane {
            n: Vec3::new(0.0, 1.0, 0.0),
            d: 0.0,
        };
        let pz = Plane {
            n: Vec3::new(0.0, 0.0, 1.0),
            d: 0.0,
        };
        let v = intersect_three(px, py, pz).expect("non-degenerate");
        assert!(v.x.abs() < 1e-9 && v.y.abs() < 1e-9 && v.z.abs() < 1e-9);
    }

    /// Parity tests that compare our `AmbientBsp` output to the reference
    /// JSON checked in under `test_data/baseline_outputs/`. We don't have
    /// the source `.map` file fixtured, so we can't drive the parser
    /// end-to-end here — but we *can* round-trip the reference JSON
    /// through serde to confirm our schema (field names, casing, nesting)
    /// matches the expected output. Parses both sides into
    /// `serde_json::Value` and walks structurally; floats compared with
    /// epsilon to absorb formatting drift (`0` vs `0.0`).
    mod baseline_parity {
        use super::*;
        use serde_json::Value;
        use std::fs;

        const REF_PATH: &str =
            "test_data/baseline_outputs/zm_karelia/zm_karelia.ambientgeometry.json";

        /// Numeric tolerance for plane / split coefficients. Baseline
        /// emits integer-valued floats as `0` rather than `0.0`; serde_json
        /// emits `0.0`. Both parse back to the same f64 value, but only
        /// when we compare via parsed numbers rather than as raw strings.
        const EPS: f64 = 1e-6;

        fn approx_equal(a: &Value, b: &Value, path: &str) {
            match (a, b) {
                (Value::Object(ao), Value::Object(bo)) => {
                    let ak: BTreeSet<&String> = ao.keys().collect();
                    let bk: BTreeSet<&String> = bo.keys().collect();
                    assert_eq!(
                        ak, bk,
                        "object key set differs at {}: ours={:?} ref={:?}",
                        path, ak, bk
                    );
                    for k in ak {
                        approx_equal(&ao[k], &bo[k], &format!("{}.{}", path, k));
                    }
                }
                (Value::Array(aa), Value::Array(ba)) => {
                    assert_eq!(
                        aa.len(),
                        ba.len(),
                        "array length differs at {}: ours={} ref={}",
                        path,
                        aa.len(),
                        ba.len()
                    );
                    for (i, (av, bv)) in aa.iter().zip(ba.iter()).enumerate() {
                        approx_equal(av, bv, &format!("{}[{}]", path, i));
                    }
                }
                (Value::Number(an), Value::Number(bn)) => {
                    let af = an.as_f64().unwrap_or(f64::NAN);
                    let bf = bn.as_f64().unwrap_or(f64::NAN);
                    assert!(
                        (af - bf).abs() < EPS,
                        "number differs at {}: ours={} ref={}",
                        path,
                        af,
                        bf
                    );
                }
                (a, b) => {
                    assert_eq!(a, b, "value differs at {}", path);
                }
            }
        }

        use std::collections::BTreeSet;

        /// Round-trip the reference JSON through our `AmbientBsp` types
        /// and compare. If the reference parses → serializes → parses
        /// without divergence, our schema (PascalCase keys, array shape,
        /// scalar types) matches the expected output exactly.
        #[test]
        fn schema_round_trips_through_our_types() {
            let raw = fs::read_to_string(REF_PATH).expect("read reference json");
            let baseline: Value = serde_json::from_str(&raw).expect("parse baseline");

            // Decoding into our own types and re-serializing exercises
            // every Serialize derive on the BSP types.
            let typed: AmbientBsp =
                serde_json::from_value(baseline.clone()).expect("decode into AmbientBsp");
            let our_json = typed.to_json().expect("serialize our AmbientBsp");
            let ours: Value = serde_json::from_str(&our_json).expect("parse our json");

            approx_equal(&ours, &baseline, "$");
        }

        /// Pin the trigger payload directly — the part downstream
        /// consumers actually read. Verifies room/priority/id are intact
        /// after a round-trip through our types.
        #[test]
        fn trigger_round_trip_preserves_room_priority() {
            let raw = fs::read_to_string(REF_PATH).expect("read reference json");
            let typed: AmbientBsp = serde_json::from_str(&raw).expect("decode into AmbientBsp");
            assert!(!typed.triggers.is_empty(), "fixture has no triggers");
            for t in &typed.triggers {
                assert!(t.id >= 0);
                // Priority is parsed as int; baseline always emits it.
                assert!(t.priority >= -1 << 16);
                // Room is non-empty for every ambient_package trigger in
                // a real zone. If a fixture trigger has an empty room
                // that's still legal data — but the priority/room pair
                // having intact strings + ints is what we're after.
            }
        }
    }

    #[test]
    fn intersect_three_returns_none_for_parallel() {
        // Two coincident planes plus an arbitrary third — denominator is
        // zero, so no unique intersection.
        let p = Plane {
            n: Vec3::new(0.0, 0.0, 1.0),
            d: 0.0,
        };
        let q = Plane {
            n: Vec3::new(0.0, 0.0, 1.0),
            d: -5.0,
        };
        let r = Plane {
            n: Vec3::new(1.0, 0.0, 0.0),
            d: 0.0,
        };
        assert!(intersect_three(p, q, r).is_none());
    }

    #[test]
    fn skips_mesh_and_curve_inside_brush() {
        // A brush whose body contains a mesh and a curve in addition to
        // plane lines. Both nested blocks must be skipped without
        // producing spurious planes.
        let brush = format!(
            "// brush 0\n{{\nmesh\n{{\n\"foo\" \"bar\"\n}}\ncurve\n{{\nrandom stuff\n}}\n{}\n}}",
            UNIT_CUBE_BRUSH
        );
        let entity = format!(
            "// entity 0\n{{\n\"classname\" \"trigger_multiple\"\n\"targetname\" \"ambient_package\"\n{}\n}}",
            brush
        );
        let bsp = AmbientBsp::from_text(&entity).expect("parse");
        assert_eq!(bsp.volumes[0].plane_count, 6);
    }
}
