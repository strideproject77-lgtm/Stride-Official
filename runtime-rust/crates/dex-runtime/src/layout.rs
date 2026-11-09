//! Safe structural inflation of decoded Android XML into a runtime-neutral view tree.
//! Resource IDs and theme-dependent values are intentionally retained as unresolved
//! attributes; this layer never fabricates resolved Android resources.

use std::collections::BTreeMap;
use crate::XmlNode;

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
