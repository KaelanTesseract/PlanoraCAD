//! 3MF schreiben (Spec §3, Plan P7): ZIP mit `[Content_Types].xml`, `_rels/.rels` und
//! `3D/3dmodel.model`; ein `object` je Körper (`type="model"`), ein `item` je Objekt,
//! Millimeter, Punkte mit sechs Nachkommastellen.

use crate::error::ExchangeError;
use crate::source::ExportBody;
use crate::zip::ZipWriter;
use std::io::{self, Write};

const CONTENT_TYPES: &str = concat!(
    r#"<?xml version="1.0" encoding="UTF-8"?>"#,
    "\n",
    r#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">"#,
    r#"<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>"#,
    r#"<Default Extension="model" ContentType="application/vnd.ms-package.3dmanufacturing-3dmodel+xml"/>"#,
    "</Types>\n"
);

const RELS: &str = concat!(
    r#"<?xml version="1.0" encoding="UTF-8"?>"#,
    "\n",
    r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">"#,
    r#"<Relationship Target="/3D/3dmodel.model" Id="rel0" Type="http://schemas.microsoft.com/3dmanufacturing/2013/01/3dmodel"/>"#,
    "</Relationships>\n"
);

/// Schreibt alle Körper mit Dreiecken als 3MF; Körper ohne Dreiecke entfallen.
pub fn write_3mf<W: Write>(w: &mut W, bodies: &[ExportBody]) -> Result<(), ExchangeError> {
    let used: Vec<&ExportBody> = bodies
        .iter()
        .filter(|b| b.mesh.triangle_count() > 0)
        .collect();
    if used.is_empty() {
        return Err(ExchangeError::Empty);
    }
    let mut zip = ZipWriter::new(w);
    zip.add_entry("[Content_Types].xml", &|o| {
        o.write_all(CONTENT_TYPES.as_bytes())
    })?;
    zip.add_entry("_rels/.rels", &|o| o.write_all(RELS.as_bytes()))?;
    zip.add_entry("3D/3dmodel.model", &|o| write_model(o, &used))?;
    zip.finish()?;
    Ok(())
}

fn write_model(out: &mut dyn Write, bodies: &[&ExportBody]) -> io::Result<()> {
    let mut o = io::BufWriter::new(out);
    writeln!(o, r#"<?xml version="1.0" encoding="UTF-8"?>"#)?;
    writeln!(
        o,
        r#"<model unit="millimeter" xml:lang="de-DE" xmlns="http://schemas.microsoft.com/3dmanufacturing/core/2015/02">"#
    )?;
    writeln!(o, r#"<metadata name="Application">PlanoraCAD</metadata>"#)?;
    writeln!(o, "<resources>")?;
    for (k, body) in bodies.iter().enumerate() {
        writeln!(
            o,
            r#"<object id="{}" type="model" name="{}">"#,
            k + 1,
            xml_escape(&body.name)
        )?;
        writeln!(o, "<mesh>")?;
        writeln!(o, "<vertices>")?;
        for p in &body.mesh.positions {
            writeln!(
                o,
                r#"<vertex x="{:.6}" y="{:.6}" z="{:.6}"/>"#,
                f64::from(p[0]),
                f64::from(p[1]),
                f64::from(p[2])
            )?;
        }
        writeln!(o, "</vertices>")?;
        writeln!(o, "<triangles>")?;
        for t in body.mesh.indices.as_chunks::<3>().0 {
            writeln!(
                o,
                r#"<triangle v1="{}" v2="{}" v3="{}"/>"#,
                t[0], t[1], t[2]
            )?;
        }
        writeln!(o, "</triangles>")?;
        writeln!(o, "</mesh>")?;
        writeln!(o, "</object>")?;
    }
    writeln!(o, "</resources>")?;
    writeln!(o, "<build>")?;
    for k in 1..=bodies.len() {
        writeln!(o, r#"<item objectid="{k}"/>"#)?;
    }
    writeln!(o, "</build>")?;
    writeln!(o, "</model>")?;
    o.flush()
}

/// Maskiert Text für XML-Attribute; Steuerzeichen (in XML 1.0 verboten) werden Leerzeichen.
fn xml_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            c if c.is_control() => out.push(' '),
            c => out.push(c),
        }
    }
    out
}
