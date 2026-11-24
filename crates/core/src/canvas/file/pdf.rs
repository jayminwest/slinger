//! A `pdf` item's fields as the file reads them (docs/design/pdf.md).

use serde_json::Value;

use super::json::{Bad, Fields, field, int, opt_text, text, units};
use crate::canvas::{Rect, Shape};

/// A `pdf` item's fields (docs/design/pdf.md). Values the canvas would
/// refuse (a page 0, a crop off the page) are left for [`Shape::check`].
pub(super) fn read(m: &Fields) -> Result<Shape, Bad> {
    let at = |key| int(m, key).map(units);
    let frame = Rect::new(at("x")?, at("y")?, at("w")?, at("h")?);
    let pdf_page =
        u32::try_from(int(m, "pdfpage")?).map_err(|_| "`pdfpage` is not a page number")?;
    let [pw, ph] = tenths_list::<2>(m, "pagesize")?;
    let crop = match m.get("crop") {
        None | Some(Value::Null) => None,
        Some(_) => {
            let [left, top, width, height] = tenths_list::<4>(m, "crop")?;
            Some(Rect::new(left, top, width, height))
        }
    };
    Ok(Shape::Pdf {
        frame,
        hash: text(m, "hash")?.to_owned(),
        pdf_page,
        page_size: (pw, ph),
        crop,
        name: opt_text(m, "name")?.map(str::to_owned),
    })
}

/// A list of exactly `N` integers in tenths, in units.
fn tenths_list<const N: usize>(m: &Fields, key: &str) -> Result<[f64; N], Bad> {
    let bad = || format!("`{key}` is not a list of {N} integers");
    let list = field(m, key)?.as_array().ok_or_else(bad)?;
    let values: Vec<f64> =
        list.iter().map(|v| v.as_i64().map(units)).collect::<Option<_>>().ok_or_else(bad)?;
    values.try_into().map_err(|_| bad())
}
