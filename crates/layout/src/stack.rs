//! Stacking order. Floating objects are drawn in two layers, behind the text and in front of it.
//! Within a layer they go bottom to top by their stacking order (Word's `relativeHeight`), then
//! by document order. Layout marks each floating object's items with [`Placed::StackStart`] and
//! [`Placed::StackEnd`]; [`order`] says in which order to draw a list of items.

use wordcraft_doc::Pos;
use wordcraft_doc::para::{Float, Wrap};

use crate::Placed;

/// Where a floating object stacks: its layer, its stacking order, then its place in the document.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct StackKey {
    /// In front of the text (every wrap but Behind Text).
    pub front: bool,
    /// Markup beside the page (comment balloons and their leaders): above every object.
    pub markup: bool,
    /// [`Float::stack_key`].
    pub z: (bool, u32),
    /// The object (its U+FFFC).
    pub at: Pos,
}

impl StackKey {
    pub fn new(float: &Float, at: Pos) -> StackKey {
        StackKey { front: float.wrap != Wrap::BehindText, markup: false, z: float.stack_key(), at }
    }
    /// The key of the markup drawn over a page ([`StackKey::markup`]).
    pub fn markup() -> StackKey {
        StackKey { front: true, markup: true, z: (false, 0), at: Pos::default() }
    }
}

/// The indices of `items` in drawing order: the floating objects behind the text bottom to top,
/// the other items in list order, then the floating objects in front of the text bottom to top,
/// then the markup beside the page. Each object's items stay together and in their own order (a
/// text box's text with its box).
pub fn order(items: &[Placed]) -> Vec<usize> {
    if !items.iter().any(|it| matches!(it, Placed::StackStart { .. })) {
        return (0..items.len()).collect();
    }
    // The end matching each start (an unmatched start counts as an ordinary item).
    let mut ends = vec![None; items.len()];
    let mut open = Vec::new();
    for (i, it) in items.iter().enumerate() {
        match it {
            Placed::StackStart { .. } => open.push(i),
            Placed::StackEnd => {
                if let Some(s) = open.pop()
                    && let Some(e) = ends.get_mut(s)
                {
                    *e = Some(i);
                }
            }
            _ => {}
        }
    }
    let mut runs: Vec<(&StackKey, std::ops::Range<usize>)> = Vec::new();
    let mut text = Vec::with_capacity(items.len());
    let mut i = 0;
    while i < items.len() {
        match (items.get(i), ends.get(i).copied().flatten()) {
            (Some(Placed::StackStart { key }), Some(end)) => {
                runs.push((key, i..end + 1));
                i = end + 1;
            }
            _ => {
                text.push(i);
                i += 1;
            }
        }
    }
    runs.sort_by(|a, b| a.0.cmp(b.0));
    let mut out = Vec::with_capacity(items.len());
    out.extend(runs.iter().filter(|(k, _)| !k.front).flat_map(|(_, r)| r.clone()));
    out.extend(text);
    out.extend(runs.iter().filter(|(k, _)| k.front).flat_map(|(_, r)| r.clone()));
    out
}

/// `items` in drawing order (see [`order`]).
pub fn stacked(items: &[Placed]) -> impl Iterator<Item = &Placed> + '_ {
    order(items).into_iter().filter_map(|i| items.get(i))
}

/// `items` topmost first: drawing order backwards, for hit testing.
pub fn topmost_first(items: &[Placed]) -> impl Iterator<Item = &Placed> + '_ {
    order(items).into_iter().rev().filter_map(|i| items.get(i))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::display::{DisplayOptions, Draw, page_display};
    use crate::{DocLayout, LayoutCache, LayoutOptions, layout};
    use wordcraft_doc::para::{Anchor, InlineObject};
    use wordcraft_doc::props::Rgb;
    use wordcraft_doc::{Document, StoryRef};

    const RED: Rgb = Rgb(255, 0, 0);
    const BLUE: Rgb = Rgb(0, 0, 255);
    const GREEN: Rgb = Rgb(0, 128, 0);
    const GOLD: Rgb = Rgb(255, 200, 0);

    fn shape(fill: Rgb, x: f32, wrap: Wrap, z: u32) -> InlineObject {
        let float = Float { wrap, h_rel: Anchor::Page, v_rel: Anchor::Page, x, y: 72.0, z, ..Default::default() };
        let mut o = crate::tests::rect_shape(100.0, 100.0, float);
        if let InlineObject::Shape { fill: f, .. } = &mut o {
            *f = Some(fill);
        }
        o
    }

    /// Red and blue in front of the text in the first paragraph, green and gold behind it in the
    /// second, each pair overlapping, with stacking orders `z` (red, blue, green, gold).
    fn doc(z: [u32; 4]) -> Document {
        let mut d = Document::from_text("First paragraph, over the shapes.\nSecond paragraph.");
        let obj = wordcraft_doc::para::OBJ.len_utf8();
        let objects = [
            (0, RED, 72.0, Wrap::InFrontOfText),
            (0, BLUE, 122.0, Wrap::InFrontOfText),
            (1, GREEN, 300.0, Wrap::BehindText),
            (1, GOLD, 350.0, Wrap::BehindText),
        ];
        for (i, (block, fill, x, wrap)) in objects.into_iter().enumerate() {
            let off = if i % 2 == 0 { 0 } else { obj };
            d.insert_object(&Pos::body(block, off), shape(fill, x, wrap, z[i]), &Default::default()).unwrap();
        }
        d
    }

    fn lay(d: &Document) -> DocLayout {
        layout(d, &mut LayoutCache::new(), &LayoutOptions::default())
    }

    /// What the page draws, in order: each shape's fill, and "text" for a run of letters.
    fn drawn(d: &Document) -> Vec<String> {
        let l = lay(d);
        let mut out: Vec<String> = Vec::new();
        for it in page_display(d, &l.pages[0], &DisplayOptions::default()) {
            let name = match it {
                Draw::Shape { fill: Some(c), .. } if c == RED => "red",
                Draw::Shape { fill: Some(c), .. } if c == BLUE => "blue",
                Draw::Shape { fill: Some(c), .. } if c == GREEN => "green",
                Draw::Shape { fill: Some(c), .. } if c == GOLD => "gold",
                Draw::Glyphs { .. } => "text",
                _ => continue,
            };
            if out.last().map(String::as_str) != Some(name) {
                out.push(name.to_string());
            }
        }
        out
    }

    /// #2956: behind the text bottom to top, then the text, then in front of it bottom to top;
    /// objects without a stacking order go by document order, above those with one.
    #[test]
    fn floats_are_drawn_in_stacking_order() {
        assert_eq!(drawn(&doc([0; 4])), ["green", "gold", "text", "red", "blue"], "document order");
        assert_eq!(drawn(&doc([2, 1, 4, 3])), ["gold", "green", "text", "blue", "red"]);
        // No order of its own stacks above one that has one.
        assert_eq!(drawn(&doc([0, 5, 0, 9])), ["gold", "green", "text", "blue", "red"]);
        // A high order doesn't lift an object out of its layer.
        assert_eq!(drawn(&doc([1, 2, 99, 98])), ["gold", "green", "text", "red", "blue"]);
    }

    /// #2956: a press where objects overlap grabs the one drawn on top.
    #[test]
    fn the_topmost_object_is_hit() {
        let at = |d: &Document| {
            let l = lay(d);
            let hit = l.object_at(0, 150.0, 120.0, 0.0).unwrap();
            d.para(StoryRef::Body, &hit.path).and_then(|p| p.object_at(hit.off)).and_then(|o| match o {
                InlineObject::Shape { fill, .. } => *fill,
                _ => None,
            })
        };
        assert_eq!(at(&doc([0; 4])), Some(BLUE), "the later one, in document order");
        assert_eq!(at(&doc([2, 1, 0, 0])), Some(RED), "the higher order");
        assert_eq!(at(&doc([1, 0, 0, 0])), Some(BLUE), "no order of its own is on top");
    }

    /// #2956: an object floating in a text box in line with the text is drawn with the box, over
    /// its fill, even behind the box's text: the box's line carries it, not the page's layers.
    #[test]
    fn floats_in_an_inline_text_box_stay_with_it() {
        let mut d = Document::from_text("Before the box.");
        let mut inner = wordcraft_doc::Paragraph::with_text("Box text", Default::default());
        inner.insert_object(0, shape(RED, 0.0, Wrap::BehindText, 0), &Default::default()).unwrap();
        let id = d.add_part(wordcraft_doc::PartKind::TextBox, vec![wordcraft_doc::para_block(inner)]);
        let float = Float { wrap: Wrap::Inline, ..Default::default() };
        let mut tb = crate::tests::rect_shape(200.0, 120.0, float);
        if let InlineObject::Shape { kind, fill, story, .. } = &mut tb {
            *kind = wordcraft_doc::para::ShapeKind::TextBox;
            *fill = Some(GOLD);
            *story = Some(id);
        }
        d.insert_object(&Pos::body(0, 0), tb, &Default::default()).unwrap();
        let l = lay(&d);
        assert!(!l.pages[0].items.iter().any(|it| matches!(it, Placed::StackStart { .. })), "no layer of its own");
        let names = drawn(&d);
        let (gold, red) = (names.iter().position(|n| n == "gold"), names.iter().position(|n| n == "red"));
        assert!(gold.is_some() && gold < red, "{names:?}");
    }

    /// Marks that don't pair up (a split the layout didn't expect) neither panic nor lose items.
    #[test]
    fn unbalanced_marks_keep_every_item() {
        let start = |front: bool| Placed::StackStart { key: Box::new(StackKey { front, markup: false, z: (false, 1), at: Pos::body(0, 0) }) };
        let fill = || Placed::Fill { rect: wordcraft_geom::Rect::new(0.0, 0.0, 1.0, 1.0), color: RED };
        let lists = [
            vec![Placed::StackEnd, fill(), start(true)],
            vec![start(true), start(false), fill(), Placed::StackEnd],
            vec![start(true), fill(), Placed::StackEnd, Placed::StackEnd, start(false), fill()],
            vec![fill(), start(true), fill(), Placed::StackEnd, start(false), fill(), Placed::StackEnd, fill()],
        ];
        for items in &lists {
            let mut o = order(items);
            o.sort_unstable();
            assert_eq!(o, (0..items.len()).collect::<Vec<_>>());
        }
        // Paired up: the object behind first, the one in front last, each with its marks.
        assert_eq!(order(&lists[3]), [4, 5, 6, 0, 7, 1, 2, 3]);
    }
}
