//! Android Canvas and 2D Graphics rendering engine.
//!
//! Implements android.graphics.Canvas, Paint, Rect, and Path rasterization
//! directly into Stride's 32-bit RGBA Surface framebuffer.

use crate::graphics::Surface;

#[cfg(target_os = "ios")]
use std::ffi::{c_char, c_void};

#[cfg(target_os = "ios")]
#[repr(C)]
#[derive(Clone, Copy)]
struct CGAffineTransform { a: f64, b: f64, c: f64, d: f64, tx: f64, ty: f64 }

#[cfg(target_os = "ios")]
#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    fn CGColorSpaceCreateDeviceRGB() -> *mut c_void;
    fn CGColorSpaceRelease(space: *mut c_void);
    fn CGBitmapContextCreate(data: *mut c_void, width: usize, height: usize, bits_per_component: usize, bytes_per_row: usize, space: *mut c_void, bitmap_info: u32) -> *mut c_void;
    fn CGContextRelease(context: *mut c_void);
    fn CGContextFlush(context: *mut c_void);
    fn CGContextTranslateCTM(context: *mut c_void, tx: f64, ty: f64);
    fn CGContextScaleCTM(context: *mut c_void, sx: f64, sy: f64);
    fn CGContextSetTextMatrix(context: *mut c_void, transform: CGAffineTransform);
    fn CGContextSetRGBFillColor(context: *mut c_void, red: f64, green: f64, blue: f64, alpha: f64);
    fn CGContextSetTextDrawingMode(context: *mut c_void, mode: i32);
    fn CGContextSelectFont(context: *mut c_void, name: *const c_char, size: f64, encoding: i32);
    fn CGContextShowTextAtPoint(context: *mut c_void, x: f64, y: f64, text: *const c_char, length: usize);
    fn CGContextSetShouldAntialias(context: *mut c_void, should_antialias: bool);
    fn CGContextSetAllowsAntialiasing(context: *mut c_void, allows_antialiasing: bool);
}

#[cfg(target_os = "ios")]
fn create_native_text_context(surface: &mut Surface) -> Option<*mut c_void> {
    let color_space = unsafe { CGColorSpaceCreateDeviceRGB() };
    if color_space.is_null() { return None; }
    let context = unsafe {
        CGBitmapContextCreate(
            surface.pixels.as_mut_ptr().cast(),
            surface.width as usize,
            surface.height as usize,
            8,
            surface.width as usize * std::mem::size_of::<u32>(),
            color_space,
            0x2002,
        )
    };
    unsafe { CGColorSpaceRelease(color_space) };
    if context.is_null() { return None; }
    unsafe {
        CGContextTranslateCTM(context, 0.0, surface.height as f64);
        CGContextScaleCTM(context, 1.0, -1.0);
        CGContextSetTextMatrix(context, CGAffineTransform { a: 1.0, b: 0.0, c: 0.0, d: -1.0, tx: 0.0, ty: 0.0 });
        CGContextSetShouldAntialias(context, true);
        CGContextSetAllowsAntialiasing(context, true);
    }
    Some(context)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaintStyle {
    Fill,
    Stroke,
    FillAndStroke,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Paint {
    pub color: u32, // ARGB (0xAARRGGBB)
    pub stroke_width: f32,
    pub style: PaintStyle,
    pub anti_alias: bool,
    pub text_size: f32,
}

impl Default for Paint {
    fn default() -> Self {
        Self {
            color: 0xff000000, // opaque black
            stroke_width: 1.0,
            style: PaintStyle::Fill,
            anti_alias: true,
            text_size: 14.0,
        }
    }
}

impl Paint {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_color(&mut self, argb: u32) {
        self.color = argb;
    }

    pub fn set_argb(&mut self, a: u8, r: u8, g: u8, b: u8) {
        self.color = ((a as u32) << 24) | ((r as u32) << 16) | ((g as u32) << 8) | (b as u32);
    }

    pub fn set_stroke_width(&mut self, width: f32) {
        self.stroke_width = width.max(0.0);
    }

    pub fn set_style(&mut self, style: PaintStyle) {
        self.style = style;
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RectF {
    pub left: f32,
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
}

impl RectF {
    pub fn new(left: f32, top: f32, right: f32, bottom: f32) -> Self {
        Self { left, top, right, bottom }
    }

    pub fn width(&self) -> f32 {
        (self.right - self.left).max(0.0)
    }

    pub fn height(&self) -> f32 {
        (self.bottom - self.top).max(0.0)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum PathVerb {
    MoveTo(f32, f32),
    LineTo(f32, f32),
    Close,
}

#[derive(Debug, Default, Clone, PartialEq)]
pub struct Path {
    pub verbs: Vec<PathVerb>,
}

impl Path {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn move_to(&mut self, x: f32, y: f32) {
        self.verbs.push(PathVerb::MoveTo(x, y));
    }

    pub fn line_to(&mut self, x: f32, y: f32) {
        self.verbs.push(PathVerb::LineTo(x, y));
    }

    pub fn close(&mut self) {
        self.verbs.push(PathVerb::Close);
    }

    pub fn reset(&mut self) {
        self.verbs.clear();
    }
}

/// Matrix transformation stack frame for Canvas save/restore.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CanvasTransform {
    pub translate_x: f32,
    pub translate_y: f32,
    pub scale_x: f32,
    pub scale_y: f32,
}

impl Default for CanvasTransform {
    fn default() -> Self {
        Self {
            translate_x: 0.0,
            translate_y: 0.0,
            scale_x: 1.0,
            scale_y: 1.0,
        }
    }
}

/// Android Canvas drawing context rasterizing 2D primitives onto a Surface.
pub struct Canvas<'a> {
    pub surface: &'a mut Surface,
    transform: CanvasTransform,
    transform_stack: Vec<CanvasTransform>,
    #[cfg(target_os = "ios")]
    native_text_context: Option<*mut c_void>,
}

impl<'a> Canvas<'a> {
    pub fn new(surface: &'a mut Surface) -> Self {
        #[cfg(target_os = "ios")]
        let native_text_context = create_native_text_context(surface);
        Self {
            surface,
            transform: CanvasTransform::default(),
            transform_stack: Vec::new(),
            #[cfg(target_os = "ios")]
            native_text_context,
        }
    }

    pub fn width(&self) -> u32 {
        self.surface.width
    }

    pub fn height(&self) -> u32 {
        self.surface.height
    }

    pub fn save(&mut self) -> usize {
        self.transform_stack.push(self.transform);
        self.transform_stack.len()
    }

    pub fn restore(&mut self) {
        if let Some(t) = self.transform_stack.pop() {
            self.transform = t;
        }
    }

    pub fn translate(&mut self, dx: f32, dy: f32) {
        self.transform.translate_x += dx * self.transform.scale_x;
        self.transform.translate_y += dy * self.transform.scale_y;
    }

    pub fn scale(&mut self, sx: f32, sy: f32) {
        self.transform.scale_x *= sx;
        self.transform.scale_y *= sy;
    }

    pub fn draw_color(&mut self, argb: u32) {
        self.surface.clear(argb);
    }

    fn apply_transform(&self, x: f32, y: f32) -> (i32, i32) {
        let tx = (x * self.transform.scale_x) + self.transform.translate_x;
        let ty = (y * self.transform.scale_y) + self.transform.translate_y;
        (tx.round() as i32, ty.round() as i32)
    }

    pub fn draw_pixel(&mut self, x: i32, y: i32, color: u32) {
        if x >= 0 && y >= 0 && (x as u32) < self.surface.width && (y as u32) < self.surface.height {
            self.surface.set_pixel(x as u32, y as u32, color);
        }
    }

    pub fn draw_rect(&mut self, rect: RectF, paint: &Paint) {
        let (x0, y0) = self.apply_transform(rect.left.min(rect.right), rect.top.min(rect.bottom));
        let (x1, y1) = self.apply_transform(rect.left.max(rect.right), rect.top.max(rect.bottom));

        let min_x = x0.max(0).min(self.surface.width as i32);
        let max_x = x1.max(0).min(self.surface.width as i32);
        let min_y = y0.max(0).min(self.surface.height as i32);
        let max_y = y1.max(0).min(self.surface.height as i32);

        match paint.style {
            PaintStyle::Fill | PaintStyle::FillAndStroke => {
                for y in min_y..max_y {
                    for x in min_x..max_x {
                        self.surface.set_pixel(x as u32, y as u32, paint.color);
                    }
                }
            }
            PaintStyle::Stroke => {
                let stroke = (paint.stroke_width * self.transform.scale_x).max(1.0) as i32;
                for y in min_y..max_y {
                    for x in min_x..max_x {
                        if x < min_x + stroke || x >= max_x - stroke || y < min_y + stroke || y >= max_y - stroke {
                            self.surface.set_pixel(x as u32, y as u32, paint.color);
                        }
                    }
                }
            }
        }
    }

    pub fn draw_circle(&mut self, cx: f32, cy: f32, radius: f32, paint: &Paint) {
        if radius <= 0.0 {
            return;
        }
        let (center_x, center_y) = self.apply_transform(cx, cy);
        let r = (radius * self.transform.scale_x).max(1.0);
        let r_sq = r * r;
        let ir = r.ceil() as i32;

        let min_x = (center_x - ir).max(0);
        let max_x = (center_x + ir).min(self.surface.width as i32 - 1);
        let min_y = (center_y - ir).max(0);
        let max_y = (center_y + ir).min(self.surface.height as i32 - 1);

        for y in min_y..=max_y {
            let dy = (y - center_y) as f32;
            let dy_sq = dy * dy;
            for x in min_x..=max_x {
                let dx = (x - center_x) as f32;
                let d_sq = dx * dx + dy_sq;
                if d_sq <= r_sq {
                    if paint.style == PaintStyle::Fill || paint.style == PaintStyle::FillAndStroke {
                        self.surface.set_pixel(x as u32, y as u32, paint.color);
                    } else if paint.style == PaintStyle::Stroke {
                        let stroke = (paint.stroke_width * self.transform.scale_x).max(1.0);
                        let inner_r = (r - stroke).max(0.0);
                        if d_sq >= inner_r * inner_r {
                            self.surface.set_pixel(x as u32, y as u32, paint.color);
                        }
                    }
                }
            }
        }
    }

    pub fn draw_line(&mut self, x0: f32, y0: f32, x1: f32, y1: f32, paint: &Paint) {
        let (mut px0, mut py0) = self.apply_transform(x0, y0);
        let (px1, py1) = self.apply_transform(x1, y1);

        let dx = (px1 - px0).abs();
        let dy = -(py1 - py0).abs();
        let sx = if px0 < px1 { 1 } else { -1 };
        let sy = if py0 < py1 { 1 } else { -1 };
        let mut err = dx + dy;

        loop {
            self.draw_pixel(px0, py0, paint.color);
            if px0 == px1 && py0 == py1 {
                break;
            }
            let e2 = 2 * err;
            if e2 >= dy {
                err += dy;
                px0 += sx;
            }
            if e2 <= dx {
                err += dx;
                py0 += sy;
            }
        }
    }


    pub fn draw_text(&mut self,text:&str,x:f32,baseline:f32,paint:&Paint)->f32 {
        #[cfg(target_os = "ios")]
        if let Some(context) = self.native_text_context {
            let mut bytes = Vec::with_capacity(text.len());
            for ch in text.chars() {
                match ch {
                    '\n' | '\r' => {},
                    '×' => bytes.push(b'x'),
                    '÷' => bytes.push(b'/'),
                    '−' => bytes.push(b'-'),
                    '±' => bytes.extend_from_slice(b"+/-"),
                    '⌫' => bytes.push(b'<'),
                    ch if ch.is_ascii() => bytes.push(ch as u8),
                    _ => bytes.push(b'?'),
                }
            }
            if !bytes.is_empty() {
                let color = paint.color;
                unsafe {
                    CGContextSetRGBFillColor(
                        context,
                        ((color >> 16) & 0xff) as f64 / 255.0,
                        ((color >> 8) & 0xff) as f64 / 255.0,
                        (color & 0xff) as f64 / 255.0,
                        ((color >> 24) & 0xff) as f64 / 255.0,
                    );
                    CGContextSetTextDrawingMode(context, 0);
                    // The bytes below are ASCII/MacRoman character codes, not font-specific glyph IDs.
                    // Using kCGEncodingFontSpecific (0) maps letters to unrelated glyphs on device.\n                    CGContextSelectFont(context, b"Helvetica\0".as_ptr().cast(), paint.text_size.max(1.0) as f64, 1);
                    CGContextShowTextAtPoint(context, x as f64, baseline as f64, bytes.as_ptr().cast(), bytes.len());
                    CGContextFlush(context);
                }
            }
            return x + bytes.len() as f32 * paint.text_size.max(1.0) * 0.56;
        }

        // Non-iOS test hosts retain the deterministic fallback rasterizer. Physical
        // iPhone builds use native Helvetica glyphs instead of enlarged 5x7 pixels.
        let scale=(paint.text_size/7.0).max(1.0);let mut cursor=x;
        for ch in text.chars(){if ch=='\n'{continue;}let rows=glyph5x7(ch.to_ascii_uppercase());
            for(row,bits)in rows.iter().enumerate(){for col in 0..5{if bits&(1<<(4-col))!=0{
                let left=cursor+col as f32*scale;let top=baseline-7.0*scale+row as f32*scale;
                self.draw_rect(RectF::new(left,top,left+scale,top+scale),paint);
            }}}
            cursor+=6.0*scale;
        }cursor
    }
    pub fn draw_image_argb(&mut self,pixels:&[u32],width:u32,height:u32,left:i32,top:i32){
        self.draw_image_argb_scaled_alpha(pixels, width, height, left, top,
            left.saturating_add(width as i32), top.saturating_add(height as i32), 255);
    }

    /// Draw an ARGB bitmap into destination bounds using nearest-neighbour
    /// sampling and Android-style drawable alpha modulation.
    pub fn draw_image_argb_scaled_alpha(
        &mut self, pixels:&[u32], width:u32, height:u32,
        left:i32, top:i32, right:i32, bottom:i32, alpha:u8,
    ) {
        if width == 0 || height == 0 || right <= left || bottom <= top
            || (width as usize).checked_mul(height as usize) != Some(pixels.len()) || alpha == 0 {
            return;
        }
        let dst_width = (right as i64 - left as i64) as u64;
        let dst_height = (bottom as i64 - top as i64) as u64;
        let min_x = left.max(0);
        let max_x = right.min(self.surface.width as i32);
        let min_y = top.max(0);
        let max_y = bottom.min(self.surface.height as i32);
        for dy in min_y..max_y {
            let sy = (((dy as i64 - top as i64) as u64 * height as u64) / dst_height)
                .min(height as u64 - 1) as u32;
            for dx in min_x..max_x {
                let sx = (((dx as i64 - left as i64) as u64 * width as u64) / dst_width)
                    .min(width as u64 - 1) as u32;
                let src = pixels[(sy * width + sx) as usize];
                let a = (((src >> 24) & 255) * alpha as u32 + 127) / 255;
                if a == 255 {
                    self.surface.set_pixel(dx as u32, dy as u32, 0xff000000 | (src & 0x00ffffff));
                } else if a > 0 {
                    let dst = self.surface.get_pixel(dx as u32, dy as u32).unwrap_or(0xff000000);
                    let inv = 255 - a;
                    let blend = |shift:u32| ((((src >> shift) & 255) * a
                        + ((dst >> shift) & 255) * inv + 127) / 255) & 255;
                    self.surface.set_pixel(dx as u32, dy as u32,
                        0xff000000 | (blend(16) << 16) | (blend(8) << 8) | blend(0));
                }
            }
        }
    }

    pub fn draw_path(&mut self, path: &Path, paint: &Paint) {
        let mut cur_x = 0.0;
        let mut cur_y = 0.0;
        let mut start_x = 0.0;
        let mut start_y = 0.0;

        for verb in &path.verbs {
            match *verb {
                PathVerb::MoveTo(x, y) => {
                    cur_x = x;
                    cur_y = y;
                    start_x = x;
                    start_y = y;
                }
                PathVerb::LineTo(x, y) => {
                    self.draw_line(cur_x, cur_y, x, y, paint);
                    cur_x = x;
                    cur_y = y;
                }
                PathVerb::Close => {
                    self.draw_line(cur_x, cur_y, start_x, start_y, paint);
                    cur_x = start_x;
                    cur_y = start_y;
                }
            }
        }
    }
}

fn glyph5x7(ch: char) -> [u8; 7] {
    match ch {
        'A'=>[14,17,17,31,17,17,17], 'B'=>[30,17,17,30,17,17,30],
        'C'=>[14,17,16,16,16,17,14], 'D'=>[30,17,17,17,17,17,30],
        'E'=>[31,16,16,30,16,16,31], 'F'=>[31,16,16,30,16,16,16],
        'G'=>[14,17,16,23,17,17,15], 'H'=>[17,17,17,31,17,17,17],
        'I'=>[14,4,4,4,4,4,14], 'J'=>[7,2,2,2,18,18,12],
        'K'=>[17,18,20,24,20,18,17], 'L'=>[16,16,16,16,16,16,31],
        'M'=>[17,27,21,21,17,17,17], 'N'=>[17,25,21,19,17,17,17],
        'O'=>[14,17,17,17,17,17,14], 'P'=>[30,17,17,30,16,16,16],
        'Q'=>[14,17,17,17,21,18,13], 'R'=>[30,17,17,30,20,18,17],
        'S'=>[15,16,16,14,1,1,30], 'T'=>[31,4,4,4,4,4,4],
        'U'=>[17,17,17,17,17,17,14], 'V'=>[17,17,17,17,17,10,4],
        'W'=>[17,17,17,21,21,21,10], 'X'=>[17,17,10,4,10,17,17],
        'Y'=>[17,17,10,4,4,4,4], 'Z'=>[31,1,2,4,8,16,31],
        '0'=>[14,17,19,21,25,17,14], '1'=>[4,12,4,4,4,4,14],
        '2'=>[14,17,1,2,4,8,31], '3'=>[30,1,1,14,1,1,30],
        '4'=>[2,6,10,18,31,2,2], '5'=>[31,16,16,30,1,1,30],
        '6'=>[14,16,16,30,17,17,14], '7'=>[31,1,2,4,8,8,8],
        '8'=>[14,17,17,14,17,17,14], '9'=>[14,17,17,15,1,1,14],
        '+'=>[0,4,4,31,4,4,0], '='=>[0,0,31,0,31,0,0],
        '-'=>[0,0,0,31,0,0,0], '−'=>[0,0,0,31,0,0,0],
        '×'=>[17,10,4,4,4,10,17], '*'=>[0,21,14,31,14,21,0],
        '÷'=>[0,4,0,31,0,4,0], '/'=>[1,2,2,4,8,8,16],
        '%'=>[17,2,4,8,16,17,0], '±'=>[0,4,4,31,4,4,31],
        '('=>[2,4,8,8,8,4,2], ')'=>[8,4,2,2,2,4,8],
        '['=>[14,8,8,8,8,8,14], ']'=>[14,2,2,2,2,2,14],
        '.'=>[0,0,0,0,0,12,12], ','=>[0,0,0,0,0,12,8],
        ':'=>[0,12,12,0,12,12,0], ' '=>[0;7],
        '!'=>[4,4,4,4,4,0,4], '?'=>[14,17,1,2,4,0,4],
        '⌫'=>[16,8,4,2,4,8,16],
        _=>[31,17,5,9,4,0,4]
    }
}

#[cfg(target_os = "ios")]
impl Drop for Canvas<'_> {
    fn drop(&mut self) {
        if let Some(context) = self.native_text_context.take() {
            unsafe { CGContextFlush(context); CGContextRelease(context); }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canvas_draws_rect_and_pixel() {
        let mut surface = Surface::new(100, 100);
        let mut canvas = Canvas::new(&mut surface);
        canvas.draw_color(0xffffffff);
        let mut paint = Paint::new();
        paint.set_color(0xffff0000);
        canvas.draw_rect(RectF::new(10.0, 10.0, 20.0, 20.0), &paint);
        assert_eq!(canvas.surface.get_pixel(15, 15), Some(0xffff0000));
        assert_eq!(canvas.surface.get_pixel(5, 5), Some(0xffffffff));
    }

    #[test]
    fn canvas_draws_text_and_image_pixels() {
        let mut surface = Surface::new(90, 20);
        let mut canvas = Canvas::new(&mut surface);
        let mut paint = Paint::new();
        paint.color = 0xffffffff;
        paint.text_size = 7.0;
        canvas.draw_text("STRIDE 123", 1.0, 9.0, &paint);
        assert_ne!(canvas.surface.get_pixel(2, 2), Some(0xff000000));
        canvas.draw_image_argb(&[0xffff0000, 0x8000ff00], 2, 1, 0, 10);
        assert_eq!(canvas.surface.get_pixel(0, 10), Some(0xffff0000));
        assert_eq!(canvas.surface.get_pixel(1, 10), Some(0xff008000));
    }

    #[test]
    fn scaled_bitmap_draw_honors_destination_bounds_and_global_alpha() {
        let mut surface = Surface::new(4, 4);
        surface.clear(0xff0000ff);
        let mut canvas = Canvas::new(&mut surface);
        canvas.draw_image_argb_scaled_alpha(
            &[0xffff0000], 1, 1, 1, 1, 3, 3, 128,
        );
        assert_eq!(canvas.surface.get_pixel(0, 0), Some(0xff0000ff));
        assert_eq!(canvas.surface.get_pixel(1, 1), Some(0xff80007f));
        assert_eq!(canvas.surface.get_pixel(2, 2), Some(0xff80007f));
        assert_eq!(canvas.surface.get_pixel(3, 3), Some(0xff0000ff));
    }

    #[test]
    fn calculator_key_glyphs_are_present_and_not_question_mark_fallbacks() {
        for symbol in ['A', 'C', '0', '9', '+', '-', '−', '×', '÷', '%', '=', '⌫'] {
            let glyph = glyph5x7(symbol);
            assert!(glyph.iter().any(|row| *row != 0), "missing visible glyph for {:?}", symbol);
            if matches!(symbol, '+' | '-' | '−' | '×' | '÷' | '⌫') {
                assert_ne!(glyph, glyph5x7('?'), "calculator symbol {:?} fell back to question-mark pixels", symbol);
            }
        }
    }

    #[test]
    fn canvas_transforms_and_circles() {
        let mut surface = Surface::new(100, 100);
        let mut canvas = Canvas::new(&mut surface);
        canvas.save();
        canvas.translate(50.0, 50.0);
        let mut paint = Paint::new();
        paint.set_color(0xff00ff00);
        canvas.draw_circle(0.0, 0.0, 10.0, &paint);
        assert_eq!(canvas.surface.get_pixel(50, 50), Some(0xff00ff00));
        canvas.restore();
    }
}
