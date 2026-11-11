//! Minimal, stateful Android View/Activity framework compatibility.
//!
//! These bridges implement the object state required by common View construction
//! and Activity content installation. They deliberately do not claim to render
//! pixels or dispatch native iOS touch events; those require the host presentation
//! loop and are reported as separate compatibility work.

use crate::heap::ManagedHeap;
use crate::jni::{JniEnv, JniNativeRegistry};
use crate::types::Value;

fn receiver(args: &[Value]) -> Result<u32, String> {
    args.first().and_then(Value::as_object).filter(|id| *id != 0)
        .ok_or_else(|| "View framework call has a null or invalid receiver".to_owned())
}

fn register_view_constructor(
    jni: &mut JniNativeRegistry,
    class_descriptor: &'static str,
    prototype: &'static str,
) {
    jni.register(class_descriptor, "<init>", prototype, |_env, args| {
        let view = receiver(args)?;
        let context = args.get(1).copied().unwrap_or(Value::Null);
        _env.set_object_field(view, "mContext", context)?;
        _env.set_object_field(view, "mViewFlags", Value::Int(0x00000000))?;
        _env.set_object_field(view, "mVisibility", Value::Int(0))?;
        _env.set_object_field(view, "mEnabled", Value::Int(1))?;
        _env.set_object_field(view, "mClickable", Value::Int(0))?;
        _env.set_object_field(view, "mFocusable", Value::Int(0))?;
        _env.set_object_field(view, "mId", Value::Int(-1))?;
        _env.set_object_field(view, "mWidth", Value::Int(0))?;
        _env.set_object_field(view, "mHeight", Value::Int(0))?;
        _env.set_object_field(view, "mLeft", Value::Int(0))?;
        _env.set_object_field(view, "mTop", Value::Int(0))?;
        _env.set_object_field(view, "mRight", Value::Int(0))?;
        _env.set_object_field(view, "mBottom", Value::Int(0))?;
        _env.set_object_field(view, "mNeedsLayout", Value::Int(1))?;
        _env.set_object_field(view, "mNeedsDraw", Value::Int(1))?;
        _env.set_object_field(view, "mBackgroundColor", Value::Int(0))?;
        _env.set_object_field(view, "mText", Value::Null)?;
        _env.set_object_field(view, "mTextResourceId", Value::Int(0))?;
        _env.set_object_field(view, "mBackgroundResourceId", Value::Int(0))?;
        _env.set_object_field(view, "mTextColor", Value::Int(0xff000000u32 as i32))?;
        _env.set_object_field(view, "mTextSize", Value::Float(14.0))?;
        _env.set_object_field(view, "mOrientation", Value::Int(1))?;
        _env.set_object_field(view, "mImageResourceId", Value::Int(0))?;
        _env.set_object_field(view, "mLayoutWidth", Value::Int(-1))?;
        _env.set_object_field(view, "mLayoutHeight", Value::Int(-2))?;
        _env.set_object_field(view, "mLayoutWeight", Value::Float(0.0))?;
        _env.set_object_field(view, "mMarginLeft", Value::Int(0))?;
        _env.set_object_field(view, "mMarginTop", Value::Int(0))?;
        _env.set_object_field(view, "mMarginRight", Value::Int(0))?;
        _env.set_object_field(view, "mMarginBottom", Value::Int(0))?;
        Ok(None)
    });
}

fn register_view_framework(jni: &mut JniNativeRegistry) {
    // Android framework constructors are invoked on the concrete DEX owner.
    // Register TextView explicitly as well as View so ordinary
    // new TextView(context) applications do not fail before setContentView.
    for class_descriptor in [
        "Landroid/view/View;",
        "Landroid/view/ViewGroup;",
        "Landroid/widget/LinearLayout;",
        "Landroid/widget/TextView;",
        "Landroid/widget/Button;",
        "Landroid/widget/EditText;",
        "Landroid/widget/ImageButton;",
    ] {
        register_view_constructor(jni, class_descriptor, "(Landroid/content/Context;)V");
        register_view_constructor(jni, class_descriptor, "(Landroid/content/Context;Landroid/util/AttributeSet;)V");
        register_view_constructor(jni, class_descriptor, "(Landroid/content/Context;Landroid/util/AttributeSet;I)V");
        register_view_constructor(jni, class_descriptor, "(Landroid/content/Context;Landroid/util/AttributeSet;II)V");
    }

    jni.register("Landroid/view/View;", "getContext", "()Landroid/content/Context;", |env, args| {
        let id = receiver(args)?;
        Ok(Some(env.get_object_field(id, "mContext")?))
    });
    jni.register("Landroid/view/View;", "getId", "()I", |env, args| {
        let id = receiver(args)?;
        Ok(Some(env.get_object_field(id, "mId")?))
    });
    jni.register("Landroid/view/View;", "setId", "(I)V", |env, args| {
        let id = receiver(args)?;
        env.set_object_field(id, "mId", Value::Int(args.get(1).map(Value::as_int).unwrap_or(-1)))?;
        Ok(None)
    });
    jni.register("Landroid/view/View;", "getVisibility", "()I", |env, args| {
        let id = receiver(args)?;
        Ok(Some(env.get_object_field(id, "mVisibility")?))
    });
    jni.register("Landroid/view/View;", "setVisibility", "(I)V", |env, args| {
        let id = receiver(args)?;
        let visibility = args.get(1).map(Value::as_int).unwrap_or(0);
        if ![0, 4, 8].contains(&visibility) {
            return Err(format!("Illegal View visibility value: {}", visibility));
        }
        env.set_object_field(id, "mVisibility", Value::Int(visibility))?;
        env.set_object_field(id, "mNeedsDraw", Value::Int(1))?;
        Ok(None)
    });
    for (name, field) in [("isEnabled", "mEnabled"), ("isClickable", "mClickable"), ("isFocusable", "mFocusable")] {
        jni.register("Landroid/view/View;", name, "()Z", move |env, args| {
            let id = receiver(args)?;
            Ok(Some(env.get_object_field(id, field)?))
        });
    }
    for (name, field) in [("setEnabled", "mEnabled"), ("setClickable", "mClickable"), ("setFocusable", "mFocusable")] {
        jni.register("Landroid/view/View;", name, "(Z)V", move |env, args| {
            let id = receiver(args)?;
            let value = args.get(1).map(Value::as_int).unwrap_or(0);
            env.set_object_field(id, field, Value::Int(i32::from(value != 0)))?;
            Ok(None)
        });
    }
    jni.register("Landroid/view/View;", "setOnClickListener", "(Landroid/view/View$OnClickListener;)V", |env, args| {
        let id = receiver(args)?;
        env.set_object_field(id, "mOnClickListener", args.get(1).copied().unwrap_or(Value::Null))?;
        env.set_object_field(id, "mClickable", Value::Int(1))?;
        Ok(None)
    });
    jni.register("Landroid/view/View;", "setOnTouchListener", "(Landroid/view/View$OnTouchListener;)V", |env, args| {
        let id = receiver(args)?;
        env.set_object_field(id, "mOnTouchListener", args.get(1).copied().unwrap_or(Value::Null))?;
        Ok(None)
    });
    jni.register("Landroid/view/View;", "setTag", "(Ljava/lang/Object;)V", |env, args| {
        let id = receiver(args)?;
        env.set_object_field(id, "mTag", args.get(1).copied().unwrap_or(Value::Null))?;
        Ok(None)
    });
    jni.register("Landroid/view/View;", "getTag", "()Ljava/lang/Object;", |env, args| {
        let id = receiver(args)?;
        Ok(Some(env.get_object_field(id, "mTag")?))
    });
    jni.register("Landroid/view/View;", "setBackgroundColor", "(I)V", |env, args| {
        let id = receiver(args)?;
        env.set_object_field(id, "mBackgroundColor", Value::Int(args.get(1).map(Value::as_int).unwrap_or(0)))?;
        env.set_object_field(id, "mNeedsDraw", Value::Int(1))?;
        Ok(None)
    });

    jni.register("Landroid/view/View;","setBackgroundResource","(I)V",|env,args|{
        let id=receiver(args)?;env.set_object_field(id,"mBackgroundResourceId",Value::Int(args.get(1).map(Value::as_int).unwrap_or(0)))?;
        env.set_object_field(id,"mNeedsDraw",Value::Int(1))?;Ok(None)
    });
    jni.register("Landroid/widget/TextView;","getText","()Ljava/lang/CharSequence;",|env,args|{
        let id=receiver(args)?;Ok(Some(env.get_object_field(id,"mText")?))
    });
    jni.register("Landroid/widget/TextView;","getCurrentTextColor","()I",|env,args|{
        let id=receiver(args)?;Ok(Some(env.get_object_field(id,"mTextColor")?))
    });
    jni.register("Landroid/widget/TextView;","getTextSize","()F",|env,args|{
        let id=receiver(args)?;Ok(Some(env.get_object_field(id,"mTextSize")?))
    });
    jni.register("Landroid/widget/TextView;","addTextChangedListener","(Landroid/text/TextWatcher;)V",|env,args|{
        let id=receiver(args)?;
        env.set_object_field(id,"mTextWatcher",args.get(1).copied().unwrap_or(Value::Null))?;
        Ok(None)
    });
    jni.register("Landroid/widget/TextView;","removeTextChangedListener","(Landroid/text/TextWatcher;)V",|env,args|{
        let id=receiver(args)?;
        let current=env.get_object_field(id,"mTextWatcher")?.as_object();
        let requested=args.get(1).and_then(Value::as_object);
        if current.is_some() && current==requested {
            env.set_object_field(id,"mTextWatcher",Value::Null)?;
        }
        Ok(None)
    });
    jni.register("Landroid/widget/TextView;","getText","()Ljava/lang/CharSequence;",|env,args|{
        let id=receiver(args)?;
        Ok(Some(env.get_object_field(id,"mText")?))
    });
    jni.register("Landroid/widget/TextView;","setText","(Ljava/lang/CharSequence;)V",|env,args|{
        let id=receiver(args)?;
        env.set_object_field(id,"mText",args.get(1).copied().unwrap_or(Value::Null))?;
        // A CharSequence update replaces any XML text resource, including null
        // (which means clear the display), rather than falling back to stale text.
        env.set_object_field(id,"mTextResourceId",Value::Int(0))?;
        env.set_object_field(id,"mNeedsDraw",Value::Int(1))?;Ok(None)
    });
    jni.register("Landroid/widget/TextView;","setText","(Ljava/lang/CharSequence;Landroid/widget/TextView$BufferType;)V",|env,args|{
        let id=receiver(args)?;
        env.set_object_field(id,"mText",args.get(1).copied().unwrap_or(Value::Null))?;
        env.set_object_field(id,"mTextResourceId",Value::Int(0))?;
        env.set_object_field(id,"mNeedsDraw",Value::Int(1))?;Ok(None)
    });
    jni.register("Landroid/widget/TextView;","setText","(I)V",|env,args|{
        let id=receiver(args)?;env.set_object_field(id,"mText",Value::Null)?;
        env.set_object_field(id,"mTextResourceId",Value::Int(args.get(1).map(Value::as_int).unwrap_or(0)))?;
        env.set_object_field(id,"mNeedsDraw",Value::Int(1))?;Ok(None)
    });
    jni.register("Landroid/widget/TextView;","setTextColor","(I)V",|env,args|{
        let id=receiver(args)?;env.set_object_field(id,"mTextColor",Value::Int(args.get(1).map(Value::as_int).unwrap_or(0xff000000u32 as i32)))?;
        env.set_object_field(id,"mNeedsDraw",Value::Int(1))?;Ok(None)
    });
    jni.register("Landroid/widget/TextView;","setTextSize","(IF)V",|env,args|{
        let id=receiver(args)?;let size=args.get(2).map(Value::as_float).unwrap_or(14.0);
        if size.is_finite()&&size>0.0{env.set_object_field(id,"mTextSize",Value::Float(size))?;}
        env.set_object_field(id,"mNeedsDraw",Value::Int(1))?;Ok(None)
    });
    jni.register("Landroid/widget/TextView;","setTextSize","(F)V",|env,args|{
        let id=receiver(args)?;let size=args.get(1).map(Value::as_float).unwrap_or(14.0);
        if size.is_finite()&&size>0.0{env.set_object_field(id,"mTextSize",Value::Float(size))?;}
        env.set_object_field(id,"mNeedsDraw",Value::Int(1))?;Ok(None)
    });
    jni.register("Landroid/widget/ImageView;","setImageResource","(I)V",|env,args|{
        let id=receiver(args)?;env.set_object_field(id,"mImageResourceId",Value::Int(args.get(1).map(Value::as_int).unwrap_or(0)))?;
        env.set_object_field(id,"mNeedsDraw",Value::Int(1))?;Ok(None)
    });
    jni.register("Landroid/widget/LinearLayout;","setOrientation","(I)V",|env,args|{
        let id=receiver(args)?;let orientation=args.get(1).map(Value::as_int).unwrap_or(1);
        env.set_object_field(id,"mOrientation",Value::Int(if orientation==0{0}else{1}))?;
        env.set_object_field(id,"mNeedsLayout",Value::Int(1))?;Ok(None)
    });

    jni.register("Landroid/view/View;","setLayoutParams","(Landroid/view/ViewGroup$LayoutParams;)V",|env,args|{
        let view=receiver(args)?;let Some(params)=args.get(1).and_then(Value::as_object)else{return Err("setLayoutParams received null".into())};
        for (src,dst) in [("width","mLayoutWidth"),("height","mLayoutHeight"),("weight","mLayoutWeight"),("leftMargin","mMarginLeft"),("topMargin","mMarginTop"),("rightMargin","mMarginRight"),("bottomMargin","mMarginBottom")] {
            if let Ok(value)=env.get_object_field(params,src){env.set_object_field(view,dst,value)?;}
        }
        env.set_object_field(view,"mNeedsLayout",Value::Int(1))?;Ok(None)
    });
    jni.register("Landroid/view/ViewGroup$LayoutParams;","<init>","(II)V",|env,args|{
        let params=receiver(args)?;env.set_object_field(params,"width",Value::Int(args.get(1).map(Value::as_int).unwrap_or(-1)))?;
        env.set_object_field(params,"height",Value::Int(args.get(2).map(Value::as_int).unwrap_or(-2)))?;Ok(None)
    });
    jni.register("Landroid/widget/LinearLayout$LayoutParams;","<init>","(IIF)V",|env,args|{
        let params=receiver(args)?;env.set_object_field(params,"width",Value::Int(args.get(1).map(Value::as_int).unwrap_or(-1)))?;
        env.set_object_field(params,"height",Value::Int(args.get(2).map(Value::as_int).unwrap_or(-2)))?;
        let weight=args.get(3).map(Value::as_float).unwrap_or(0.0);env.set_object_field(params,"weight",Value::Float(if weight.is_finite(){weight.max(0.0)}else{0.0}))?;Ok(None)
    });
    jni.register("Landroid/view/View;", "getWidth", "()I", |env, args| {
        let id = receiver(args)?;
        Ok(Some(env.get_object_field(id, "mWidth")?))
    });
    jni.register("Landroid/view/View;", "getHeight", "()I", |env, args| {
        let id = receiver(args)?;
        Ok(Some(env.get_object_field(id, "mHeight")?))
    });
    jni.register("Landroid/view/View;", "getMeasuredWidth", "()I", |env, args| {
        let id = receiver(args)?;
        Ok(Some(env.get_object_field(id, "mWidth")?))
    });
    jni.register("Landroid/view/View;", "getMeasuredHeight", "()I", |env, args| {
        let id = receiver(args)?;
        Ok(Some(env.get_object_field(id, "mHeight")?))
    });
    jni.register("Landroid/view/View;", "getLeft", "()I", |env, args| {
        let id = receiver(args)?;
        Ok(Some(env.get_object_field(id, "mLeft")?))
    });
    jni.register("Landroid/view/View;", "getTop", "()I", |env, args| {
        let id = receiver(args)?;
        Ok(Some(env.get_object_field(id, "mTop")?))
    });
    jni.register("Landroid/view/View;", "getRight", "()I", |env, args| {
        let id = receiver(args)?;
        Ok(Some(env.get_object_field(id, "mRight")?))
    });
    jni.register("Landroid/view/View;", "getBottom", "()I", |env, args| {
        let id = receiver(args)?;
        Ok(Some(env.get_object_field(id, "mBottom")?))
    });
    jni.register("Landroid/view/View;", "invalidate", "()V", |env, args| {
        let id = receiver(args)?;
        env.set_object_field(id, "mNeedsDraw", Value::Int(1))?;
        Ok(None)
    });
    jni.register("Landroid/view/View;", "setOnApplyWindowInsetsListener",
        "(Landroid/view/View$OnApplyWindowInsetsListener;)V", |env, args| {
            let view = receiver(args)?;
            env.set_object_field(view, "mWindowInsetsListener", args.get(1).copied().unwrap_or(Value::Null))?;
            Ok(None)
        });
    jni.register("Landroid/view/View;", "setSystemUiVisibility", "(I)V", |env, args| {
        let view = receiver(args)?;
        env.set_object_field(view, "mSystemUiVisibility", Value::Int(args.get(1).map(Value::as_int).unwrap_or(0)))?;
        Ok(None)
    });
    jni.register("Landroid/view/View;", "getSystemUiVisibility", "()I", |env, args| {
        let view = receiver(args)?;
        Ok(Some(env.get_object_field(view, "mSystemUiVisibility")?))
    });
    jni.register("Landroid/view/View;", "requestLayout", "()V", |env, args| {
        let id = receiver(args)?;
        env.set_object_field(id, "mNeedsLayout", Value::Int(1))?;
        Ok(None)
    });
    jni.register("Landroid/view/View;", "findViewById", "(I)Landroid/view/View;", |env, args| {
        let id = receiver(args)?;
        let wanted = args.get(1).map(Value::as_int).unwrap_or(-1);
        let own = env.get_object_field(id, "mId")?.as_int();
        Ok(Some(if wanted == own { Value::Object(id) } else { Value::Null }))
    });

    // Ordered managed-object children for common ViewGroup subclasses.
    // This stores hierarchy only; it does not claim layout inflation or drawing.
    jni.register("Landroid/view/ViewGroup;", "addView", "(Landroid/view/View;)V", |env, args| {
        let parent = receiver(args)?;
        let child = args.get(1).and_then(Value::as_object).ok_or_else(|| "ViewGroup.addView received a null child".to_owned())?;
        if parent == child || !env.heap.contains(child) { return Err("ViewGroup.addView received an invalid child".to_owned()); }
        let count = env.get_object_field(parent, "mChildCount")?.as_int().max(0) as usize;
        env.set_object_field(parent, &format!("mChild{}", count), Value::Object(child))?;
        env.set_object_field(parent, "mChildCount", Value::Int((count + 1) as i32))?;
        env.set_object_field(parent, "mNeedsLayout", Value::Int(1))?;
        env.set_object_field(parent, "mNeedsDraw", Value::Int(1))?;
        Ok(None)
    });
    jni.register("Landroid/view/ViewGroup;", "addView", "(Landroid/view/View;I)V", |env, args| {
        let parent = receiver(args)?;
        let child = args.get(1).and_then(Value::as_object).ok_or_else(|| "ViewGroup.addView received a null child".to_owned())?;
        if parent == child || !env.heap.contains(child) { return Err("ViewGroup.addView received an invalid child".to_owned()); }
        let count = env.get_object_field(parent, "mChildCount")?.as_int().max(0) as usize;
        let requested = args.get(2).map(Value::as_int).unwrap_or(count as i32);
        let index = if requested < 0 { count } else { (requested as usize).min(count) };
        for slot in (index..count).rev() {
            let previous = env.get_object_field(parent, &format!("mChild{}", slot))?;
            env.set_object_field(parent, &format!("mChild{}", slot + 1), previous)?;
        }
        env.set_object_field(parent, &format!("mChild{}", index), Value::Object(child))?;
        env.set_object_field(parent, "mChildCount", Value::Int((count + 1) as i32))?;
        env.set_object_field(parent, "mNeedsLayout", Value::Int(1))?;
        env.set_object_field(parent, "mNeedsDraw", Value::Int(1))?;
        Ok(None)
    });
    jni.register("Landroid/view/ViewGroup;", "removeView", "(Landroid/view/View;)V", |env, args| {
        let parent = receiver(args)?;
        let child = args.get(1).and_then(Value::as_object).ok_or_else(|| "ViewGroup.removeView received a null child".to_owned())?;
        let count = env.get_object_field(parent, "mChildCount")?.as_int().clamp(0, 4096) as usize;
        let mut found = None;
        for index in 0..count {
            if env.get_object_field(parent, &format!("mChild{}", index))?.as_object() == Some(child) {
                found = Some(index);
                break;
            }
        }
        if let Some(index) = found {
            for slot in index..count.saturating_sub(1) {
                let next = env.get_object_field(parent, &format!("mChild{}", slot + 1))?;
                env.set_object_field(parent, &format!("mChild{}", slot), next)?;
            }
            env.set_object_field(parent, &format!("mChild{}", count - 1), Value::Null)?;
            env.set_object_field(parent, "mChildCount", Value::Int((count - 1) as i32))?;
            env.set_object_field(parent, "mNeedsLayout", Value::Int(1))?;
            env.set_object_field(parent, "mNeedsDraw", Value::Int(1))?;
        }
        Ok(None)
    });
    jni.register("Landroid/view/ViewGroup;", "removeAllViews", "()V", |env, args| {
        let parent = receiver(args)?;
        let count = env.get_object_field(parent, "mChildCount")?.as_int().clamp(0, 4096) as usize;
        for index in 0..count { env.set_object_field(parent, &format!("mChild{}", index), Value::Null)?; }
        env.set_object_field(parent, "mChildCount", Value::Int(0))?;
        env.set_object_field(parent, "mNeedsLayout", Value::Int(1))?;
        env.set_object_field(parent, "mNeedsDraw", Value::Int(1))?;
        Ok(None)
    });
    jni.register("Landroid/view/ViewGroup;", "getChildCount", "()I", |env, args| {
        let parent = receiver(args)?;
        Ok(Some(env.get_object_field(parent, "mChildCount")?))
    });
    jni.register("Landroid/view/ViewGroup;", "getChildAt", "(I)Landroid/view/View;", |env, args| {
        let parent = receiver(args)?;
        let index = args.get(1).map(Value::as_int).unwrap_or(-1);
        let count = env.get_object_field(parent, "mChildCount")?.as_int();
        if index < 0 || index >= count { return Ok(Some(Value::Null)); }
        Ok(Some(env.get_object_field(parent, &format!("mChild{}", index))?))
    });

    // Activity content ownership is real managed-object state. Layout-resource
    // inflation and recursive ViewGroup traversal are intentionally not faked.
    jni.register("Landroid/app/Activity;", "setContentView", "(Landroid/view/View;)V", |env, args| {
        let activity = receiver(args)?;
        let content = args.get(1).copied().unwrap_or(Value::Null);
        if let Some(view_id) = content.as_object() {
            if !env.heap.contains(view_id) {
                return Err(format!("setContentView received invalid View handle {}", view_id));
            }
        }
        env.set_object_field(activity, "mContentView", content)?;
        env.set_object_field(activity, "mContentLayoutResource", Value::Int(0))?;
        Ok(None)
    });
    jni.register("Landroid/app/Activity;", "setContentView", "(Landroid/view/View;Landroid/view/ViewGroup$LayoutParams;)V", |env, args| {
        let activity = receiver(args)?;
        let content = args.get(1).copied().unwrap_or(Value::Null);
        if let Some(view_id) = content.as_object() {
            if !env.heap.contains(view_id) {
                return Err(format!("setContentView received invalid View handle {}", view_id));
            }
        }
        env.set_object_field(activity, "mContentView", content)?;
        env.set_object_field(activity, "mContentLayoutResource", Value::Int(0))?;
        Ok(None)
    });
    jni.register("Landroid/app/Activity;", "setContentView", "(I)V", |env, args| {
        let activity = receiver(args)?;
        let layout_id = args.get(1).map(Value::as_int).unwrap_or(0);
        env.set_object_field(activity, "mContentView", Value::Null)?;
        env.set_object_field(activity, "mContentLayoutResource", Value::Int(layout_id))?;
        Ok(None)
    });
    jni.register("Landroid/app/Activity;", "findViewById", "(I)Landroid/view/View;", |env, args| {
        let activity = receiver(args)?;
        let wanted = args.get(1).map(Value::as_int).unwrap_or(-1);
        let root = env.get_object_field(activity, "mContentView")?;
        let Some(root_id) = root.as_object() else { return Ok(Some(Value::Null)); };
        fn search(env: &JniEnv<'_>, node: u32, wanted: i32, seen: &mut std::collections::HashSet<u32>) -> Result<Value, String> {
            if !seen.insert(node) { return Ok(Value::Null); }
            if env.get_object_field(node, "mId")?.as_int() == wanted { return Ok(Value::Object(node)); }
            let count = env.get_object_field(node, "mChildCount")?.as_int().clamp(0, 4096) as usize;
            for index in 0..count {
                if let Some(child) = env.get_object_field(node, &format!("mChild{}", index))?.as_object() {
                    let found = search(env, child, wanted, seen)?;
                    if !found.is_null() { return Ok(found); }
                }
            }
            Ok(Value::Null)
        }
        Ok(Some(search(env, root_id, wanted, &mut std::collections::HashSet::new())?))
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn invoke(
        jni: &JniNativeRegistry,
        heap: &mut ManagedHeap,
        class: &str,
        name: &str,
        proto: &str,
        args: &[Value],
    ) -> Result<Option<Value>, String> {
        let handler = jni.find(class, name, proto).cloned()
            .ok_or_else(|| format!("missing registered method {}->{}{}", class, name, proto))?;
        let mut env = JniEnv::new(heap);
        handler(&mut env, args)
    }

    #[test]
    fn view_constructor_initializes_context_and_default_state() {
        let mut jni = JniNativeRegistry::new();
        register_view_framework(&mut jni);
        let mut heap = ManagedHeap::default();
        let context = heap.allocate_instance("Landroid/app/Activity;").unwrap();
        let view = heap.allocate_instance("Lgame/CustomView;").unwrap();
        invoke(&jni, &mut heap, "Landroid/view/View;", "<init>", "(Landroid/content/Context;)V",
            &[Value::Object(view), Value::Object(context)]).unwrap();
        assert_eq!(heap.get_instance_field(view, "mContext").unwrap(), Value::Object(context));
        assert_eq!(heap.get_instance_field(view, "mVisibility").unwrap(), Value::Int(0));
        assert_eq!(heap.get_instance_field(view, "mEnabled").unwrap(), Value::Int(1));
    }

    #[test]
    fn linear_layout_context_constructor_initializes_managed_view_state() {
        let mut jni = JniNativeRegistry::new();
        register_view_framework(&mut jni);
        let mut heap = ManagedHeap::default();
        let layout = heap.allocate_instance("Landroid/widget/LinearLayout;").unwrap();
        let context = heap.allocate_instance("Landroid/app/Activity;").unwrap();

        invoke(&jni, &mut heap, "Landroid/widget/LinearLayout;", "<init>",
            "(Landroid/content/Context;)V",
            &[Value::Object(layout), Value::Object(context)]).unwrap();

        assert_eq!(heap.get_instance_field(layout, "mContext").unwrap(), Value::Object(context));
        assert_eq!(heap.get_instance_field(layout, "mOrientation").unwrap(), Value::Int(1));
        assert_eq!(heap.get_instance_field(layout, "mNeedsLayout").unwrap(), Value::Int(1));
    }

    #[test]
    fn view_state_methods_round_trip_values() {
        let mut jni = JniNativeRegistry::new();
        register_view_framework(&mut jni);
        let mut heap = ManagedHeap::default();
        let view = heap.allocate_instance("Lgame/CustomView;").unwrap();
        invoke(&jni, &mut heap, "Landroid/view/View;", "<init>", "(Landroid/content/Context;)V",
            &[Value::Object(view), Value::Null]).unwrap();
        invoke(&jni, &mut heap, "Landroid/view/View;", "setId", "(I)V",
            &[Value::Object(view), Value::Int(42)]).unwrap();
        invoke(&jni, &mut heap, "Landroid/view/View;", "setVisibility", "(I)V",
            &[Value::Object(view), Value::Int(4)]).unwrap();
        assert_eq!(invoke(&jni, &mut heap, "Landroid/view/View;", "getId", "()I",
            &[Value::Object(view)]).unwrap(), Some(Value::Int(42)));
        assert_eq!(invoke(&jni, &mut heap, "Landroid/view/View;", "getVisibility", "()I",
            &[Value::Object(view)]).unwrap(), Some(Value::Int(4)));
    }

    #[test]
    fn view_group_supports_indexed_insert_remove_and_clear() {
        let mut jni = JniNativeRegistry::new();
        register_view_framework(&mut jni);
        let mut heap = ManagedHeap::default();
        let parent = heap.allocate_instance("Landroid/widget/LinearLayout;").unwrap();
        let first = heap.allocate_instance("Landroid/widget/TextView;").unwrap();
        let second = heap.allocate_instance("Landroid/widget/Button;").unwrap();
        invoke(&jni, &mut heap, "Landroid/view/ViewGroup;", "addView", "(Landroid/view/View;)V",
            &[Value::Object(parent), Value::Object(first)]).unwrap();
        invoke(&jni, &mut heap, "Landroid/view/ViewGroup;", "addView", "(Landroid/view/View;I)V",
            &[Value::Object(parent), Value::Object(second), Value::Int(0)]).unwrap();
        assert_eq!(invoke(&jni, &mut heap, "Landroid/view/ViewGroup;", "getChildAt", "(I)Landroid/view/View;",
            &[Value::Object(parent), Value::Int(0)]).unwrap(), Some(Value::Object(second)));
        invoke(&jni, &mut heap, "Landroid/view/ViewGroup;", "removeView", "(Landroid/view/View;)V",
            &[Value::Object(parent), Value::Object(second)]).unwrap();
        assert_eq!(invoke(&jni, &mut heap, "Landroid/view/ViewGroup;", "getChildCount", "()I",
            &[Value::Object(parent)]).unwrap(), Some(Value::Int(1)));
        invoke(&jni, &mut heap, "Landroid/view/ViewGroup;", "removeAllViews", "()V",
            &[Value::Object(parent)]).unwrap();
        assert_eq!(invoke(&jni, &mut heap, "Landroid/view/ViewGroup;", "getChildCount", "()I",
            &[Value::Object(parent)]).unwrap(), Some(Value::Int(0)));
    }

    #[test]
    fn common_button_and_edit_text_constructors_are_registered() {
        let mut jni = JniNativeRegistry::new();
        register_view_framework(&mut jni);
        for class in [
            "Landroid/widget/Button;",
            "Landroid/widget/EditText;",
            "Landroid/widget/ImageButton;",
        ] {
            assert!(
                jni.find(class, "<init>", "(Landroid/content/Context;)V").is_some(),
                "missing managed constructor bridge for {}",
                class
            );
        }
    }

    #[test]
    fn text_view_context_constructor_initializes_managed_view_state() {
        let mut jni = JniNativeRegistry::new();
        register_view_framework(&mut jni);
        let mut heap = ManagedHeap::default();
        let activity = heap.allocate_instance("Lgame/MainActivity;").unwrap();
        let text = heap.allocate_instance("Landroid/widget/TextView;").unwrap();

        invoke(
            &jni,
            &mut heap,
            "Landroid/widget/TextView;",
            "<init>",
            "(Landroid/content/Context;)V",
            &[Value::Object(text), Value::Object(activity)],
        ).unwrap();

        assert_eq!(heap.get_instance_field(text, "mContext").unwrap(), Value::Object(activity));
        assert_eq!(heap.get_instance_field(text, "mTextColor").unwrap(), Value::Int(0xff000000u32 as i32));
        assert_eq!(heap.get_instance_field(text, "mNeedsDraw").unwrap(), Value::Int(1));
    }

    #[test]
    fn text_view_text_and_style_getters_return_managed_state() {
        let mut jni = JniNativeRegistry::new();
        register_view_framework(&mut jni);
        let mut heap = ManagedHeap::default();
        let view = heap.allocate_instance("Landroid/widget/TextView;").unwrap();
        let text = heap.allocate_string("Tap Trail").unwrap();

        invoke(&jni, &mut heap, "Landroid/widget/TextView;", "<init>",
            "(Landroid/content/Context;)V", &[Value::Object(view), Value::Null]).unwrap();
        invoke(&jni, &mut heap, "Landroid/widget/TextView;", "setText",
            "(Ljava/lang/CharSequence;)V", &[Value::Object(view), Value::Object(text)]).unwrap();
        invoke(&jni, &mut heap, "Landroid/widget/TextView;", "setTextColor",
            "(I)V", &[Value::Object(view), Value::Int(0xff12ab34u32 as i32)]).unwrap();
        invoke(&jni, &mut heap, "Landroid/widget/TextView;", "setTextSize",
            "(F)V", &[Value::Object(view), Value::Float(22.0)]).unwrap();

        assert_eq!(invoke(&jni, &mut heap, "Landroid/widget/TextView;", "getText",
            "()Ljava/lang/CharSequence;", &[Value::Object(view)]).unwrap(),
            Some(Value::Object(text)));
        assert_eq!(invoke(&jni, &mut heap, "Landroid/widget/TextView;", "getCurrentTextColor",
            "()I", &[Value::Object(view)]).unwrap(), Some(Value::Int(0xff12ab34u32 as i32)));
        assert_eq!(invoke(&jni, &mut heap, "Landroid/widget/TextView;", "getTextSize",
            "()F", &[Value::Object(view)]).unwrap(), Some(Value::Float(22.0)));
    }

    #[test]
    fn view_background_color_is_stored_for_rendering() {
        let mut jni = JniNativeRegistry::new();
        register_view_framework(&mut jni);
        let mut heap = ManagedHeap::default();
        let view = heap.allocate_instance("Landroid/widget/TextView;").unwrap();
        invoke(&jni, &mut heap, "Landroid/view/View;", "<init>",
            "(Landroid/content/Context;)V", &[Value::Object(view), Value::Null]).unwrap();
        let color = 0xff35c9ffu32 as i32;
        invoke(&jni, &mut heap, "Landroid/view/View;", "setBackgroundColor",
            "(I)V", &[Value::Object(view), Value::Int(color)]).unwrap();
        assert_eq!(heap.get_instance_field(view, "mBackgroundColor").unwrap(), Value::Int(color));
        assert_eq!(heap.get_instance_field(view, "mNeedsDraw").unwrap(), Value::Int(1));
    }

    #[test]
    fn activity_owns_content_view_and_finds_root_id() {
        let mut jni = JniNativeRegistry::new();
        register_view_framework(&mut jni);
        let mut heap = ManagedHeap::default();
        let activity = heap.allocate_instance("Lgame/MainActivity;").unwrap();
        let view = heap.allocate_instance("Lgame/CustomView;").unwrap();
        invoke(&jni, &mut heap, "Landroid/view/View;", "<init>", "(Landroid/content/Context;)V",
            &[Value::Object(view), Value::Object(activity)]).unwrap();
        invoke(&jni, &mut heap, "Landroid/view/View;", "setId", "(I)V",
            &[Value::Object(view), Value::Int(7)]).unwrap();
        invoke(&jni, &mut heap, "Landroid/app/Activity;", "setContentView", "(Landroid/view/View;)V",
            &[Value::Object(activity), Value::Object(view)]).unwrap();
        assert_eq!(invoke(&jni, &mut heap, "Landroid/app/Activity;", "findViewById", "(I)Landroid/view/View;",
            &[Value::Object(activity), Value::Int(7)]).unwrap(), Some(Value::Object(view)));
    }
}

/// Ensure the module's registration remains usable from the runtime bootstrap.
pub(crate) fn register_core_view_methods(jni: &mut JniNativeRegistry) {
    register_view_framework(jni);
}
