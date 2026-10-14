//! Kopf, Einheiten, Kontext und je Körper ein Teil (Spec §2, E1, E2).

use crate::entity::{Entities, refs};
use crate::error::StepError;
use crate::geometry::{direction, point};
use crate::number::push_real;
use crate::text::step_string;
use planora_math::DVec3;
use std::time::{SystemTime, UNIX_EPOCH};

/// Schema AP214 (Spec E1).
pub const SCHEMA: &str = "AUTOMOTIVE_DESIGN { 1 0 10303 214 1 1 1 1 }";
/// Name des Programms im Kopf.
pub const SYSTEM: &str = "PlanoraCAD";

const SECONDS_PER_DAY: u64 = 86_400;
const SECONDS_PER_HOUR: u64 = 3_600;
const SECONDS_PER_MINUTE: u64 = 60;

/// Datum aus der Zahl der Tage seit 1970-01-01 (Verfahren „civil_from_days“ von Howard
/// Hinnant; die Zahlen sind die des Verfahrens: Tage bis 0000-03-01, Tage je 400 Jahre usw.).
fn civil_from_days(days: i64) -> (i64, u64, u64) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    (year, month as u64, day as u64)
}

/// Zeitpunkt in UTC als `YYYY-MM-DDThh:mm:ss` (Spec §2); vor 1970 gilt 1970-01-01.
pub fn utc_timestamp(t: SystemTime) -> String {
    let secs = t.duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs());
    let days = i64::try_from(secs / SECONDS_PER_DAY).unwrap_or(i64::MAX / 2);
    let rest = secs % SECONDS_PER_DAY;
    let (y, m, d) = civil_from_days(days);
    let (hh, mm, ss) = (
        rest / SECONDS_PER_HOUR,
        rest % SECONDS_PER_HOUR / SECONDS_PER_MINUTE,
        rest % SECONDS_PER_MINUTE,
    );
    format!("{y:04}-{m:02}-{d:02}T{hh:02}:{mm:02}:{ss:02}")
}

/// Kopf bis einschließlich `DATA;`.
pub(crate) fn header(e: &mut Entities, file_name: &str, time: SystemTime) -> Result<(), StepError> {
    let name = step_string(file_name).text;
    let stamp = utc_timestamp(time);
    e.line("ISO-10303-21;")?;
    e.line("HEADER;")?;
    e.line("FILE_DESCRIPTION(('PlanoraCAD Export'),'2;1');")?;
    e.line(&format!(
        "FILE_NAME({name},'{stamp}',(''),(''),'{SYSTEM}','{SYSTEM}','');"
    ))?;
    e.line(&format!("FILE_SCHEMA(('{SCHEMA}'));"))?;
    e.line("ENDSEC;")?;
    e.line("DATA;")
}

/// `ENDSEC;` und Dateiende.
pub(crate) fn trailer(e: &mut Entities) -> Result<(), StepError> {
    e.line("ENDSEC;")?;
    e.line("END-ISO-10303-21;")?;
    e.flush()
}

/// Gemeinsame Einträge aller Teile.
pub(crate) struct Context {
    product: u64,
    definition: u64,
    geometry: u64,
}

/// Anwendungskontext, Einheiten (Millimeter, Radiant, Steradiant) und der geometrische Kontext
/// mit der Unsicherheit `uncertainty` in Millimetern.
pub(crate) fn context(e: &mut Entities, uncertainty: f64) -> Result<Context, StepError> {
    let app =
        e.push("APPLICATION_CONTEXT('core data for automotive mechanical design processes')")?;
    e.push(&format!(
        "APPLICATION_PROTOCOL_DEFINITION('international standard','automotive_design',2000,#{app})"
    ))?;
    let product = e.push(&format!("PRODUCT_CONTEXT('',#{app},'mechanical')"))?;
    let definition = e.push(&format!(
        "PRODUCT_DEFINITION_CONTEXT('part definition',#{app},'design')"
    ))?;
    let length = e.push("(LENGTH_UNIT()NAMED_UNIT(*)SI_UNIT(.MILLI.,.METRE.))")?;
    let angle = e.push("(NAMED_UNIT(*)PLANE_ANGLE_UNIT()SI_UNIT($,.RADIAN.))")?;
    let solid_angle = e.push("(NAMED_UNIT(*)SI_UNIT($,.STERADIAN.)SOLID_ANGLE_UNIT())")?;
    let mut u = String::from("UNCERTAINTY_MEASURE_WITH_UNIT(LENGTH_MEASURE(");
    push_real(&mut u, uncertainty)?;
    u.push_str(&format!(
        "),#{length},'distance_accuracy_value','confusion accuracy')"
    ));
    let uncertainty = e.push(&u)?;
    let geometry = e.push(&format!(
        "(GEOMETRIC_REPRESENTATION_CONTEXT(3)GLOBAL_UNCERTAINTY_ASSIGNED_CONTEXT((#{uncertainty}))\
         GLOBAL_UNIT_ASSIGNED_CONTEXT((#{length},#{angle},#{solid_angle}))\
         REPRESENTATION_CONTEXT('3D','3D'))"
    ))?;
    Ok(Context {
        product,
        definition,
        geometry,
    })
}

/// Ein Teil (Spec E2): Produkt mit Namen, Definition und Form; `items` sind die schon
/// geschriebenen Körper (`MANIFOLD_SOLID_BREP` bzw. `BREP_WITH_VOIDS`).
pub(crate) fn product(
    e: &mut Entities,
    ctx: &Context,
    name: &str,
    items: &[u64],
) -> Result<(), StepError> {
    let o = point(e, DVec3::ZERO)?;
    let z = direction(e, DVec3::Z)?;
    let x = direction(e, DVec3::X)?;
    let origin = e.push(&format!("AXIS2_PLACEMENT_3D('',#{o},#{z},#{x})"))?;
    let mut all = Vec::with_capacity(items.len() + 1);
    all.push(origin);
    all.extend_from_slice(items);
    let shape = e.push(&format!(
        "ADVANCED_BREP_SHAPE_REPRESENTATION({name},{},#{})",
        refs(&all),
        ctx.geometry
    ))?;
    let prod = e.push(&format!("PRODUCT({name},{name},'',(#{}))", ctx.product))?;
    e.push(&format!(
        "PRODUCT_RELATED_PRODUCT_CATEGORY('part',$,(#{prod}))"
    ))?;
    let formation = e.push(&format!("PRODUCT_DEFINITION_FORMATION('','',#{prod})"))?;
    let definition = e.push(&format!(
        "PRODUCT_DEFINITION('design','',#{formation},#{})",
        ctx.definition
    ))?;
    let pds = e.push(&format!("PRODUCT_DEFINITION_SHAPE('','',#{definition})"))?;
    e.push(&format!("SHAPE_DEFINITION_REPRESENTATION(#{pds},#{shape})"))?;
    Ok(())
}
