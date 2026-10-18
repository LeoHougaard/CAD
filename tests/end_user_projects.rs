//! Ten complete, dimensioned practice projects based on the official Onshape
//! feature tutorials linked in docs/end-user-projects.md. Their dimensions are
//! ours, so their independent analytic acceptance checks are reproducible.
use cad::{
    engine::{Evaluation, Runtime},
    model::{Document, Sketch, SketchConstraint},
    plugins::{Numbers, Registry},
};
use serde_json::{Value, json};
use std::{collections::BTreeMap, path::Path, sync::Mutex, time::Instant};

const PI: f64 = std::f64::consts::PI;
static PROJECT_RUN: Mutex<()> = Mutex::new(());
const IDS: [&str; 10] = [
    "adjustment-bracket",
    "bearing-block",
    "grooved-pulley",
    "instrument-enclosure",
    "cable-comb",
    "clevis-pair",
    "standoff-set",
    "swept-handle",
    "circular-reducer",
    "rectangular-duct",
];
struct Project {
    doc: Document,
    edit: (&'static str, &'static str),
    last: (&'static str, [&'static str; 2]),
}
struct Expected {
    volume: f64,
    bounds: [[f64; 3]; 2],
    parts: usize,
}
fn doc(name: &str, vars: &[(&str, &str)]) -> Document {
    Document {
        name: name.into(),
        variables: vars.iter().map(|&(k, v)| (k.into(), v.into())).collect(),
        ..Default::default()
    }
}
fn n(d: &Document, expr: &str) -> f64 {
    Numbers::new(&d.variables).unwrap().eval(expr).unwrap()
}
fn c(kind: &str, id: u64, value: &str) -> SketchConstraint {
    SketchConstraint {
        kind: kind.into(),
        a: id,
        value: value.into(),
        ..Default::default()
    }
}
fn rect(s: &mut Sketch, d: &Document, x: &str, y: &str, w: &str, h: &str) {
    let (a, b, width, height) = (n(d, x), n(d, y), n(d, w), n(d, h));
    s.add_rect([a, b], [a + width, b + height]);
    for (kind, value) in [("X", x), ("Y", y), ("Width", w), ("Height", h)] {
        s.constraints.push(c(kind, s.next_id, value));
    }
}
fn circle(s: &mut Sketch, d: &Document, x: &str, y: &str, r: &str) {
    s.add_circle([n(d, x), n(d, y)], n(d, r));
    for (kind, value) in [("X", x), ("Y", y), ("Radius", r)] {
        s.constraints.push(c(kind, s.next_id, value));
    }
}
fn polygon(d: &Document, points: &[(&str, &str)]) -> Sketch {
    let mut s = Sketch::default();
    for (i, &(x, y)) in points.iter().enumerate() {
        let (x1, y1) = points[(i + 1) % points.len()];
        s.add_line([n(d, x), n(d, y)], [n(d, x1), n(d, y1)]);
        s.constraints
            .extend([c("X", s.next_id, x), c("Y", s.next_id, y)]);
    }
    s
}
fn feature(d: &mut Document, plugin: &str, name: &str, params: Value) -> String {
    let mut f = Registry::builtin().unwrap().create(plugin, d).unwrap();
    f.name = name.into();
    for (k, v) in params.as_object().unwrap() {
        f.params[k] = v.clone();
    }
    let id = f.id.clone();
    d.features.push(f);
    id
}
fn sketch(
    d: &mut Document,
    name: &str,
    plane: &str,
    offset: &str,
    support: &str,
    s: Sketch,
) -> String {
    let id = feature(
        d,
        "sketch",
        name,
        json!({"plane":plane,"offset":offset,"support":support}),
    );
    d.features.last_mut().unwrap().set_sketch(&s);
    id
}
fn extrude(
    d: &mut Document,
    name: &str,
    sk: &str,
    depth: &str,
    mode: &str,
    reverse: bool,
) -> String {
    feature(
        d,
        "extrude",
        name,
        json!({"sketch":sk,"depth":depth,"mode":mode,"reverse":reverse,"target":"@last"}),
    )
}
fn rectangular_sketch(
    d: &mut Document,
    name: &str,
    plane: &str,
    offset: &str,
    support: &str,
    xywh: [&str; 4],
) -> String {
    let mut s = Sketch::default();
    rect(&mut s, d, xywh[0], xywh[1], xywh[2], xywh[3]);
    sketch(d, name, plane, offset, support, s)
}
fn checked(runtime: &mut Runtime, d: &Document) -> Evaluation {
    let result = runtime.evaluate(d).unwrap();
    assert!(
        result.scene.error.is_none(),
        "{}: {:?}",
        d.name,
        result.scene.statuses
    );
    result
}
fn evaluate(d: &Document) -> Evaluation {
    checked(&mut Runtime::new(Registry::builtin().unwrap()), d)
}

fn adjustment_bracket() -> Project {
    let mut d = doc(
        "Slotted adjustment bracket",
        &[
            ("width", "80"),
            ("base", "6"),
            ("rise", "40"),
            ("slot_radius", "3"),
            ("slot_centers", "12"),
        ],
    );
    let sk = rectangular_sketch(
        &mut d,
        "Base footprint",
        "Top",
        "0",
        "",
        ["-width/2", "-20", "width", "40"],
    );
    let base = extrude(&mut d, "Base plate", &sk, "base", "New", false);
    let sk = rectangular_sketch(
        &mut d,
        "Upright footprint",
        "Top",
        "0",
        &format!("{base}:end"),
        ["-width/2", "-20", "width", "6"],
    );
    extrude(&mut d, "Upright", &sk, "rise", "Add", false);
    let mut s = Sketch::default();
    for x in ["-width/4-slot_radius", "width/4-slot_radius"] {
        rect(
            &mut s,
            &d,
            x,
            "base+rise/2-slot_centers/2",
            "2*slot_radius",
            "slot_centers",
        );
    }
    let sk = sketch(&mut d, "Slot straight sections", "Front", "20", "", s);
    extrude(&mut d, "Straight slot cuts", &sk, "6", "Remove", true);
    let mut s = Sketch::default();
    for x in ["-width/4", "width/4"] {
        for y in ["base+rise/2-slot_centers/2", "base+rise/2+slot_centers/2"] {
            circle(&mut s, &d, x, y, "slot_radius");
        }
    }
    let sk = sketch(&mut d, "Slot end circles", "Front", "20", "", s);
    extrude(&mut d, "Rounded slot ends", &sk, "6", "Remove", true);
    let mut s = Sketch::default();
    for x in ["-width/2+10", "width/2-10"] {
        circle(&mut s, &d, x, "8", "3");
    }
    let sk = sketch(
        &mut d,
        "Base bolt holes",
        "Top",
        "0",
        &format!("{base}:end"),
        s,
    );
    extrude(&mut d, "Base bolt drilling", &sk, "base", "Remove", true);
    Project {
        doc: d,
        edit: ("width", "90"),
        last: ("depth", ["5.8", "5.9"]),
    }
}
fn bearing_block() -> Project {
    let mut d = doc(
        "Bearing pedestal",
        &[
            ("height", "30"),
            ("base", "6"),
            ("bore", "9"),
            ("counterbore", "5"),
        ],
    );
    let sk = rectangular_sketch(
        &mut d,
        "Footprint",
        "Top",
        "0",
        "",
        ["-40", "-25", "80", "50"],
    );
    let base = extrude(&mut d, "Foot", &sk, "base", "New", false);
    let sk = rectangular_sketch(
        &mut d,
        "Pedestal profile",
        "Top",
        "0",
        &format!("{base}:end"),
        ["-25", "-15", "50", "30"],
    );
    extrude(&mut d, "Pedestal", &sk, "height", "Add", false);
    let mut s = Sketch::default();
    circle(&mut s, &d, "0", "base+height/2", "bore");
    let sk = sketch(&mut d, "Bearing bore", "Front", "15", "", s);
    extrude(&mut d, "Through bore", &sk, "30", "Remove", true);
    let mut s = Sketch::default();
    for x in ["-30", "30"] {
        for y in ["-15", "15"] {
            circle(&mut s, &d, x, y, "3");
        }
    }
    let sk = sketch(
        &mut d,
        "Foot bolt pattern",
        "Top",
        "0",
        &format!("{base}:end"),
        s,
    );
    extrude(&mut d, "Foot drilling", &sk, "base", "Remove", true);
    let mut s = Sketch::default();
    circle(&mut s, &d, "0", "base+height/2", "11");
    let sk = sketch(&mut d, "Bearing shoulder", "Front", "15", "", s);
    extrude(&mut d, "Counterbore", &sk, "counterbore", "Remove", true);
    Project {
        doc: d,
        edit: ("height", "36"),
        last: ("depth", ["4.8", "4.9"]),
    }
}
fn grooved_pulley() -> Project {
    let mut d = doc(
        "V-groove belt pulley",
        &[("rim", "24"), ("width", "16"), ("bore", "4")],
    );
    let s = polygon(
        &d,
        &[
            ("bore", "0"),
            ("rim", "0"),
            ("rim", "4"),
            ("rim-6", "8"),
            ("rim", "12"),
            ("rim", "width"),
            ("12", "width"),
            ("12", "width+6"),
            ("bore", "width+6"),
        ],
    );
    let sk = sketch(&mut d, "Radial section", "Top", "0", "", s);
    feature(
        &mut d,
        "revolve",
        "Groove and hub",
        json!({"sketch":sk,"angle":"360","axis":"V"}),
    );
    let mut s = Sketch::default();
    for (x, y) in [("15", "0"), ("-15", "0"), ("0", "15"), ("0", "-15")] {
        circle(&mut s, &d, x, y, "2");
    }
    let sk = sketch(&mut d, "Four web holes", "Front", "0", "", s);
    extrude(&mut d, "Web drilling", &sk, "width", "Remove", true);
    Project {
        doc: d,
        edit: ("rim", "26"),
        last: ("depth", ["15.7", "15.8"]),
    }
}
fn instrument_enclosure() -> Project {
    let mut d = doc(
        "Instrument enclosure",
        &[
            ("width", "80"),
            ("depth", "50"),
            ("height", "25"),
            ("wall", "2"),
            ("boss_height", "10"),
        ],
    );
    let sk = rectangular_sketch(
        &mut d,
        "Enclosure footprint",
        "Top",
        "0",
        "",
        ["-width/2", "-depth/2", "width", "depth"],
    );
    let outer = extrude(&mut d, "Outer enclosure", &sk, "height", "New", false);
    feature(
        &mut d,
        "shell",
        "Open top",
        json!({"faces":[format!("{outer}:end")],"thickness":"wall"}),
    );
    let mut s = Sketch::default();
    for x in ["-width/2+8", "width/2-8"] {
        for y in ["-depth/2+8", "depth/2-8"] {
            circle(&mut s, &d, x, y, "2");
        }
    }
    let sk = sketch(&mut d, "Mounting screw bores", "Top", "0", "", s);
    extrude(&mut d, "Drill floor", &sk, "wall", "Remove", false);
    let mut s = Sketch::default();
    for x in ["-width/2+8", "width/2-8"] {
        for y in ["-depth/2+8", "depth/2-8"] {
            circle(&mut s, &d, x, y, "4");
            circle(&mut s, &d, x, y, "2");
        }
    }
    let sk = sketch(&mut d, "Mounting boss rings", "Top", "wall", "", s);
    extrude(&mut d, "Four bosses", &sk, "boss_height-wall", "Add", false);
    let sk = rectangular_sketch(
        &mut d,
        "Connector aperture",
        "Front",
        "depth/2",
        "",
        ["-8", "5", "16", "8"],
    );
    extrude(&mut d, "Connector opening", &sk, "wall", "Remove", true);
    Project {
        doc: d,
        edit: ("height", "30"),
        last: ("depth", ["1.8", "1.9"]),
    }
}
fn cable_comb() -> Project {
    let mut d = doc(
        "Cable management comb",
        &[
            ("width", "120"),
            ("slot", "12"),
            ("pitch", "20"),
            ("edge_break", "0.5"),
        ],
    );
    let sk = rectangular_sketch(
        &mut d,
        "Comb blank",
        "Top",
        "0",
        "",
        ["-width/2", "-12", "width", "24"],
    );
    let blank = extrude(&mut d, "Comb stock", &sk, "12", "New", false);
    let result = evaluate(&d);
    let edge = result.scene.meshes[0]
        .edges
        .iter()
        .find(|e| {
            e.points.len() == 2
                && e.points
                    .iter()
                    .all(|p| (p[1] + 12.).abs() < 1e-5 && (p[2] - 12.).abs() < 1e-5)
        })
        .unwrap()
        .key
        .clone();
    feature(
        &mut d,
        "chamfer",
        "Back edge break",
        json!({"edges":[edge],"distance":"edge_break"}),
    );
    let mut s = Sketch::default();
    for x in [
        "-2*pitch-slot/2",
        "-pitch-slot/2",
        "-slot/2",
        "pitch-slot/2",
        "2*pitch-slot/2",
    ] {
        rect(&mut s, &d, x, "-4", "slot", "20");
    }
    let sk = sketch(
        &mut d,
        "Five open channels",
        "Top",
        "0",
        &format!("{blank}:end"),
        s,
    );
    extrude(&mut d, "Cut channels", &sk, "12", "Remove", true);
    let mut s = Sketch::default();
    for x in ["-width/2+8", "width/2-8"] {
        circle(&mut s, &d, x, "-8", "2.5");
    }
    let sk = sketch(
        &mut d,
        "Mounting bores",
        "Top",
        "0",
        &format!("{blank}:end"),
        s,
    );
    extrude(&mut d, "Mounting holes", &sk, "12", "Remove", true);
    Project {
        doc: d,
        edit: ("width", "130"),
        last: ("depth", ["11.8", "11.9"]),
    }
}
fn clevis_pair() -> Project {
    let mut d = doc(
        "Mirrored clevis cheeks",
        &[("height", "40"), ("gap", "12"), ("wall", "6")],
    );
    let sk = rectangular_sketch(
        &mut d,
        "Cheek footprint",
        "Top",
        "0",
        "",
        ["gap/2", "-15", "wall", "30"],
    );
    extrude(&mut d, "Cheek", &sk, "height", "New", false);
    let sk = rectangular_sketch(
        &mut d,
        "Foot profile",
        "Top",
        "0",
        "",
        ["gap/2", "-20", "20", "40"],
    );
    extrude(&mut d, "Mounting foot", &sk, "5", "Add", false);
    let mut s = Sketch::default();
    circle(&mut s, &d, "0", "height-10", "4");
    let sk = sketch(&mut d, "Clevis pin", "Right", "gap/2+wall", "", s);
    extrude(&mut d, "Pin bore", &sk, "wall", "Remove", true);
    let mut s = Sketch::default();
    for y in ["-12", "12"] {
        circle(&mut s, &d, "gap/2+15", y, "2.5");
    }
    let sk = sketch(&mut d, "Foot screws", "Top", "5", "", s);
    extrude(&mut d, "Foot drilling", &sk, "5", "Remove", true);
    feature(
        &mut d,
        "mirror",
        "Opposite cheek",
        json!({"plane":"Right","offset":"0","keep":true}),
    );
    Project {
        doc: d,
        edit: ("height", "46"),
        last: ("offset", ["0.1", "0.2"]),
    }
}
fn corner_fillet_removed(radius: f64, r: f64) -> f64 {
    2. * PI * (radius * r * r * (1. - PI / 4.) - r.powi(3) * (5. / 6. - PI / 4.))
}
fn standoff_set() -> Project {
    let mut d = doc(
        "Flanged instrument standoffs",
        &[("height", "18"), ("pitch", "30"), ("fillet", "0.5")],
    );
    let mut s = Sketch::default();
    circle(&mut s, &d, "0", "0", "10");
    circle(&mut s, &d, "0", "0", "2.5");
    let sk = sketch(&mut d, "Flange rings", "Top", "0", "", s);
    let base = extrude(&mut d, "Flange", &sk, "3", "New", false);
    let mut s = Sketch::default();
    circle(&mut s, &d, "0", "0", "7");
    circle(&mut s, &d, "0", "0", "2.5");
    let sk = sketch(&mut d, "Stem ring", "Top", "0", &format!("{base}:end"), s);
    extrude(&mut d, "Stem", &sk, "height-3", "Add", false);
    let result = evaluate(&d);
    let edge = result.scene.meshes[0]
        .edges
        .iter()
        .find(|e| {
            !e.seam
                && e.points.iter().all(|p| (p[2] - 18.).abs() < 1e-5)
                && e.points.iter().any(|p| p[0].hypot(p[1]) > 6.9)
        })
        .unwrap()
        .key
        .clone();
    feature(
        &mut d,
        "fillet",
        "Top rim round",
        json!({"edges":[edge],"radius":"fillet"}),
    );
    feature(
        &mut d,
        "pattern",
        "Four standoffs",
        json!({"axis":"X","spacing":"pitch","count":"4"}),
    );
    Project {
        doc: d,
        edit: ("height", "22"),
        last: ("spacing", ["30.1", "30.2"]),
    }
}
fn swept_handle() -> Project {
    let mut d = doc(
        "Mitred rectangular pull handle",
        &[
            ("rise", "30"),
            ("run", "60"),
            ("foot", "4"),
            ("bar_width", "6"),
            ("bar_depth", "8"),
        ],
    );
    let sk = rectangular_sketch(
        &mut d,
        "Handle bar section",
        "Top",
        "0",
        "",
        ["-bar_width/2", "-bar_depth/2", "bar_width", "bar_depth"],
    );
    let mut s = Sketch::default();
    for (a, b, horizontal, length) in [
        ([0., 0.], [0., 30.], false, "rise"),
        ([0., 30.], [60., 30.], true, "run"),
        ([60., 30.], [60., 0.], false, "rise"),
    ] {
        s.add_line(a, b);
        s.constraints.extend([
            c(
                if horizontal { "Horizontal" } else { "Vertical" },
                s.next_id,
                "",
            ),
            c("Length", s.next_id, length),
        ]);
    }
    s.constraints.extend([c("X", 1, "0"), c("Y", 1, "0")]);
    let path = sketch(&mut d, "Handle centerline", "Front", "0", "", s);
    feature(
        &mut d,
        "sweep",
        "Bent rectangular bar",
        json!({"profile":sk,"path":path}),
    );
    let mut s = Sketch::default();
    rect(&mut s, &d, "-10", "-12", "20", "24");
    rect(&mut s, &d, "run-10", "-12", "20", "24");
    let sk = sketch(&mut d, "Mounting feet", "Top", "-foot", "", s);
    extrude(&mut d, "Add feet", &sk, "foot", "Add", false);
    let mut s = Sketch::default();
    for x in ["0", "run"] {
        for y in ["-8", "8"] {
            circle(&mut s, &d, x, y, "2");
        }
    }
    let sk = sketch(&mut d, "Foot screws", "Top", "0", "", s);
    extrude(&mut d, "Foot drilling", &sk, "foot", "Remove", true);
    Project {
        doc: d,
        edit: ("rise", "36"),
        last: ("depth", ["3.8", "3.9"]),
    }
}
fn circular_reducer() -> Project {
    let mut d = doc(
        "Flanged circular reducer",
        &[("length", "50"), ("flange", "5"), ("flange_radius", "23")],
    );
    let section = polygon(
        &d,
        &[
            ("3", "-flange"),
            ("flange_radius", "-flange"),
            ("flange_radius", "0"),
            ("16", "0"),
            ("10", "length/2"),
            ("6", "length"),
            ("3", "length"),
        ],
    );
    let sk = sketch(&mut d, "Hollow radial section", "Front", "0", "", section);
    feature(
        &mut d,
        "revolve",
        "Hollow reducer and flange",
        json!({"sketch":sk,"axis":"V","angle":"360"}),
    );
    let mut s = Sketch::default();
    for (x, y) in [("19", "0"), ("-19", "0"), ("0", "19"), ("0", "-19")] {
        circle(&mut s, &d, x, y, "2");
    }
    let sk = sketch(&mut d, "Flange bolts", "Top", "0", "", s);
    extrude(&mut d, "Bolt holes", &sk, "flange", "Remove", true);
    Project {
        doc: d,
        edit: ("length", "60"),
        last: ("depth", ["4.8", "4.9"]),
    }
}
fn rectangular_duct() -> Project {
    let mut d = doc(
        "Flanged rectangular duct transition",
        &[("length", "40"), ("flange", "4")],
    );
    let mut outer = vec![];
    let mut inner = vec![];
    for (name, z, w, h) in [("Inlet", "0", "50", "30"), ("Outlet", "length", "30", "20")] {
        outer.push(rectangular_sketch(
            &mut d,
            &format!("{name} outside"),
            "Top",
            z,
            "",
            [&format!("-{w}/2"), &format!("-{h}/2"), w, h],
        ));
    }
    feature(
        &mut d,
        "loft",
        "Outer transition",
        json!({"profiles":outer}),
    );
    for (name, z, w, h) in [("Inlet", "0", "46", "26"), ("Outlet", "length", "26", "16")] {
        inner.push(rectangular_sketch(
            &mut d,
            &format!("{name} inside"),
            "Top",
            z,
            "",
            [&format!("-{w}/2"), &format!("-{h}/2"), w, h],
        ));
    }
    feature(
        &mut d,
        "loft",
        "Air passage",
        json!({"profiles":inner,"mode":"Remove"}),
    );
    let mut s = Sketch::default();
    rect(&mut s, &d, "-35", "-25", "70", "50");
    rect(&mut s, &d, "-23", "-13", "46", "26");
    let sk = sketch(&mut d, "Flange ring", "Top", "-flange", "", s);
    extrude(&mut d, "Mounting flange", &sk, "flange", "Add", false);
    let mut s = Sketch::default();
    for x in ["-28", "28"] {
        for y in ["-18", "18"] {
            circle(&mut s, &d, x, y, "2");
        }
    }
    let sk = sketch(&mut d, "Flange mounting holes", "Top", "0", "", s);
    extrude(&mut d, "Bolt drilling", &sk, "flange", "Remove", true);
    Project {
        doc: d,
        edit: ("length", "50"),
        last: ("depth", ["3.8", "3.9"]),
    }
}
fn frustum(a: f64, b: f64, h: f64) -> f64 {
    PI * h * (a * a + a * b + b * b) / 3.
}
fn rectangular_frustum(w0: f64, h0: f64, w1: f64, h1: f64, l: f64) -> f64 {
    l * (2. * w0 * h0 + w0 * h1 + w1 * h0 + 2. * w1 * h1) / 6.
}
fn expected(id: &str, d: &Document) -> Expected {
    let (v, bounds, parts) = match id {
        "adjustment-bracket" => {
            let (w, b, h, r, l) = (
                n(d, "width"),
                n(d, "base"),
                n(d, "rise"),
                n(d, "slot_radius"),
                n(d, "slot_centers"),
            );
            (
                w * 40. * b + w * 6. * h - 2. * (2. * r * l + PI * r * r) * 6. - 2. * PI * 9. * b,
                [[-w / 2., -20., 0.], [w / 2., 20., b + h]],
                1,
            )
        }
        "bearing-block" => {
            let (h, b, r, cb) = (
                n(d, "height"),
                n(d, "base"),
                n(d, "bore"),
                n(d, "counterbore"),
            );
            (
                80. * 50. * b + 50. * 30. * h
                    - PI * r * r * 30.
                    - PI * (121. - r * r) * cb
                    - 4. * PI * 9. * b,
                [[-40., -25., 0.], [40., 25., b + h]],
                1,
            )
        }
        "grooved-pulley" => {
            let (r, w, b) = (n(d, "rim"), n(d, "width"), n(d, "bore"));
            (
                PI * (r * r * 8.
                    + 8. * (r * r + r * (r - 6.) + (r - 6.).powi(2)) / 3.
                    + 12f64.powi(2) * (w - 10.)
                    - b * b * (w + 6.))
                    - 4. * PI * 4. * w,
                [[-r, 0., -r], [r, w + 6., r]],
                1,
            )
        }
        "instrument-enclosure" => {
            let (w, l, h, t, b) = (
                n(d, "width"),
                n(d, "depth"),
                n(d, "height"),
                n(d, "wall"),
                n(d, "boss_height"),
            );
            (
                w * l * h - (w - 2. * t) * (l - 2. * t) * (h - t) + 4. * PI * 12. * (b - t)
                    - 4. * PI * 4. * t
                    - 16. * 8. * t,
                [[-w / 2., -l / 2., 0.], [w / 2., l / 2., h]],
                1,
            )
        }
        "cable-comb" => {
            let (w, s, c) = (n(d, "width"), n(d, "slot"), n(d, "edge_break"));
            (
                w * 24. * 12. - 5. * s * 16. * 12. - 2. * PI * 6.25 * 12. - c * c * w / 2.,
                [[-w / 2., -12., 0.], [w / 2., 12., 12.]],
                1,
            )
        }
        "clevis-pair" => {
            let (h, g, w) = (n(d, "height"), n(d, "gap"), n(d, "wall"));
            (
                2. * (w * 30. * h + (20. * 40. - w * 30.) * 5.
                    - PI * 16. * w
                    - 2. * PI * 6.25 * 5.),
                [[-g / 2. - 20., -20., 0.], [g / 2. + 20., 20., h]],
                2,
            )
        }
        "standoff-set" => {
            let (h, p, f) = (n(d, "height"), n(d, "pitch"), n(d, "fillet"));
            (
                4. * (PI * (100. - 6.25) * 3. + PI * (49. - 6.25) * (h - 3.)
                    - corner_fillet_removed(7., f)),
                [[-10., -10., 0.], [10. + 3. * p, 10., h]],
                4,
            )
        }
        "swept-handle" => {
            let (h, l, f, w, b) = (
                n(d, "rise"),
                n(d, "run"),
                n(d, "foot"),
                n(d, "bar_width"),
                n(d, "bar_depth"),
            );
            (
                w * b * (2. * h + l) + 2. * 20. * 24. * f - 4. * PI * 4. * f,
                [[-10., -12., -f], [l + 10., 12., h + w / 2.]],
                1,
            )
        }
        "circular-reducer" => {
            let (l, f, r) = (n(d, "length"), n(d, "flange"), n(d, "flange_radius"));
            (
                frustum(16., 10., l / 2.) + frustum(10., 6., l / 2.) - PI * 9. * l
                    + PI * (r * r - 9.) * f
                    - 4. * PI * 4. * f,
                [[-r, -r, -f], [r, r, l]],
                1,
            )
        }
        "rectangular-duct" => {
            let (l, f) = (n(d, "length"), n(d, "flange"));
            (
                rectangular_frustum(50., 30., 30., 20., l)
                    - rectangular_frustum(46., 26., 26., 16., l)
                    + (70. * 50. - 46. * 26.) * f
                    - 4. * PI * 4. * f,
                [[-35., -25., -f], [35., 25., l]],
                1,
            )
        }
        _ => unreachable!(),
    };
    Expected {
        volume: v,
        bounds,
        parts,
    }
}
fn verify(result: &Evaluation, id: &str, d: &Document) {
    let expected = expected(id, d);
    assert_eq!(result.geometry.parts.len(), expected.parts, "{id}");
    assert!(
        result.scene.sketches.values().all(|s| s.dof == 0),
        "{id}: {:?}",
        result
            .scene
            .sketches
            .iter()
            .map(|(id, s)| (id, s.dof))
            .collect::<Vec<_>>()
    );
    let actual: f64 = result.scene.meshes.iter().map(|m| m.volume).sum();
    assert!(
        (actual - expected.volume).abs() < expected.volume * 1e-7,
        "{id}: volume {actual} vs {}",
        expected.volume
    );
    let mut lo = [f64::INFINITY; 3];
    let mut hi = [f64::NEG_INFINITY; 3];
    for part in &result.geometry.parts {
        let report =
            brepkit_operations::validate::validate_solid(&result.geometry.topo, part.solid)
                .unwrap();
        assert!(report.is_valid(), "{id}: {:?}", report.issues);
        let b = brepkit_operations::measure::solid_bounding_box(&result.geometry.topo, part.solid)
            .unwrap();
        for (k, (a, b)) in [
            (b.min.x(), b.max.x()),
            (b.min.y(), b.max.y()),
            (b.min.z(), b.max.z()),
        ]
        .into_iter()
        .enumerate()
        {
            lo[k] = lo[k].min(a);
            hi[k] = hi[k].max(b);
        }
    }
    for k in 0..3 {
        assert!(
            (lo[k] - expected.bounds[0][k]).abs() < 1e-5
                && (hi[k] - expected.bounds[1][k]).abs() < 1e-5,
            "{id}: bounds {lo:?}..{hi:?} vs {:?}",
            expected.bounds
        );
    }
    for mesh in &result.scene.meshes {
        let mut edges = BTreeMap::new();
        for triangle in mesh.indices.as_chunks::<3>().0 {
            for k in 0..3 {
                let (a, b) = (triangle[k], triangle[(k + 1) % 3]);
                let entry = edges.entry((a.min(b), a.max(b))).or_insert((0, 0i32));
                entry.0 += 1;
                entry.1 += if a < b { 1 } else { -1 };
            }
        }
        assert!(
            edges
                .values()
                .all(|&(count, orientation)| count == 2 && orientation == 0),
            "{id}: display mesh is not closed and consistently oriented"
        );
    }
}
fn artifact(directory: &Path, id: &str, variant: &str, d: &Document, result: &Evaluation) {
    let path = directory.join(format!("{variant}.cadpart"));
    d.save(&path).unwrap();
    let reopened = Document::load(&path).unwrap();
    assert_eq!(&reopened, d);
    verify(&evaluate(&reopened), id, &reopened);
    #[cfg(feature = "step")]
    {
        result
            .geometry
            .export("step", &directory.join(format!("{variant}.step")))
            .unwrap();
    }
    let e = expected(id, d);
    std::fs::write(directory.join(format!("{variant}.json")),serde_json::to_vec_pretty(&json!({"project":id,"variant":variant,"expected_volume_mm3":e.volume,"expected_bounds_mm":e.bounds,"expected_parts":e.parts,"step_exported":cfg!(feature="step"),"all_sketches_zero_dof":true,"closed_brep_and_mesh":true,"document_roundtrip_verified":true})).unwrap()).unwrap();
}
fn percentile(values: &mut [f64], p: f64) -> f64 {
    values.sort_by(f64::total_cmp);
    values[((values.len() as f64 * p).ceil() as usize)
        .saturating_sub(1)
        .min(values.len() - 1)]
}
fn run(id: &str, build: fn() -> Project) {
    let _guard = PROJECT_RUN
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let project = build();
    let d = project.doc;
    let directory = Path::new("target/test-output/end-user-projects").join(id);
    std::fs::create_dir_all(&directory).unwrap();
    d.save(&directory.join("source.cadpart")).unwrap();
    let mut rt = Runtime::new(Registry::builtin().unwrap());
    let baseline = checked(&mut rt, &d);
    verify(&baseline, id, &d);
    artifact(&directory, id, "baseline", &d, &baseline);
    let mut edited = d.clone();
    edited
        .variables
        .insert(project.edit.0.into(), project.edit.1.into());
    let rebuilt = checked(&mut rt, &edited);
    verify(&rebuilt, id, &edited);
    artifact(&directory, id, "edited", &edited, &rebuilt);
    let samples = std::env::var("CAD_PROJECT_BENCH_SAMPLES")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(20)
        .clamp(5, 100);
    let mut full = vec![];
    for i in 0..samples {
        let candidate = if i.is_multiple_of(2) { &d } else { &edited };
        let mut rt = Runtime::new(Registry::builtin().unwrap());
        let start = Instant::now();
        let result = checked(&mut rt, candidate);
        assert_eq!(result.scene.reused_features, 0);
        full.push(start.elapsed().as_secs_f64() * 1000.);
    }
    let mut last = vec![];
    let mut rt = Runtime::new(Registry::builtin().unwrap());
    checked(&mut rt, &d);
    for i in 0..samples {
        let mut candidate = d.clone();
        candidate.features.last_mut().unwrap().params[project.last.0] =
            json!(project.last.1[i % 2]);
        let start = Instant::now();
        let result = checked(&mut rt, &candidate);
        assert_eq!(
            result.scene.reused_features,
            d.features.len() - 1,
            "{id}: last-feature cache"
        );
        last.push(start.elapsed().as_secs_f64() * 1000.);
    }
    let report = json!({"project":id,"result":"passed","history_entries":d.features.len(),"sketches":baseline.scene.sketches.len(),"parts":baseline.geometry.parts.len(),"triangles":baseline.scene.meshes.iter().map(|m|m.indices.len()/3).sum::<usize>(),"upstream_edit":{"variable":project.edit.0,"value":project.edit.1},"benchmark":{"profile":if cfg!(debug_assertions){"debug"}else{"release"},"architecture":std::env::consts::ARCH,"os":std::env::consts::OS,"samples":samples,"full_rebuild_ms":{"p50":percentile(&mut full,0.5),"p95":percentile(&mut full,0.95)},"last_feature_edit_ms":{"p50":percentile(&mut last,0.5),"p95":percentile(&mut last,0.95)},"scope":"engine regeneration and display mesh construction, not interactive frame latency"}});
    std::fs::write(
        directory.join("verification.json"),
        serde_json::to_vec_pretty(&report).unwrap(),
    )
    .unwrap();
    assert!(IDS.contains(&id));
}
macro_rules! project_test {
    ($test:ident,$id:literal,$build:ident) => {
        #[test]
        fn $test() {
            run($id, $build)
        }
    };
}
project_test!(
    adjustment_bracket_project,
    "adjustment-bracket",
    adjustment_bracket
);
project_test!(bearing_block_project, "bearing-block", bearing_block);
project_test!(grooved_pulley_project, "grooved-pulley", grooved_pulley);
project_test!(
    instrument_enclosure_project,
    "instrument-enclosure",
    instrument_enclosure
);
project_test!(cable_comb_project, "cable-comb", cable_comb);
project_test!(clevis_pair_project, "clevis-pair", clevis_pair);
project_test!(standoff_set_project, "standoff-set", standoff_set);
project_test!(swept_handle_project, "swept-handle", swept_handle);
project_test!(
    circular_reducer_project,
    "circular-reducer",
    circular_reducer
);
project_test!(
    rectangular_duct_project,
    "rectangular-duct",
    rectangular_duct
);
