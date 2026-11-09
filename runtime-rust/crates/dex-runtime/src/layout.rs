//! Safe structural inflation of decoded Android XML into a runtime-neutral view tree.
//! Resource IDs and theme-dependent values are intentionally retained as unresolved
//! attributes; this layer never fabricates resolved Android resources.

use std::collections::BTreeMap;
use crate::XmlNode;
use crate::graphics::Surface;
use crate::canvas::{Canvas, Paint, PaintStyle, RectF};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LayoutDimension {
    MatchParent,
    WrapContent,
    Pixels(f32),
}

impl LayoutDimension {
    pub fn parse(value: &str) -> Option<Self> {
        match value.trim() {
            "match_parent" | "fill_parent" => Some(Self::MatchParent),
            "wrap_content" => Some(Self::WrapContent),
            raw => {
                let number = raw.strip_suffix("dp")
                    .or_else(|| raw.strip_suffix("dip"))
                    .or_else(|| raw.strip_suffix("px"))?
                    .trim().parse::<f32>().ok()?;
                if number.is_finite() && number >= 0.0 {
                    Some(Self::Pixels(number))
                } else {
                    None
                }
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum InflatedViewKind {
    View,
    ViewGroup,
    LinearLayout,
    FrameLayout,
    RelativeLayout,
    TextView,
    Button,
    ImageView,
    EditText,
    ScrollView,
    Custom(String),
}

impl InflatedViewKind {
    fn from_tag(tag: &str) -> Self {
        let short = tag.rsplit('.').next().unwrap_or(tag);
        match short {
            "View" => Self::View,
            "ViewGroup" => Self::ViewGroup,
            "LinearLayout" => Self::LinearLayout,
            "FrameLayout" => Self::FrameLayout,
            "RelativeLayout" => Self::RelativeLayout,
            "TextView" => Self::TextView,
            "Button" => Self::Button,
            "ImageView" => Self::ImageView,
            "EditText" => Self::EditText,
            "ScrollView" => Self::ScrollView,
            _ => Self::Custom(tag.to_owned()),
        }
    }

    pub fn is_container(&self) -> bool {
        matches!(self, Self::ViewGroup | Self::LinearLayout | Self::FrameLayout
            | Self::RelativeLayout | Self::ScrollView)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct InflatedView {
    pub kind: InflatedViewKind,
    pub source_tag: String,
    pub attributes: BTreeMap<String, String>,
    pub layout_width: Option<LayoutDimension>,
    pub layout_height: Option<LayoutDimension>,
    pub children: Vec<InflatedView>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InflateError {
    InvalidRoot,
    NonContainerHasChildren(String),
    TooDeep,
    TooManyNodes,
}

const MAX_LAYOUT_DEPTH: usize = 128;
const MAX_LAYOUT_NODES: usize = 100_000;

fn inflate_node(node: &XmlNode, depth: usize, count: &mut usize) -> Result<InflatedView, InflateError> {
    if depth > MAX_LAYOUT_DEPTH {
        return Err(InflateError::TooDeep);
    }
    *count = count.checked_add(1).ok_or(InflateError::TooManyNodes)?;
    if *count > MAX_LAYOUT_NODES {
        return Err(InflateError::TooManyNodes);
    }
    let kind = InflatedViewKind::from_tag(&node.name);
    if !node.children.is_empty() && !kind.is_container() {
        return Err(InflateError::NonContainerHasChildren(node.name.clone()));
    }
    let mut children = Vec::with_capacity(node.children.len());
    for child in &node.children {
        children.push(inflate_node(child, depth + 1, count)?);
    }
    Ok(InflatedView {
        kind,
        source_tag: node.name.clone(),
        layout_width: node.attributes.get("layout_width").and_then(|v| LayoutDimension::parse(v)),
        layout_height: node.attributes.get("layout_height").and_then(|v| LayoutDimension::parse(v)),
        attributes: node.attributes.clone(),
        children,
    })
}

/// Converts a parsed Android layout tree into a bounded, typed intermediate view tree.
/// This is the inflater's structural stage; resource/theme resolution and managed-heap
/// View construction are separate runtime stages and are not implied by this function.
pub fn inflate_layout_tree(root: &XmlNode) -> Result<InflatedView, InflateError> {
    if root.name.is_empty() {
        return Err(InflateError::InvalidRoot);
    }
    let mut node_count = 0usize;
    inflate_node(root, 0, &mut node_count)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(name: &str, children: Vec<XmlNode>) -> XmlNode {
        XmlNode {
            name: name.to_owned(),
            attributes: BTreeMap::from([
                ("layout_width".to_owned(), "match_parent".to_owned()),
                ("layout_height".to_owned(), "48dp".to_owned()),
            ]),
            text: None,
            children,
        }
    }

    #[test]
    fn inflates_common_layout_tags_and_preserves_attributes() {
        let root = node("LinearLayout", vec![node("TextView", vec![])]);
        let inflated = inflate_layout_tree(&root).unwrap();
        assert_eq!(inflated.kind, InflatedViewKind::LinearLayout);
        assert_eq!(inflated.layout_width, Some(LayoutDimension::MatchParent));
        assert_eq!(inflated.children[0].kind, InflatedViewKind::TextView);
        assert_eq!(inflated.children[0].layout_height, Some(LayoutDimension::Pixels(48.0)));
    }

    #[test]
    fn rejects_children_on_leaf_views_and_unknown_dimensions_remain_unresolved() {
        let root = node("TextView", vec![node("Button", vec![])]);
        assert_eq!(inflate_layout_tree(&root), Err(InflateError::NonContainerHasChildren("TextView".into())));
        assert_eq!(LayoutDimension::parse("12sp"), None);
        assert_eq!(LayoutDimension::parse("-3dp"), None);
    }

    #[test]
    fn keeps_custom_view_class_names_without_claiming_support() {
        let root = node("com.example.CustomWidget", vec![]);
        let inflated = inflate_layout_tree(&root).unwrap();
        assert_eq!(inflated.kind, InflatedViewKind::Custom("com.example.CustomWidget".into()));
    }
}


/// Pixel-space bounds produced by the first-pass Android ViewGroup layout engine.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LayoutRect {
    pub left: f32,
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
}

impl LayoutRect {
    pub fn width(self) -> f32 { (self.right - self.left).max(0.0) }
    pub fn height(self) -> f32 { (self.bottom - self.top).max(0.0) }
}

#[derive(Debug, Clone, PartialEq)]
pub struct LaidOutView {
    pub kind: InflatedViewKind,
    pub bounds: LayoutRect,
    pub background_argb: Option<u32>, pub text:Option<String>, pub text_argb:u32, pub text_size:f32,
    pub image_resource_id:Option<u32>, pub children:Vec<LaidOutView>,
}

fn attr_dimension(view: &InflatedView, key: &str, available: f32, fallback: f32) -> f32 {
    match view.attributes.get(key).and_then(|v| LayoutDimension::parse(v)) {
        Some(LayoutDimension::Pixels(value)) => value.min(available),
        Some(LayoutDimension::MatchParent) => available,
        Some(LayoutDimension::WrapContent) | None => fallback.min(available),
    }
}

fn parse_argb(raw:Option<&String>)->u32{let Some(raw)=raw else{return 0xff000000};let Some(hex)=raw.trim().strip_prefix("#")else{return 0xff000000};match hex.len(){6=>u32::from_str_radix(hex,16).map(|v|0xff000000|v).unwrap_or(0xff000000),8=>u32::from_str_radix(hex,16).unwrap_or(0xff000000),_=>0xff000000}}
fn parse_text_size(raw:Option<&String>)->f32{raw.and_then(|v|v.trim_end_matches("sp").trim_end_matches("dp").parse::<f32>().ok()).filter(|v|v.is_finite()&&*v>0.0).unwrap_or(14.0)}
fn parse_resource_id(raw:Option<&String>)->Option<u32>{raw.and_then(|v|v.strip_prefix("@0x").or_else(||v.strip_prefix("0x"))).and_then(|v|u32::from_str_radix(v,16).ok())}
fn parse_background(view: &InflatedView) -> Option<u32> {
    let raw = view.attributes.get("background")?.trim();
    let hex = raw.strip_prefix("#")?;
    match hex.len() {
        6 => u32::from_str_radix(hex, 16).ok().map(|rgb| 0xff00_0000 | rgb),
        8 => u32::from_str_radix(hex, 16).ok(),
        _ => None,
    }
}

fn layout_node(view: &InflatedView, bounds: LayoutRect, depth: usize) -> LaidOutView {
    if depth >= MAX_LAYOUT_DEPTH || view.children.is_empty() {
        return LaidOutView{kind:view.kind.clone(),bounds,background_argb:parse_background(view),text:view.attributes.get("text").cloned(),text_argb:parse_argb(view.attributes.get("textColor")),text_size:parse_text_size(view.attributes.get("textSize")),image_resource_id:parse_resource_id(view.attributes.get("src")),children:Vec::new()};
    }

    let vertical = view.attributes.get("orientation").map(|v| v != "horizontal").unwrap_or(true);
    let is_linear = matches!(view.kind, InflatedViewKind::LinearLayout);
    let mut children = Vec::with_capacity(view.children.len());
    let count=view.children.len()as f32;let mut remaining=if vertical{bounds.height()}else{bounds.width()};
    let mut remaining_weight:f32=view.children.iter().map(|c|c.attributes.get("layout_weight").and_then(|v|v.parse::<f32>().ok()).filter(|v|v.is_finite()&&*v>0.0).unwrap_or(0.0)).sum();
    let segment=remaining/count;
    let mut cursor = if vertical { bounds.top } else { bounds.left };

    for child in &view.children {
        let available_width = bounds.width();
        let available_height = bounds.height();
        let child_width = attr_dimension(child, "layout_width", available_width, available_width);
        let child_height = attr_dimension(child, "layout_height", available_height, segment);
        let weight=child.attributes.get("layout_weight").and_then(|v|v.parse::<f32>().ok()).filter(|v|v.is_finite()&&*v>0.0).unwrap_or(0.0);
        let extent=if weight>0.0&&remaining_weight>0.0{remaining*weight/remaining_weight}else{segment};
        let margin=child.attributes.get("layout_margin").and_then(|v|v.trim_end_matches("dp").parse::<f32>().ok()).filter(|v|v.is_finite()&&*v>=0.0).unwrap_or(0.0);
        let child_bounds = if is_linear {
            if vertical {
                let height=if weight>0.0{extent}else if matches!(child.layout_height,Some(LayoutDimension::Pixels(_))){child_height.min(extent)}else{extent};
                let rect=LayoutRect{left:bounds.left+margin,top:cursor+margin,right:bounds.left+child_width-margin,bottom:cursor+height-margin};
                cursor+=extent;remaining=(remaining-extent).max(0.0);remaining_weight=(remaining_weight-weight).max(0.0);
                rect
            } else {
                let width=if weight>0.0{extent}else if matches!(child.layout_width,Some(LayoutDimension::Pixels(_))){child_width.min(extent)}else{extent};
                let rect=LayoutRect{left:cursor+margin,top:bounds.top+margin,right:cursor+width-margin,bottom:bounds.top+child_height-margin};
                cursor+=extent;remaining=(remaining-extent).max(0.0);remaining_weight=(remaining_weight-weight).max(0.0);
                rect
            }
        } else {
            LayoutRect { left: bounds.left, top: bounds.top, right: bounds.left + child_width, bottom: bounds.top + child_height }
        };
        children.push(layout_node(child, child_bounds, depth + 1));
    }

    LaidOutView{kind:view.kind.clone(),bounds,background_argb:parse_background(view),text:view.attributes.get("text").cloned(),text_argb:parse_argb(view.attributes.get("textColor")),text_size:parse_text_size(view.attributes.get("textSize")),image_resource_id:parse_resource_id(view.attributes.get("src")),children}
}

/// Computes bounded common ViewGroup layout with LinearLayout weight and uniform margins. Per-edge margins, gravity, baseline alignment and resource dimensions remain pending.
pub fn measure_and_layout(root: &InflatedView, width: u32, height: u32) -> LaidOutView {
    let bounds = LayoutRect { left: 0.0, top: 0.0, right: width as f32, bottom: height as f32 };
    layout_node(root, bounds, 0)
}

fn draw_layout_node(canvas: &mut Canvas<'_>, node: &LaidOutView) {
    if let Some(color) = node.background_argb {
        let mut paint = Paint::new();
        paint.color = color;
        paint.style = PaintStyle::Fill;
        canvas.draw_rect(RectF::new(node.bounds.left, node.bounds.top, node.bounds.right, node.bounds.bottom), &paint);
    }
    if let Some(text)=node.text.as_deref(){let mut paint=Paint::new();paint.color=node.text_argb;paint.text_size=node.text_size;
        let baseline=(node.bounds.top+node.bounds.bottom)*0.5+node.text_size*0.35;
        let width=text.chars().count()as f32*node.text_size*6.0/7.0;
        canvas.draw_text(text,node.bounds.left+(node.bounds.width()-width).max(0.0)*0.5,baseline,&paint);
    }
    for child in &node.children {
        draw_layout_node(canvas, child);
    }
}

/// Rasterizes resolved solid View backgrounds into the guest Surface.
/// This first pass does not draw glyphs, images, gradients, or custom View.onDraw.
pub fn render_layout_backgrounds(surface: &mut Surface, tree: &LaidOutView) {
    let mut canvas = Canvas::new(surface);
    draw_layout_node(&mut canvas, tree);
}

#[cfg(test)]
mod layout_engine_tests {
    use super::*;

    fn view(kind: InflatedViewKind, attrs: &[(&str, &str)], children: Vec<InflatedView>) -> InflatedView {
        InflatedView {
            kind, source_tag: "test".into(),
            attributes: attrs.iter().map(|(k,v)| ((*k).into(), (*v).into())).collect(),
            layout_width: None, layout_height: None, children,
        }
    }

    #[test]
    fn linear_layout_assigns_vertical_child_bounds_and_renders_background() {
        let child = view(InflatedViewKind::View, &[("background", "#ff0000")], vec![]);
        let root = view(InflatedViewKind::LinearLayout, &[("orientation", "vertical")], vec![child.clone(), child]);
        let tree = measure_and_layout(&root, 20, 40);
        assert_eq!(tree.children[0].bounds, LayoutRect { left: 0.0, top: 0.0, right: 20.0, bottom: 20.0 });
        assert_eq!(tree.children[1].bounds, LayoutRect { left: 0.0, top: 20.0, right: 20.0, bottom: 40.0 });
        let mut surface = Surface::new(20, 40);
        render_layout_backgrounds(&mut surface, &tree);
        assert_eq!(surface.get_pixel(5, 5), Some(0xffff0000));
        assert_eq!(surface.get_pixel(5, 25), Some(0xffff0000));
    }

    #[test]
    fn horizontal_linear_layout_splits_width() {
        let child = view(InflatedViewKind::View, &[], vec![]);
        let root = view(InflatedViewKind::LinearLayout, &[("orientation", "horizontal")], vec![child.clone(), child]);
        let tree = measure_and_layout(&root, 40, 20);
        assert_eq!(tree.children[1].bounds.left, 20.0);
        assert_eq!(tree.children[1].bounds.right, 40.0);
    }
}
