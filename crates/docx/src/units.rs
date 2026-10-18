//! Lenient number parsing for OOXML attribute values.

use crate::xml::El;

/// Largest absolute length we accept, points (beyond any real page: 200 inches).
pub const MAX_LEN_PT: f32 = 14_400.0;

/// Parse a number, accepting decimals and ISO 29500 universal measures (`1in`, `2.5cm`, `12pt`,
/// `3mm`, `1pc`, `1pi`). `per_pt` is how many file units make a point when there's no unit
/// suffix (20 for twips, 2 for half-points, 8 for eighths, 12700 for EMU). Returns points.
pub fn measure(s: &str, per_pt: f32) -> Option<f32> {
    let t = s.trim();
    if t.is_empty() || t.len() > 40 {
        return None;
    }
    let split = t.find(|c: char| !(c.is_ascii_digit() || c == '.' || c == '-' || c == '+' || c == 'e' || c == 'E')).unwrap_or(t.len());
    let (num, unit) = t.split_at(split);
    let v: f64 = num.parse().ok()?;
    if !v.is_finite() {
        return None;
    }
    let pt = match unit.trim() {
        "" => v / per_pt as f64,
        "pt" => v,
        "in" => v * 72.0,
        "cm" => v * 72.0 / 2.54,
        "mm" => v * 72.0 / 25.4,
        "pc" | "pi" => v * 12.0,
        "px" => v * 0.75,
        "emu" => v / 12_700.0,
        _ => return None,
    };
    let pt = pt as f32;
    pt.is_finite().then_some(pt.clamp(-MAX_LEN_PT, MAX_LEN_PT))
}

/// `w:line` under `lineRule="auto"`: the multiple of single line spacing, in 240ths (240 =
/// single). Without a unit the value already is that — the transitional form. A universal
/// measure is a height (strict OOXML writes `w:line="13.90pt"`, ISO 29500
/// `ST_SignedTwipsMeasure`), so it goes through twips as Word reads it: × 20 ÷ 240 (13.90 pt =
/// 278 twips, the 1.1583 multiple Word renders).
pub fn line_multiple(s: &str) -> Option<f32> {
    let t = s.trim();
    let measured = t.ends_with(|c: char| c.is_ascii_alphabetic());
    measure(t, 240.0).map(|v| if measured { v * 20.0 / 240.0 } else { v })
}

/// Plain integer (accepts decimals, rounds; clamps into i64 range).
pub fn int(s: &str) -> Option<i64> {
    let t = s.trim();
    if t.is_empty() || t.len() > 40 {
        return None;
    }
    if let Ok(v) = t.parse::<i64>() {
        return Some(v);
    }
    let v: f64 = t.parse().ok()?;
    v.is_finite().then(|| v.round().clamp(i64::MIN as f64, i64::MAX as f64) as i64)
}

/// How far below a front object's stacking order a behind-text VML shape's `z-index` is:
/// 0x1E000000, as Word is believed to write it (`relativeHeight` 251658240 behind the text is
/// `z-index:-251658240`; not checked on a Word file).
const VML_BEHIND: i64 = 503_316_480;

/// A floating object's stacking order ([`wordcraft_doc::para::Float::z`]) from a VML `z-index`.
/// Higher is on top, as in CSS, negatives included; 0 is none.
pub fn z_from_vml(z: i64) -> u32 {
    if z < 0 { u32::try_from(z.saturating_add(VML_BEHIND)).unwrap_or(0).max(1) } else { u32::try_from(z).unwrap_or(u32::MAX) }
}

/// The VML `z-index` of stacking order `z` (not 0) for an object behind the text or not: the
/// inverse of [`z_from_vml`].
pub fn vml_z_index(z: u32, behind: bool) -> i64 {
    if behind { i64::from(z) - VML_BEHIND } else { i64::from(z) }
}

pub fn u32_of(s: &str) -> Option<u32> {
    int(s).map(|v| v.clamp(0, u32::MAX as i64) as u32)
}

/// Twips attribute → points.
pub fn tw(e: &El, attr: &str) -> Option<f32> {
    e.attr(attr).and_then(|v| measure(v, 20.0))
}

/// ST_OnOff: absent `w:val` = on; `0`, `false`, `off` = off.
pub fn on_off(e: &El) -> bool {
    match e.attr("w:val") {
        None => true,
        Some(v) => !matches!(v.trim(), "0" | "false" | "off" | "False" | "FALSE"),
    }
}

/// An on/off child of `parent`, if present.
pub fn flag(parent: &El, name: &str) -> Option<bool> {
    parent.child(name).map(on_off)
}

/// Format a number for an attribute: integers without a fraction.
pub fn n(v: i64) -> String {
    v.to_string()
}

/// Points → twips string.
pub fn twips(pt: f32) -> String {
    wordcraft_geom::to_twips(pt.clamp(-MAX_LEN_PT, MAX_LEN_PT)).to_string()
}

/// Points → EMU string (never negative for sizes).
pub fn emu(pt: f32) -> String {
    wordcraft_geom::to_emu(pt.clamp(-MAX_LEN_PT, MAX_LEN_PT)).to_string()
}

#[cfg(test)]
mod tests {
    /// #2956: VML `z-index` stacks as in CSS (higher on top, negatives included) among the
    /// objects behind the text and among the others, in the same scale as DrawingML's
    /// `relativeHeight`, and goes back to the same `z-index`.
    #[test]
    fn vml_z_index_keeps_its_order() {
        for order in [&[-503_316_479, -251_658_240, -251_657_216, -2, -1][..], &[1, 2, 251_658_240, 251_659_264, i64::from(u32::MAX)]] {
            let z: Vec<u32> = order.iter().map(|v| z_from_vml(*v)).collect();
            assert!(z.windows(2).all(|w| w[0] < w[1]), "{order:?} → {z:?}");
        }
        assert_eq!(z_from_vml(-251_658_240), 251_658_240, "Word's behind-text order");
        assert_eq!((z_from_vml(i64::MIN), z_from_vml(0), z_from_vml(i64::MAX)), (1, 0, u32::MAX));
        for v in [-251_657_216, -1, 5, 251_659_264] {
            assert_eq!(vml_z_index(z_from_vml(v), v < 0), v);
        }
    }

    use super::*;

    #[test]
    fn measures() {
        assert_eq!(measure("1440", 20.0), Some(72.0));
        assert_eq!(measure("1in", 20.0), Some(72.0));
        assert_eq!(measure("12pt", 2.0), Some(12.0));
        assert_eq!(measure("24.0", 2.0), Some(12.0));
        assert_eq!(measure("nan", 20.0), None);
        assert_eq!(measure("1e400", 20.0), None);
        assert_eq!(measure("99999999999999", 20.0), Some(MAX_LEN_PT));
        assert_eq!(measure("x", 20.0), None);
        // `w:line` auto: the multiple in 240ths without a unit, through twips with one (#2329).
        assert_eq!(line_multiple("240"), Some(1.0));
        assert_eq!(line_multiple("276"), Some(1.15));
        assert_eq!(line_multiple("0"), Some(0.0));
        let strict = line_multiple("13.90pt").expect("strict line height");
        assert!((strict - 278.0 / 240.0).abs() < 1e-5, "{strict}");
        let inches = line_multiple("1.2in").expect("inches");
        assert!((inches - 7.2).abs() < 1e-5, "{inches}");
        assert_eq!(line_multiple("junk"), None);
        assert_eq!(line_multiple(""), None);
        assert_eq!(int("12.6"), Some(13));
        assert_eq!(u32_of("-5"), Some(0));
    }
}
