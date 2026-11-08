//! Android Canvas and 2D Graphics rendering engine.
//!
//! Implements android.graphics.Canvas, Paint, Rect, and Path rasterization
//! directly into Stride's 32-bit RGBA Surface framebuffer.

use crate::graphics::Surface;

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
}

impl<'a> Canvas<'a> {
    pub fn new(surface: &'a mut Surface) -> Self {
        Self {
            surface,
            transform: CanvasTransform::default(),
            transform_stack: Vec::new(),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canvas_draws_rect_and_pixel() {
        let mut surface = Surface::new(100, 100);
        let mut canvas = Canvas::new(&mut surface);
        canvas.draw_color(0xffffffff); // white background

        let mut paint = Paint::new();
        paint.set_color(0xffff0000); // red
        canvas.draw_rect(RectF::new(10.0, 10.0, 20.0, 20.0), &paint);

        // Pixel inside rect should be red
        assert_eq!(canvas.surface.get_pixel(15, 15), Some(0xffff0000));
        // Pixel outside rect should be white
        assert_eq!(canvas.surface.get_pixel(5, 5), Some(0xffffffff));
    }

    #[test]
    fn canvas_transforms_and_circles() {
        let mut surface = Surface::new(100, 100);
        let mut canvas = Canvas::new(&mut surface);
        canvas.save();
        canvas.translate(50.0, 50.0);

        let mut paint = Paint::new();
        paint.set_color(0xff00ff00); // green
        canvas.draw_circle(0.0, 0.0, 10.0, &paint);

        // Center translated to (50, 50)
        assert_eq!(canvas.surface.get_pixel(50, 50), Some(0xff00ff00));
        canvas.restore();
    }
}
