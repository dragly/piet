//! Text functionality for Piet svg backend

use std::{
    collections::HashSet,
    fs, io,
    ops::RangeBounds,
    sync::{Arc, Mutex},
};

use piet::kurbo::{Point, Rect, Size};
use piet::{
    Color, Error, FontFamily, FontStyle, FontWeight, HitTestPoint, HitTestPosition, LineMetric,
    TextAlignment, TextAttribute, TextStorage,
};
use rustybuzz::{Face, UnicodeBuffer};

type Result<T> = std::result::Result<T, Error>;

/// SVG text (partially implemented)
#[derive(Clone)]
pub struct Text {}

impl Text {
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        Text {}
    }
}

impl piet::Text for Text {
    type TextLayout = TextLayout;
    type TextLayoutBuilder = TextLayoutBuilder;

    fn font_family(&mut self, family_name: &str) -> Option<FontFamily> {
        unimplemented!()
    }

    fn load_font(&mut self, data: &[u8]) -> Result<FontFamily> {
        unimplemented!()
    }

    fn new_text_layout(&mut self, text: impl TextStorage) -> TextLayoutBuilder {
        unimplemented!()
    }
}

pub struct TextLayoutBuilder {}

impl TextLayoutBuilder {
    fn new(text: impl TextStorage, ctx: Text) -> Self {
        unimplemented!()
    }
}

impl piet::TextLayoutBuilder for TextLayoutBuilder {
    type Out = TextLayout;

    fn max_width(mut self, width: f64) -> Self {
        unimplemented!()
    }

    fn alignment(mut self, alignment: piet::TextAlignment) -> Self {
        unimplemented!()
    }

    fn default_attribute(mut self, attribute: impl Into<TextAttribute>) -> Self {
        unimplemented!()
    }

    fn range_attribute(
        mut self,
        range: impl RangeBounds<usize>,
        attribute: impl Into<TextAttribute>,
    ) -> Self {
        unimplemented!()
    }

    fn build(self) -> Result<TextLayout> {
        unimplemented!()
    }
}

/// SVG text layout
#[derive(Clone)]
pub struct TextLayout {}

impl TextLayout {
    fn from_builder(builder: TextLayoutBuilder) -> Result<Self> {
        unimplemented!()
    }
}

impl piet::TextLayout for TextLayout {
    fn size(&self) -> Size {
        unimplemented!()
    }

    fn trailing_whitespace_width(&self) -> f64 {
        unimplemented!()
    }

    fn image_bounds(&self) -> Rect {
        unimplemented!()
    }

    fn line_text(&self, line_number: usize) -> Option<&str> {
        unimplemented!()
    }

    fn line_metric(&self, line_number: usize) -> Option<LineMetric> {
        unimplemented!()
    }

    fn line_count(&self) -> usize {
        unimplemented!()
    }

    fn hit_test_point(&self, _point: Point) -> HitTestPoint {
        unimplemented!()
    }

    fn hit_test_text_position(&self, _text_position: usize) -> HitTestPosition {
        unimplemented!()
    }

    fn text(&self) -> &str {
        unimplemented!()
    }
}

