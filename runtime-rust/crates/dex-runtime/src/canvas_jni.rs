//! Guest-facing android.graphics.Canvas JNI bridge.
//!
//! Canvas calls are recorded on the managed Canvas object. The renderer drains
//! these commands after guest onDraw returns, keeping interpreter execution
//! separate from mutable host framebuffer access.
use crate::heap::ManagedHeap;
use crate::jni::{JniEnv, JniNativeRegistry};
use crate::types::Value;

fn receiver(args: &[Value]) -> Result<u32, String> {
    args.first().and_then(Value::as_object).filter(|id| *id != 0)
        .ok_or_else(|| "Canvas receiver is null or invalid".to_owned())
}
fn int(args: &[Value], index: usize, default: i32) -> i32 {
    args.get(index).map(Value::as_int).unwrap_or(default)
}
fn float(args: &[Value], index: usize) -> f32 {
    args.get(index).map(Value::as_float).unwrap_or(0.0)
}

fn rounded_rect_paint_args(env: &mut JniEnv<'_>, paint: u32) -> Result<[Value; 2], String> {
    let style_object = env.get_object_field(paint, "mStyleObject")?.as_object();
    let style_name = style_object
        .and_then(|id| env.get_object_field(id, "name").ok().and_then(|v| v.as_object()))
        .and_then(|id| env.get_string_utf(id).ok().map(str::to_owned))
        .or_else(|| style_object
            .and_then(|id| env.get_object_field(id, "mName").ok().and_then(|v| v.as_object()))
            .and_then(|id| env.get_string_utf(id).ok().map(str::to_owned)));
    let style = match style_name.as_deref() {
        Some("STROKE") => 1,
        Some("FILL_AND_STROKE") => 2,
        _ => 0,
    };
    let stroke_width = env.get_object_field(paint, "mStrokeWidth")
        .map(|value| value.as_float())
        .unwrap_or(1.0);
    Ok([Value::Int(style), Value::Float(stroke_width)])
}
fn append(env: &mut JniEnv<'_>, canvas: u32, kind: i32, values: &[Value]) -> Result<(), String> {
    let count = env.get_object_field(canvas, "mStrideCommandCount")?.as_int().clamp(0, 16384) as usize;
    let prefix = format!("mStrideCommand{}", count);
    env.set_object_field(canvas, &format!("{prefix}Type"), Value::Int(kind))?;
    for (index, value) in values.iter().copied().enumerate() {
        env.set_object_field(canvas, &format!("{prefix}Arg{index}"), value)?;
    }
    env.set_object_field(canvas, "mStrideCommandCount", Value::Int((count + 1) as i32))?;
    Ok(())
}

fn register(registry: &mut JniNativeRegistry) {
    for prototype in ["()V", "(Landroid/graphics/Bitmap;)V"] {
        registry.register("Landroid/graphics/Canvas;", "<init>", prototype, |env, args| {
            let canvas = receiver(args)?;
            env.set_object_field(canvas, "mStrideCommandCount", Value::Int(0))?;
            let bitmap_value = args.get(1).copied().unwrap_or(Value::Null);
            let (width, height) = if let Some(bitmap) = bitmap_value.as_object() {
                (
                    env.get_object_field(bitmap, "mStrideWidth")?.as_int(),
                    env.get_object_field(bitmap, "mStrideHeight")?.as_int(),
                )
            } else { (0, 0) };
            env.set_object_field(canvas, "mStrideBitmap", bitmap_value)?;
            env.set_object_field(canvas, "mStrideWidth", Value::Int(width))?;
            env.set_object_field(canvas, "mStrideHeight", Value::Int(height))?;
            env.set_object_field(canvas, "mStrideSaveDepth", Value::Int(0))?;
            Ok(None)
        });
    }
    registry.register("Landroid/graphics/Canvas;", "drawColor", "(I)V", |env, args| {
        let canvas = receiver(args)?;
        append(env, canvas, 1, &[Value::Int(int(args, 1, 0))])?;
        Ok(None)
    });
    registry.register("Landroid/graphics/Canvas;", "drawRoundRect", "(Landroid/graphics/RectF;FFLandroid/graphics/Paint;)V", |env, args| {
        let canvas = receiver(args)?;
        let rect = args.get(1).and_then(Value::as_object).ok_or("Canvas.drawRoundRect RectF is null")?;
        let paint = args.get(4).and_then(Value::as_object).ok_or("Canvas.drawRoundRect Paint is null")?;
        let color = env.get_object_field(paint, "mColor")?;
        let paint_args = rounded_rect_paint_args(env, paint)?;
        let left = env.get_object_field(rect, "left")?.as_float();
        let top = env.get_object_field(rect, "top")?.as_float();
        let right = env.get_object_field(rect, "right")?.as_float();
        let bottom = env.get_object_field(rect, "bottom")?.as_float();
        append(env, canvas, 12, &[
            Value::Float(left), Value::Float(top), Value::Float(right), Value::Float(bottom),
            Value::Float(float(args, 2)), Value::Float(float(args, 3)), color,
            paint_args[0], paint_args[1],
        ])?;
        Ok(None)
    });
    registry.register("Landroid/graphics/Canvas;", "drawRoundRect", "(FFFFFFLandroid/graphics/Paint;)V", |env, args| {
        let canvas = receiver(args)?;
        let paint = args.get(7).and_then(Value::as_object).ok_or("Canvas.drawRoundRect Paint is null")?;
        let color = env.get_object_field(paint, "mColor")?;
        let paint_args = rounded_rect_paint_args(env, paint)?;
        append(env, canvas, 12, &[
            Value::Float(float(args, 1)), Value::Float(float(args, 2)),
            Value::Float(float(args, 3)), Value::Float(float(args, 4)),
            Value::Float(float(args, 5)), Value::Float(float(args, 6)), color,
            paint_args[0], paint_args[1],
        ])?;
        Ok(None)
    });
    registry.register("Landroid/graphics/Canvas;", "drawRect", "(FFFFLandroid/graphics/Paint;)V", |env, args| {
        let canvas = receiver(args)?;
        let paint = args.get(5).and_then(Value::as_object).ok_or("Canvas.drawRect Paint is null")?;
        let color = env.get_object_field(paint, "mColor")?;
        append(env, canvas, 2, &[
            Value::Float(float(args, 1)), Value::Float(float(args, 2)),
            Value::Float(float(args, 3)), Value::Float(float(args, 4)), color,
        ])?;
        Ok(None)
    });
    registry.register("Landroid/graphics/Canvas;", "drawText", "(Ljava/lang/String;FFLandroid/graphics/Paint;)V", |env, args| {
        let canvas = receiver(args)?;
        let text = args.get(1).copied().unwrap_or(Value::Null);
        let paint = args.get(4).and_then(Value::as_object).ok_or("Canvas.drawText Paint is null")?;
        let color = env.get_object_field(paint, "mColor")?;
        let size = env.get_object_field(paint, "mTextSize")?;
        let align = env.get_object_field(paint, "mTextAlignObject")?.as_object()
            .and_then(|id| env.get_object_field(id, "name").ok().and_then(|value| value.as_object()))
            .and_then(|id| env.get_string_utf(id).ok().map(str::to_owned))
            .or_else(|| env.get_object_field(paint, "mTextAlignObject").ok()
                .and_then(|value| value.as_object())
                .and_then(|id| env.get_object_field(id, "mName").ok().and_then(|value| value.as_object()))
                .and_then(|id| env.get_string_utf(id).ok().map(str::to_owned)))
            .unwrap_or_else(|| "LEFT".to_owned());
        let align_code = match align.as_str() { "CENTER" => 1, "RIGHT" => 2, _ => 0 };
        let typeface = env.get_object_field(paint, "mTypeface").unwrap_or(Value::Null);
        append(env, canvas, 3, &[
            text, Value::Float(float(args, 2)), Value::Float(float(args, 3)),
            color, size, Value::Int(align_code), typeface,
        ])?;
        Ok(None)
    });
    registry.register("Landroid/graphics/Canvas;", "drawBitmap", "(Landroid/graphics/Bitmap;FFLandroid/graphics/Paint;)V", |env, args| {
        let canvas = receiver(args)?;
        let bitmap = args.get(1).and_then(Value::as_object).ok_or("Canvas.drawBitmap Bitmap is null")?;
        // Android permits a null Paint; in that case the bitmap is drawn at full alpha.
        // A supplied Paint modulates the bitmap's per-pixel alpha via Paint.getAlpha().
        let alpha = if let Some(paint) = args.get(4).and_then(Value::as_object) {
            env.get_object_field(paint, "mAlpha").map(|value| value.as_int())
                .unwrap_or(255).clamp(0, 255)
        } else { 255 };
        append(env, canvas, 10, &[
            Value::Object(bitmap), Value::Float(float(args, 2)), Value::Float(float(args, 3)),
            Value::Int(alpha),
        ])?;
        Ok(None)
    });
    registry.register("Landroid/graphics/Canvas;", "drawLine", "(FFFFLandroid/graphics/Paint;)V", |env, args| {
        let canvas = receiver(args)?;
        let paint = args.get(5).and_then(Value::as_object).ok_or("Canvas.drawLine Paint is null")?;
        let color = env.get_object_field(paint, "mColor")?;
        append(env, canvas, 4, &[
            Value::Float(float(args, 1)), Value::Float(float(args, 2)),
            Value::Float(float(args, 3)), Value::Float(float(args, 4)), color,
        ])?;
        Ok(None)
    });
    registry.register("Landroid/graphics/Canvas;", "drawCircle", "(FFFLandroid/graphics/Paint;)V", |env, args| {
        let canvas = receiver(args)?;
        let paint = args.get(4).and_then(Value::as_object).ok_or("Canvas.drawCircle Paint is null")?;
        let color = env.get_object_field(paint, "mColor")?;
        append(env, canvas, 5, &[
            Value::Float(float(args, 1)), Value::Float(float(args, 2)),
            Value::Float(float(args, 3)), color,
        ])?;
        Ok(None)
    });
    registry.register("Landroid/graphics/Canvas;", "save", "()I", |env, args| {
        let canvas = receiver(args)?;
        let depth = env.get_object_field(canvas, "mStrideSaveDepth")?.as_int().max(0) + 1;
        env.set_object_field(canvas, "mStrideSaveDepth", Value::Int(depth))?;
        append(env, canvas, 6, &[])?;
        Ok(Some(Value::Int(depth)))
    });
    registry.register("Landroid/graphics/Canvas;", "restore", "()V", |env, args| {
        let canvas = receiver(args)?;
        let depth = env.get_object_field(canvas, "mStrideSaveDepth")?.as_int().max(0);
        env.set_object_field(canvas, "mStrideSaveDepth", Value::Int(depth.saturating_sub(1)))?;
        append(env, canvas, 7, &[])?;
        Ok(None)
    });
    for (name, kind) in [("translate", 8), ("scale", 9)] {
        registry.register("Landroid/graphics/Canvas;", name, "(FF)V", move |env, args| {
            let canvas = receiver(args)?;
            append(env, canvas, kind, &[Value::Float(float(args, 1)), Value::Float(float(args, 2))])?;
            Ok(None)
        });
    }
    registry.register("Landroid/graphics/Canvas;", "getWidth", "()I", |env, args| {
        Ok(Some(env.get_object_field(receiver(args)?, "mStrideWidth")?))
    });
    registry.register("Landroid/graphics/Canvas;", "getHeight", "()I", |env, args| {
        Ok(Some(env.get_object_field(receiver(args)?, "mStrideHeight")?))
    });
}

/// Snapshot recorded guest drawing operations without mutating the managed heap.
pub fn recorded_command_count(heap: &ManagedHeap, canvas: u32) -> Result<usize, String> {
    heap.get_instance_field(canvas, "mStrideCommandCount")
        .map(|value| value.as_int().clamp(0, 16384) as usize)
        .map_err(|error| format!("Canvas command count unavailable: {error:?}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::heap::ManagedHeap;

    #[test]
    fn records_guest_draw_color_and_rect_commands() {
        let mut registry = JniNativeRegistry::new();
        register(&mut registry);
        let mut heap = ManagedHeap::default();
        let canvas = heap.allocate_instance("Landroid/graphics/Canvas;").unwrap();
        let paint = heap.allocate_instance("Landroid/graphics/Paint;").unwrap();
        heap.set_instance_field(paint, "mColor", Value::Int(0xff123456u32 as i32)).unwrap();
        let draw_color = registry.find("Landroid/graphics/Canvas;", "drawColor", "(I)V").unwrap().clone();
        draw_color(&mut JniEnv::new(&mut heap), &[Value::Object(canvas), Value::Int(0xffabcdefu32 as i32)]).unwrap();
        let draw_rect = registry.find("Landroid/graphics/Canvas;", "drawRect", "(FFFFLandroid/graphics/Paint;)V").unwrap().clone();
        draw_rect(&mut JniEnv::new(&mut heap), &[
            Value::Object(canvas), Value::Float(1.0), Value::Float(2.0),
            Value::Float(30.0), Value::Float(40.0), Value::Object(paint),
        ]).unwrap();
        assert_eq!(recorded_command_count(&heap, canvas).unwrap(), 2);
        assert_eq!(heap.get_instance_field(canvas, "mStrideCommand0Type").unwrap(), Value::Int(1));
        assert_eq!(heap.get_instance_field(canvas, "mStrideCommand1Type").unwrap(), Value::Int(2));
        assert_eq!(heap.get_instance_field(canvas, "mStrideCommand1Arg4").unwrap(), Value::Int(0xff123456u32 as i32));
    }

    #[test]
    fn draw_text_records_typeface_reference_for_asset_font_rendering() {
        let mut registry = JniNativeRegistry::new();
        register(&mut registry);
        let mut heap = ManagedHeap::default();
        let canvas = heap.allocate_instance("Landroid/graphics/Canvas;").unwrap();
        let paint = heap.allocate_instance("Landroid/graphics/Paint;").unwrap();
        let typeface = heap.allocate_instance("Landroid/graphics/Typeface;").unwrap();
        let text = heap.allocate_string("2048").unwrap();
        heap.set_instance_field(paint, "mColor", Value::Int(0xffffffffu32 as i32)).unwrap();
        heap.set_instance_field(paint, "mTextSize", Value::Float(24.0)).unwrap();
        heap.set_instance_field(paint, "mTypeface", Value::Object(typeface)).unwrap();
        let draw = registry.find("Landroid/graphics/Canvas;", "drawText",
            "(Ljava/lang/String;FFLandroid/graphics/Paint;)V").unwrap().clone();
        draw(&mut JniEnv::new(&mut heap), &[
            Value::Object(canvas), Value::Object(text), Value::Float(20.0),
            Value::Float(30.0), Value::Object(paint),
        ]).unwrap();
        assert_eq!(heap.get_instance_field(canvas, "mStrideCommand0Arg6").unwrap(),
            Value::Object(typeface));
    }

    #[test]
    fn draw_bitmap_records_paint_alpha_and_defaults_null_paint_to_opaque() {
        let mut registry = JniNativeRegistry::new();
        register(&mut registry);
        let mut heap = ManagedHeap::default();
        let canvas = heap.allocate_instance("Landroid/graphics/Canvas;").unwrap();
        let bitmap = heap.allocate_instance("Landroid/graphics/Bitmap;").unwrap();
        let paint = heap.allocate_instance("Landroid/graphics/Paint;").unwrap();
        heap.set_instance_field(paint, "mAlpha", Value::Int(96)).unwrap();
        let draw = registry.find("Landroid/graphics/Canvas;", "drawBitmap",
            "(Landroid/graphics/Bitmap;FFLandroid/graphics/Paint;)V").unwrap().clone();

        draw(&mut JniEnv::new(&mut heap), &[
            Value::Object(canvas), Value::Object(bitmap), Value::Float(4.0),
            Value::Float(7.0), Value::Object(paint),
        ]).unwrap();
        assert_eq!(heap.get_instance_field(canvas, "mStrideCommand0Arg3").unwrap(), Value::Int(96));

        draw(&mut JniEnv::new(&mut heap), &[
            Value::Object(canvas), Value::Object(bitmap), Value::Float(0.0),
            Value::Float(0.0), Value::Null,
        ]).unwrap();
        assert_eq!(heap.get_instance_field(canvas, "mStrideCommand1Arg3").unwrap(), Value::Int(255));
    }
    #[test]
    fn records_guest_draw_round_rect_with_rect_and_radii() {
        let mut registry = JniNativeRegistry::new();
        register(&mut registry);
        let mut heap = ManagedHeap::default();
        let canvas = heap.allocate_instance("Landroid/graphics/Canvas;").unwrap();
        let rect = heap.allocate_instance("Landroid/graphics/RectF;").unwrap();
        let paint = heap.allocate_instance("Landroid/graphics/Paint;").unwrap();
        heap.set_instance_field(rect, "left", Value::Float(1.0)).unwrap();
        heap.set_instance_field(rect, "top", Value::Float(2.0)).unwrap();
        heap.set_instance_field(rect, "right", Value::Float(30.0)).unwrap();
        heap.set_instance_field(rect, "bottom", Value::Float(40.0)).unwrap();
        heap.set_instance_field(paint, "mColor", Value::Int(0xff123456u32 as i32)).unwrap();
        heap.set_instance_field(paint, "mStrokeWidth", Value::Float(3.5)).unwrap();
        let style = heap.allocate_instance("Landroid/graphics/Paint$Style;").unwrap();
        let style_name = heap.allocate_string("STROKE").unwrap();
        heap.set_instance_field(style, "name", Value::Object(style_name)).unwrap();
        heap.set_instance_field(paint, "mStyleObject", Value::Object(style)).unwrap();
        let draw = registry.find("Landroid/graphics/Canvas;", "drawRoundRect",
            "(Landroid/graphics/RectF;FFLandroid/graphics/Paint;)V").unwrap().clone();
        draw(&mut JniEnv::new(&mut heap), &[
            Value::Object(canvas), Value::Object(rect), Value::Float(5.0),
            Value::Float(6.0), Value::Object(paint),
        ]).unwrap();
        assert_eq!(recorded_command_count(&heap, canvas).unwrap(), 1);
        assert_eq!(heap.get_instance_field(canvas, "mStrideCommand0Type").unwrap(), Value::Int(12));
        assert_eq!(heap.get_instance_field(canvas, "mStrideCommand0Arg4").unwrap(), Value::Float(5.0));
        assert_eq!(heap.get_instance_field(canvas, "mStrideCommand0Arg6").unwrap(), Value::Int(0xff123456u32 as i32));
        assert_eq!(heap.get_instance_field(canvas, "mStrideCommand0Arg7").unwrap(), Value::Int(1));
        assert_eq!(heap.get_instance_field(canvas, "mStrideCommand0Arg8").unwrap(), Value::Float(3.5));
    }

    #[test]
    fn records_guest_draw_round_rect_float_overload() {
        let mut registry = JniNativeRegistry::new();
        register(&mut registry);
        let mut heap = ManagedHeap::default();
        let canvas = heap.allocate_instance("Landroid/graphics/Canvas;").unwrap();
        let paint = heap.allocate_instance("Landroid/graphics/Paint;").unwrap();
        heap.set_instance_field(paint, "mColor", Value::Int(0xffabcdefu32 as i32)).unwrap();
        let draw = registry.find("Landroid/graphics/Canvas;", "drawRoundRect",
            "(FFFFFFLandroid/graphics/Paint;)V").unwrap().clone();
        draw(&mut JniEnv::new(&mut heap), &[
            Value::Object(canvas), Value::Float(1.0), Value::Float(2.0),
            Value::Float(30.0), Value::Float(40.0), Value::Float(4.0),
            Value::Float(5.0), Value::Object(paint),
        ]).unwrap();
        assert_eq!(heap.get_instance_field(canvas, "mStrideCommand0Arg6").unwrap(),
            Value::Int(0xffabcdefu32 as i32));
    }
}

pub(crate) fn record_bitmap_draw(
    env: &mut JniEnv<'_>, canvas: u32, bitmap: u32, x: f32, y: f32,
) -> Result<(), String> {
    append(env, canvas, 10, &[Value::Object(bitmap), Value::Float(x), Value::Float(y)])
}

pub(crate) fn record_bitmap_draw_bounds(
    env: &mut JniEnv<'_>, canvas: u32, bitmap: u32,
    left: i32, top: i32, right: i32, bottom: i32, alpha: i32,
) -> Result<(), String> {
    append(env, canvas, 11, &[
        Value::Object(bitmap), Value::Int(left), Value::Int(top),
        Value::Int(right), Value::Int(bottom), Value::Int(alpha.clamp(0, 255)),
    ])
}

pub(crate) fn register_core_canvas_methods(registry: &mut JniNativeRegistry) {
    register(registry);


}
