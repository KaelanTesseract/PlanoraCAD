//! glTF (.glb): Kopf, JSON-Block, Puffergrößen, Zugriffsgrenzen und min/max für POSITION
//! (Teilprojekt 9, Task 8).

use planora_exchange::*;
use serde_json::Value;

/// Einheitswürfel, um `shift` in x verschoben.
fn cube(shift: f32) -> ExchangeMesh {
    let mut m = ExchangeMesh {
        positions: vec![
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [1.0, 1.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, 0.0, 1.0],
            [1.0, 0.0, 1.0],
            [1.0, 1.0, 1.0],
            [0.0, 1.0, 1.0],
        ],
        indices: vec![
            0, 2, 1, 0, 3, 2, 4, 5, 6, 4, 6, 7, 0, 1, 5, 0, 5, 4, 3, 7, 6, 3, 6, 2, 0, 4, 7, 0, 7,
            3, 1, 2, 6, 1, 6, 5,
        ],
    };
    for p in &mut m.positions {
        p[0] += shift;
    }
    m
}

/// Tetraeder mit Kantenlänge 10 an den Achsen (Volumen 1000/6).
fn tetra() -> ExchangeMesh {
    ExchangeMesh {
        positions: vec![
            [0.0, 0.0, 0.0],
            [10.0, 0.0, 0.0],
            [0.0, 10.0, 0.0],
            [0.0, 0.0, 10.0],
        ],
        indices: vec![0, 2, 1, 0, 1, 3, 0, 3, 2, 1, 2, 3],
    }
}

struct Glb {
    json: Value,
    bin: Vec<u8>,
}

fn u32_at(b: &[u8], at: usize) -> usize {
    u32::from_le_bytes(b[at..at + 4].try_into().unwrap()) as usize
}

/// Liest eine .glb streng: Kopf, ein JSON-Block, ein BIN-Block, Längen passend.
fn parse_glb(b: &[u8]) -> Glb {
    assert_eq!(&b[..4], b"glTF");
    assert_eq!(u32_at(b, 4), 2, "Version");
    assert_eq!(u32_at(b, 8), b.len(), "Gesamtlänge");
    let json_len = u32_at(b, 12);
    assert_eq!(u32_at(b, 16), 0x4E4F_534A, "JSON-Block");
    assert!(json_len.is_multiple_of(4));
    let json: Value = serde_json::from_slice(&b[20..20 + json_len]).unwrap();
    let at = 20 + json_len;
    let bin_len = u32_at(b, at);
    assert_eq!(u32_at(b, at + 4), 0x004E_4942, "BIN-Block");
    assert_eq!(at + 8 + bin_len, b.len());
    Glb {
        json,
        bin: b[at + 8..].to_vec(),
    }
}

fn number(v: &Value) -> usize {
    v.as_u64().unwrap() as usize
}

/// Die Bytes, auf die ein Zugriff zeigt.
fn view_bytes<'a>(g: &'a Glb, accessor: &Value) -> &'a [u8] {
    let view = &g.json["bufferViews"][number(&accessor["bufferView"])];
    let start = number(&view["byteOffset"]);
    &g.bin[start..start + number(&view["byteLength"])]
}

fn write(bodies: &[ExportBody]) -> Glb {
    let mut out = Vec::new();
    write_glb(&mut out, bodies).unwrap();
    parse_glb(&out)
}

#[test]
fn header_chunks_buffers_and_accessors_are_consistent() {
    let bodies = [
        ExportBody {
            name: "Würfel \"A\"".into(),
            mesh: cube(0.0),
        },
        ExportBody {
            name: "B".into(),
            mesh: tetra(),
        },
    ];
    let g = write(&bodies);
    let j = &g.json;
    assert_eq!(j["asset"]["version"], "2.0");
    assert_eq!(j["asset"]["generator"], "PlanoraCAD");
    assert_eq!(number(&j["buffers"][0]["byteLength"]), g.bin.len());
    assert_eq!(j["scenes"][0]["nodes"], serde_json::json!([0, 1]));
    assert_eq!(j["nodes"][0]["name"], "Würfel \"A\"");
    assert_eq!(j["nodes"][1]["mesh"], 1);
    for v in j["bufferViews"].as_array().unwrap() {
        assert!(number(&v["byteOffset"]) + number(&v["byteLength"]) <= g.bin.len());
        assert!(number(&v["byteOffset"]).is_multiple_of(4));
    }
    let meshes = j["meshes"].as_array().unwrap();
    assert_eq!(meshes.len(), 2);
    for (k, mesh) in meshes.iter().enumerate() {
        let prim = &mesh["primitives"][0];
        assert_eq!(prim["mode"], 4);
        let pos = &j["accessors"][number(&prim["attributes"]["POSITION"])];
        let nor = &j["accessors"][number(&prim["attributes"]["NORMAL"])];
        let idx = &j["accessors"][number(&prim["indices"])];
        assert_eq!(pos["componentType"], 5126);
        assert_eq!(idx["componentType"], 5125);
        let count = number(&pos["count"]);
        assert_eq!(number(&nor["count"]), count);
        let floats: Vec<f32> = view_bytes(&g, pos)
            .as_chunks::<4>()
            .0
            .iter()
            .map(|c| f32::from_le_bytes(*c))
            .collect();
        assert_eq!(floats.len(), 3 * count);
        for axis in 0..3 {
            let values: Vec<f32> = floats.iter().skip(axis).step_by(3).copied().collect();
            let min = values.iter().copied().fold(f32::MAX, f32::min);
            let max = values.iter().copied().fold(f32::MIN, f32::max);
            assert_eq!(pos["min"][axis].as_f64().unwrap() as f32, min);
            assert_eq!(pos["max"][axis].as_f64().unwrap() as f32, max);
        }
        let normals: Vec<f32> = view_bytes(&g, nor)
            .as_chunks::<4>()
            .0
            .iter()
            .map(|c| f32::from_le_bytes(*c))
            .collect();
        for n in normals.as_chunks::<3>().0 {
            let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
            assert!((len - 1.0).abs() < 1e-5, "{n:?}");
        }
        let indices: Vec<u32> = view_bytes(&g, idx)
            .as_chunks::<4>()
            .0
            .iter()
            .map(|c| u32::from_le_bytes(*c))
            .collect();
        assert_eq!(indices.len(), number(&idx["count"]));
        assert_eq!(indices.len(), 3 * bodies[k].mesh.triangle_count());
        assert!(indices.iter().all(|&i| (i as usize) < count));
    }
    // Würfel: harte Kanten, drei Punkte je Ecke
    assert_eq!(number(&j["accessors"][0]["count"]), 24);
}

#[test]
fn positions_are_written_in_metres_and_nothing_is_an_error() {
    let g = write(&[ExportBody {
        name: "W".into(),
        mesh: cube(5.0),
    }]);
    let max_x = g.json["accessors"][0]["max"][0].as_f64().unwrap();
    assert!((max_x - 0.006).abs() < 1e-9, "{max_x}");
    assert!((GLTF_SCALE - 0.001).abs() < f64::EPSILON);
    let mut out = Vec::new();
    assert_eq!(write_glb(&mut out, &[]), Err(ExchangeError::Empty));
    let empty = [ExportBody {
        name: "leer".into(),
        mesh: ExchangeMesh::default(),
    }];
    assert_eq!(write_glb(&mut out, &empty), Err(ExchangeError::Empty));
}

#[test]
fn names_are_escaped_for_json() {
    let name = "a\\b\u{1}\"c";
    let g = write(&[ExportBody {
        name: name.into(),
        mesh: tetra(),
    }]);
    assert_eq!(g.json["meshes"][0]["name"], name);
}
