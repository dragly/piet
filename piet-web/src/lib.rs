// allows e.g. raw_data[dst_off + x * 4 + 2] = buf[src_off + x * 4 + 0];
#![allow(clippy::identity_op)]
#![deny(clippy::trivially_copy_pass_by_ref)]

//! The Web Canvas backend for the Piet 2D graphics abstraction.

mod text;

use std::borrow::Cow;
use std::fmt;
use std::marker::PhantomData;
use std::ops::Deref;

use js_sys::{Float64Array, Reflect};
use wasm_bindgen::{Clamped, JsCast, JsValue};
use web_sys::{
    CanvasGradient, CanvasRenderingContext2d, CanvasWindingRule, DomMatrix, HtmlCanvasElement,
    ImageData, Window,
};

use piet::kurbo::{Affine, PathEl, Point, Rect, Shape, Size};

use piet::util::unpremul;
use piet::{
    Color, Error, FixedGradient, GradientStop, Image, ImageFormat, InterpolationMode, IntoBrush,
    LineCap, LineJoin, RenderContext, StrokeDash, StrokeStyle,
};

pub use text::{WebFont, WebTextLayout, WebTextLayoutBuilder};

pub struct WebRenderContext<'a> {
    ctx: CanvasRenderingContext2d,
    /// Used for creating image bitmaps and possibly other resources.
    window: Window,
    text: WebText,
    err: Result<(), Error>,
    canvas_states: Vec<CanvasState>,
    _phantom: PhantomData<&'a ()>,
}

impl WebRenderContext<'_> {
    pub fn new(ctx: CanvasRenderingContext2d, window: Window) -> WebRenderContext<'static> {
        WebRenderContext {
            ctx: ctx.clone(),
            window,
            text: WebText::new(ctx),
            err: Ok(()),
            canvas_states: vec![CanvasState::default()],
            _phantom: PhantomData,
        }
    }
}

#[derive(Clone)]
struct CanvasState {
    line_cap: LineCap,
    line_dash: StrokeDash,
    line_dash_offset: f64,
    line_join: LineJoin,
    line_width: f64,
}

impl Default for CanvasState {
    /// Returns the default canvas state according to the Canvas API.
    fn default() -> CanvasState {
        CanvasState {
            // https://developer.mozilla.org/en-US/docs/Web/API/CanvasRenderingContext2D/lineCap#value
            line_cap: LineCap::Butt,
            // https://developer.mozilla.org/en-US/docs/Web/API/CanvasRenderingContext2D/setLineDash
            line_dash: StrokeDash::default(),
            // https://developer.mozilla.org/en-US/docs/Web/API/CanvasRenderingContext2D/lineDashOffset#value
            line_dash_offset: 0.,
            // https://developer.mozilla.org/en-US/docs/Web/API/CanvasRenderingContext2D/lineJoin#value
            // https://developer.mozilla.org/en-US/docs/Web/API/CanvasRenderingContext2D/miterLimit#value
            line_join: LineJoin::Miter { limit: 10. },
            // https://developer.mozilla.org/en-US/docs/Web/API/CanvasRenderingContext2D/lineWidth#value
            line_width: 1.,
        }
    }
}

#[derive(Clone)]
pub struct WebText {
    ctx: CanvasRenderingContext2d,
}

impl WebText {
    pub fn new(ctx: CanvasRenderingContext2d) -> WebText {
        WebText { ctx }
    }
}

#[derive(Clone)]
pub enum Brush {
    Solid(u32),
    Gradient(CanvasGradient),
}

#[derive(Clone)]
pub struct WebImage {
    /// We use a canvas element for now, but could be ImageData or ImageBitmap,
    /// so consider an enum.
    inner: HtmlCanvasElement,
    width: u32,
    height: u32,
}

#[derive(Debug)]
struct WrappedJs(JsValue);

trait WrapError<T> {
    fn wrap(self) -> Result<T, Error>;
}

impl std::error::Error for WrappedJs {}

impl fmt::Display for WrappedJs {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Canvas error: {:?}", self.0)
    }
}

// Discussion question: a blanket impl here should be pretty doable.

impl<T> WrapError<T> for Result<T, JsValue> {
    fn wrap(self) -> Result<T, Error> {
        self.map_err(|e| {
            let e: Box<dyn std::error::Error> = Box::new(WrappedJs(e));
            e.into()
        })
    }
}

fn convert_line_cap(line_cap: LineCap) -> &'static str {
    match line_cap {
        LineCap::Butt => "butt",
        LineCap::Round => "round",
        LineCap::Square => "square",
    }
}

fn convert_line_join(line_join: LineJoin) -> &'static str {
    match line_join {
        LineJoin::Miter { .. } => "miter",
        LineJoin::Round => "round",
        LineJoin::Bevel => "bevel",
    }
}

fn convert_dash_pattern(pattern: &[f64]) -> Float64Array {
    let len = pattern.len() as u32;
    let array = Float64Array::new_with_length(len);
    for (i, elem) in pattern.iter().enumerate() {
        Reflect::set(
            array.as_ref(),
            &JsValue::from(i as u32),
            &JsValue::from(*elem),
        )
        .unwrap();
    }
    array
}

impl RenderContext for WebRenderContext<'_> {
    /// wasm-bindgen doesn't have a native Point type, so use kurbo's.
    type Brush = Brush;

    type Text = WebText;
    type TextLayout = WebTextLayout;

    type Image = WebImage;

    fn status(&mut self) -> Result<(), Error> {
        std::mem::replace(&mut self.err, Ok(()))
    }

    fn clear(&mut self, region: impl Into<Option<Rect>>, color: Color) {
        let (width, height) = match self.ctx.canvas() {
            Some(canvas) => (canvas.offset_width(), canvas.offset_height()),
            None => return,
            /* Canvas might be null if the dom node is not in
             * the document; do nothing. */
        };
        let rect = region
            .into()
            .unwrap_or_else(|| Rect::new(0.0, 0.0, width as f64, height as f64));
        let brush = self.solid_brush(color);
        self.fill(rect, &brush);
    }

    fn solid_brush(&mut self, color: Color) -> Brush {
        Brush::Solid(color.as_rgba_u32())
    }

    fn gradient(&mut self, gradient: impl Into<FixedGradient>) -> Result<Brush, Error> {
        match gradient.into() {
            FixedGradient::Linear(linear) => {
                let (x0, y0) = (linear.start.x, linear.start.y);
                let (x1, y1) = (linear.end.x, linear.end.y);
                let mut lg = self.ctx.create_linear_gradient(x0, y0, x1, y1);
                set_gradient_stops(&mut lg, &linear.stops);
                Ok(Brush::Gradient(lg))
            }
            FixedGradient::Radial(radial) => {
                let (xc, yc) = (radial.center.x, radial.center.y);
                let (xo, yo) = (radial.origin_offset.x, radial.origin_offset.y);
                let r = radial.radius;
                let mut rg = self
                    .ctx
                    .create_radial_gradient(xc + xo, yc + yo, 0.0, xc, yc, r)
                    .wrap()?;
                set_gradient_stops(&mut rg, &radial.stops);
                Ok(Brush::Gradient(rg))
            }
        }
    }

    fn fill(&mut self, shape: impl Shape, brush: &impl IntoBrush<Self>) {
        let brush = brush.make_brush(self, || shape.bounding_box());
        self.set_path(shape);
        self.set_brush(&brush, true);
        self.ctx
            .fill_with_canvas_winding_rule(CanvasWindingRule::Nonzero);
    }

    fn fill_even_odd(&mut self, shape: impl Shape, brush: &impl IntoBrush<Self>) {
        let brush = brush.make_brush(self, || shape.bounding_box());
        self.set_path(shape);
        self.set_brush(&brush, true);
        self.ctx
            .fill_with_canvas_winding_rule(CanvasWindingRule::Evenodd);
    }

    fn clip(&mut self, shape: impl Shape) {
        self.set_path(shape);
        self.ctx
            .clip_with_canvas_winding_rule(CanvasWindingRule::Nonzero);
    }

    fn stroke(&mut self, shape: impl Shape, brush: &impl IntoBrush<Self>, width: f64) {
        let brush = brush.make_brush(self, || shape.bounding_box());
        self.set_path(shape);
        self.set_stroke(width, None);
        self.set_brush(brush.deref(), false);
        self.ctx.stroke();
    }

    fn stroke_styled(
        &mut self,
        shape: impl Shape,
        brush: &impl IntoBrush<Self>,
        width: f64,
        style: &StrokeStyle,
    ) {
        let brush = brush.make_brush(self, || shape.bounding_box());
        self.set_path(shape);
        self.set_stroke(width, Some(style));
        self.set_brush(brush.deref(), false);
        self.ctx.stroke();
    }

    fn text(&mut self) -> &mut Self::Text {
        &mut self.text
    }

    fn draw_text(&mut self, layout: &Self::TextLayout, pos: impl Into<Point>) {
        // TODO: bounding box for text
        self.ctx.save();
        self.ctx.set_font(&layout.font.get_font_string());
        let color = layout.color();
        let brush = color.make_brush(self, || layout.size().to_rect());
        self.set_brush(&brush, true);
        let pos = pos.into();
        for lm in &layout.line_metrics {
            let line_text = &layout.text[lm.range()];
            let line_y = lm.y_offset + lm.baseline + pos.y;
            let draw_line = self.ctx.fill_text(line_text, pos.x, line_y).wrap();

            if let Err(e) = draw_line {
                self.err = Err(e);
            }
        }
        self.ctx.restore();
    }

    fn save(&mut self) -> Result<(), Error> {
        self.ctx.save();
        self.canvas_states
            .push(self.canvas_states.last().unwrap().clone());
        Ok(())
    }

    fn restore(&mut self) -> Result<(), Error> {
        // restore state only if there is a state to restore
        if self.canvas_states.len() > 1 {
            self.canvas_states.pop();
            self.ctx.restore();
        }
        Ok(())
    }

    fn finish(&mut self) -> Result<(), Error> {
        self.status()
    }

    fn transform(&mut self, transform: Affine) {
        let a = transform.as_coeffs();
        let _ = self.ctx.transform(a[0], a[1], a[2], a[3], a[4], a[5]);
    }

    fn current_transform(&self) -> Affine {
        matrix_to_affine(self.ctx.get_transform().unwrap())
    }

    fn make_image(
        &mut self,
        width: usize,
        height: usize,
        buf: &[u8],
        format: ImageFormat,
    ) -> Result<Self::Image, Error> {
        let document = self.window.document().unwrap();
        let element = document.create_element("canvas").unwrap();
        let canvas = element.dyn_into::<HtmlCanvasElement>().unwrap();
        canvas.set_width(width as u32);
        canvas.set_height(height as u32);
        let mut new_buf: Vec<u8>;
        let buf = match format {
            ImageFormat::RgbaSeparate => buf,
            ImageFormat::RgbaPremul => {
                new_buf = vec![0; width * height * 4];
                for i in 0..width * height {
                    let a = buf[i * 4 + 3];
                    new_buf[i * 4 + 0] = unpremul(buf[i * 4 + 0], a);
                    new_buf[i * 4 + 1] = unpremul(buf[i * 4 + 1], a);
                    new_buf[i * 4 + 2] = unpremul(buf[i * 4 + 2], a);
                    new_buf[i * 4 + 3] = a;
                }
                new_buf.as_slice()
            }
            ImageFormat::Rgb => {
                new_buf = vec![0; width * height * 4];
                for i in 0..width * height {
                    new_buf[i * 4 + 0] = buf[i * 3 + 0];
                    new_buf[i * 4 + 1] = buf[i * 3 + 1];
                    new_buf[i * 4 + 2] = buf[i * 3 + 2];
                    new_buf[i * 4 + 3] = 255;
                }
                new_buf.as_slice()
            }
            ImageFormat::Grayscale => {
                new_buf = vec![0; width * height * 4];
                for i in 0..width * height {
                    new_buf[i * 4 + 0] = buf[i];
                    new_buf[i * 4 + 1] = buf[i];
                    new_buf[i * 4 + 2] = buf[i];
                    new_buf[i * 4 + 3] = 255;
                }
                new_buf.as_slice()
            }
            _ => &[],
        };

        let image_data = ImageData::new_with_u8_clamped_array(Clamped(buf), width as u32).wrap()?;
        let context = canvas
            .get_context("2d")
            .unwrap()
            .unwrap()
            .dyn_into::<web_sys::CanvasRenderingContext2d>()
            .unwrap();
        context.put_image_data(&image_data, 0.0, 0.0).wrap()?;
        Ok(WebImage {
            inner: canvas,
            width: width as u32,
            height: height as u32,
        })
    }

    #[inline]
    fn draw_image(
        &mut self,
        image: &Self::Image,
        dst_rect: impl Into<Rect>,
        interp: InterpolationMode,
    ) {
        draw_image(self, image, None, dst_rect.into(), interp);
    }

    #[inline]
    fn draw_image_area(
        &mut self,
        image: &Self::Image,
        src_rect: impl Into<Rect>,
        dst_rect: impl Into<Rect>,
        interp: InterpolationMode,
    ) {
        draw_image(self, image, Some(src_rect.into()), dst_rect.into(), interp);
    }

    fn capture_image_area(&mut self, _rect: impl Into<Rect>) -> Result<Self::Image, Error> {
        Err(Error::Unimplemented)
    }

    fn blurred_rect(&mut self, rect: Rect, blur_radius: f64, brush: &impl IntoBrush<Self>) {
        let brush = brush.make_brush(self, || rect);
        self.ctx.set_shadow_blur(blur_radius);
        let color = match *brush {
            Brush::Solid(rgba) => format_color(rgba),
            // Gradients not yet implemented.
            Brush::Gradient(_) => "#f0f".into(),
        };
        self.ctx.set_shadow_color(&color);
        self.ctx
            .fill_rect(rect.x0, rect.y0, rect.width(), rect.height());
        self.ctx.set_shadow_color("none");
    }
}

fn draw_image(
    ctx: &mut WebRenderContext,
    image: &<WebRenderContext as RenderContext>::Image,
    src_rect: Option<Rect>,
    dst_rect: Rect,
    _interp: InterpolationMode,
) {
    let result = ctx.with_save(|rc| {
        // TODO: Implement InterpolationMode::NearestNeighbor in software
        //       See for inspiration http://phrogz.net/tmp/canvas_image_zoom.html
        let src_rect = match src_rect {
            Some(src_rect) => src_rect,
            None => Rect::new(0.0, 0.0, image.width as f64, image.height as f64),
        };
        rc.ctx
            .draw_image_with_html_canvas_element_and_sw_and_sh_and_dx_and_dy_and_dw_and_dh(
                &image.inner,
                src_rect.x0,
                src_rect.y0,
                src_rect.width(),
                src_rect.height(),
                dst_rect.x0,
                dst_rect.y0,
                dst_rect.width(),
                dst_rect.height(),
            )
            .wrap()
    });
    if let Err(e) = result {
        ctx.err = Err(e);
    }
}

impl IntoBrush<WebRenderContext<'_>> for Brush {
    fn make_brush<'b>(
        &'b self,
        _piet: &mut WebRenderContext,
        _bbox: impl FnOnce() -> Rect,
    ) -> std::borrow::Cow<'b, Brush> {
        Cow::Borrowed(self)
    }
}

impl Image for WebImage {
    fn size(&self) -> Size {
        Size::new(self.width.into(), self.height.into())
    }
}

fn format_color(rgba: u32) -> String {
    let rgb = rgba >> 8;
    let a = rgba & 0xff;
    if a == 0xff {
        format!("#{:06x}", rgba >> 8)
    } else {
        format!(
            "rgba({},{},{},{:.3})",
            (rgb >> 16) & 0xff,
            (rgb >> 8) & 0xff,
            rgb & 0xff,
            byte_to_frac(a)
        )
    }
}

fn set_gradient_stops(dst: &mut CanvasGradient, src: &[GradientStop]) {
    for stop in src {
        // TODO: maybe get error?
        let rgba = stop.color.as_rgba_u32();
        let _ = dst.add_color_stop(stop.pos, &format_color(rgba));
    }
}

impl WebRenderContext<'_> {
    /// Set the source pattern to the brush.
    ///
    /// Web canvas is super stateful, and we're trying to have more retained stuff.
    /// This is part of the impedance matching.
    fn set_brush(&mut self, brush: &Brush, is_fill: bool) {
        let value = self.brush_value(brush);
        if is_fill {
            self.ctx.set_fill_style(&value);
        } else {
            self.ctx.set_stroke_style(&value);
        }
    }

    fn brush_value(&self, brush: &Brush) -> JsValue {
        match *brush {
            Brush::Solid(rgba) => JsValue::from_str(&format_color(rgba)),
            Brush::Gradient(ref gradient) => JsValue::from(gradient),
        }
    }

    /// Set the stroke parameters.
    fn set_stroke(&mut self, width: f64, style: Option<&StrokeStyle>) {
        let default_style = StrokeStyle::default();
        let style = style.unwrap_or(&default_style);
        let mut canvas_state = self.canvas_states.last_mut().unwrap();

        if width != canvas_state.line_width {
            self.ctx.set_line_width(width);
            canvas_state.line_width = width;
        }

        if style.line_join != canvas_state.line_join {
            self.ctx.set_line_join(convert_line_join(style.line_join));
            if let Some(limit) = style.miter_limit() {
                self.ctx.set_miter_limit(limit);
            }
            canvas_state.line_join = style.line_join;
        }

        if style.line_cap != canvas_state.line_cap {
            self.ctx.set_line_cap(convert_line_cap(style.line_cap));
            canvas_state.line_cap = style.line_cap;
        }

        if style.dash_pattern != canvas_state.line_dash {
            let dash_segs = convert_dash_pattern(&style.dash_pattern);
            self.ctx.set_line_dash(dash_segs.as_ref()).unwrap();
            canvas_state.line_dash = style.dash_pattern.clone();
        }

        if style.dash_offset != canvas_state.line_dash_offset {
            self.ctx.set_line_dash_offset(style.dash_offset);
            canvas_state.line_dash_offset = style.dash_offset;
        }
    }

    fn set_path(&mut self, shape: impl Shape) {
        // This shouldn't be necessary, we always leave the context in no-path
        // state. But just in case, and it should be harmless.
        self.ctx.begin_path();
        for el in shape.path_elements(1e-3) {
            match el {
                PathEl::MoveTo(p) => self.ctx.move_to(p.x, p.y),
                PathEl::LineTo(p) => self.ctx.line_to(p.x, p.y),
                PathEl::QuadTo(p1, p2) => self.ctx.quadratic_curve_to(p1.x, p1.y, p2.x, p2.y),
                PathEl::CurveTo(p1, p2, p3) => {
                    self.ctx.bezier_curve_to(p1.x, p1.y, p2.x, p2.y, p3.x, p3.y)
                }
                PathEl::ClosePath => self.ctx.close_path(),
            }
        }
    }
}

fn byte_to_frac(byte: u32) -> f64 {
    ((byte & 255) as f64) * (1.0 / 255.0)
}

fn matrix_to_affine(matrix: DomMatrix) -> Affine {
    Affine::new([
        matrix.a(),
        matrix.b(),
        matrix.c(),
        matrix.d(),
        matrix.e(),
        matrix.f(),
    ])
}
