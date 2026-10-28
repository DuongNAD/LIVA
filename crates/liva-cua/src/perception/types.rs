//! Semantic UI element tree data models, roles, states, and perception results.

use crate::types::{CuaPoint, CuaRect};
use crate::vision::types::RawFrame;
use serde::{Deserialize, Serialize};

/// Semantic roles for UI controls and layout elements.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UiRole {
    Button,
    Edit,
    Text,
    CheckBox,
    RadioButton,
    ComboBox,
    ListBox,
    ListItem,
    MenuItem,
    TabItem,
    ScrollBar,
    Slider,
    Hyperlink,
    Image,
    ProgressBar,
    Window,
    Dialog,
    Pane,
    Group,
    Custom,
}

impl UiRole {
    /// Returns true if elements of this role are typically interactive by default.
    #[inline]
    pub const fn is_interactive_by_default(&self) -> bool {
        matches!(
            self,
            Self::Button
                | Self::Edit
                | Self::CheckBox
                | Self::RadioButton
                | Self::ComboBox
                | Self::ListBox
                | Self::ListItem
                | Self::MenuItem
                | Self::TabItem
                | Self::ScrollBar
                | Self::Slider
                | Self::Hyperlink
        )
    }

    /// Returns true if this role represents a container/layout grouping.
    #[inline]
    pub const fn is_container(&self) -> bool {
        matches!(
            self,
            Self::Window | Self::Dialog | Self::Pane | Self::Group | Self::Custom
        )
    }
}

/// Dynamic behavioral state flags for a UI element.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct UiState {
    pub is_enabled: bool,
    pub is_visible: bool,
    pub is_focused: bool,
    pub is_selected: bool,
    pub is_clickable: bool,
    pub is_checkable: bool,
    pub is_checked: bool,
    pub is_scrollable: bool,
}

impl Default for UiState {
    fn default() -> Self {
        Self {
            is_enabled: true,
            is_visible: true,
            is_focused: false,
            is_selected: false,
            is_clickable: false,
            is_checkable: false,
            is_checked: false,
            is_scrollable: false,
        }
    }
}

impl UiState {
    /// Creates a state for an interactive clickable control (e.g., button, menu item).
    pub fn clickable() -> Self {
        Self {
            is_enabled: true,
            is_visible: true,
            is_clickable: true,
            ..Default::default()
        }
    }

    /// Creates a state for an editable text field.
    pub fn editable() -> Self {
        Self {
            is_enabled: true,
            is_visible: true,
            is_clickable: true,
            ..Default::default()
        }
    }

    /// Creates a state for a checkable control (e.g., checkbox, radio button).
    pub fn checkable(checked: bool) -> Self {
        Self {
            is_enabled: true,
            is_visible: true,
            is_clickable: true,
            is_checkable: true,
            is_checked: checked,
            ..Default::default()
        }
    }

    /// Creates a state for static non-interactive text/icon display.
    pub fn static_display() -> Self {
        Self {
            is_enabled: true,
            is_visible: true,
            is_clickable: false,
            ..Default::default()
        }
    }

    /// Creates a state for a hidden or off-screen element.
    pub fn hidden() -> Self {
        Self {
            is_enabled: true,
            is_visible: false,
            ..Default::default()
        }
    }

    /// Creates a state for a disabled control.
    pub fn disabled() -> Self {
        Self {
            is_enabled: false,
            is_visible: true,
            ..Default::default()
        }
    }
}

/// A node in the semantic UI element hierarchy.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UiElement {
    /// Stable or synthetic element identifier.
    pub id: String,
    /// Semantic role classification.
    pub role: UiRole,
    /// Accessible name, label, or caption.
    pub name: String,
    /// Optional textual value or text-box content.
    pub value: Option<String>,
    /// Bounding rectangle in virtual screen coordinates.
    pub bounds: CuaRect,
    /// Element state flags.
    pub state: UiState,
    /// Hierarchical child elements.
    pub children: Vec<UiElement>,
    /// Set-of-Mark (SoM) 1-based index assigned by perception indexer.
    pub som_index: Option<usize>,
}

impl UiElement {
    /// Creates a new UI element node.
    pub fn new(
        id: impl Into<String>,
        role: UiRole,
        name: impl Into<String>,
        bounds: CuaRect,
    ) -> Self {
        let is_role_interactive = role.is_interactive_by_default();
        Self {
            id: id.into(),
            role,
            name: name.into(),
            value: None,
            bounds,
            state: UiState {
                is_clickable: is_role_interactive,
                ..Default::default()
            },
            children: Vec::new(),
            som_index: None,
        }
    }

    /// Builder: sets explicit state flags.
    pub fn with_state(mut self, state: UiState) -> Self {
        self.state = state;
        self
    }

    /// Builder: sets text value.
    pub fn with_value(mut self, value: impl Into<String>) -> Self {
        self.value = Some(value.into());
        self
    }

    /// Builder: sets child elements.
    pub fn with_children(mut self, children: Vec<UiElement>) -> Self {
        self.children = children;
        self
    }

    /// Builder: appends a single child element.
    pub fn with_child(mut self, child: UiElement) -> Self {
        self.children.push(child);
        self
    }

    /// Returns the center coordinate of this element's bounding rectangle.
    #[inline]
    pub fn center(&self) -> (i32, i32) {
        self.bounds.center()
    }

    /// Returns the center as a CuaPoint.
    #[inline]
    pub fn center_point(&self) -> CuaPoint {
        let (cx, cy) = self.bounds.center();
        CuaPoint::new(cx, cy)
    }

    /// Determines whether this element qualifies as interactive for Set-of-Mark indexing.
    pub fn is_interactive(&self) -> bool {
        if !self.state.is_visible || !self.state.is_enabled {
            return false;
        }
        if self.bounds.is_empty() || self.bounds.is_minimized_sentinel() {
            return false;
        }
        // Offscreen coordinates sentinel
        if self.bounds.x <= -10000 || self.bounds.y <= -10000 {
            return false;
        }
        // Containers are only interactive if explicitly marked clickable/checkable
        if self.role.is_container() {
            return self.state.is_clickable || self.state.is_checkable;
        }
        // Plain static text or images are non-interactive unless explicitly clickable
        if matches!(
            self.role,
            UiRole::Text | UiRole::Image | UiRole::ProgressBar
        ) {
            return self.state.is_clickable || self.state.is_checkable;
        }

        self.state.is_clickable
            || self.state.is_checkable
            || self.state.is_scrollable
            || self.role.is_interactive_by_default()
    }

    /// Recursively flattens the subtree into a list of references in preorder traversal.
    pub fn flatten(&self) -> Vec<&UiElement> {
        let mut list = Vec::new();
        Self::collect_elements(self, &mut list);
        list
    }

    fn collect_elements<'a>(node: &'a UiElement, list: &mut Vec<&'a UiElement>) {
        list.push(node);
        for child in &node.children {
            Self::collect_elements(child, list);
        }
    }

    /// Recursively executes a mutating closure over every element in preorder traversal.
    pub fn for_each_mut<F: FnMut(&mut UiElement)>(&mut self, f: &mut F) {
        f(self);
        for child in &mut self.children {
            child.for_each_mut(f);
        }
    }

    /// Recursively clears all Set-of-Mark indices across the subtree.
    pub fn clear_som_indices(&mut self) {
        self.for_each_mut(&mut |node| {
            node.som_index = None;
        });
    }

    /// Recursively locates an element by its assigned Set-of-Mark numeric index.
    pub fn find_by_som(&self, target_index: usize) -> Option<&UiElement> {
        if self.som_index == Some(target_index) {
            return Some(self);
        }
        for child in &self.children {
            if let Some(found) = child.find_by_som(target_index) {
                return Some(found);
            }
        }
        None
    }

    /// Recursively locates a mutable element by its assigned Set-of-Mark numeric index.
    pub fn find_by_som_mut(&mut self, target_index: usize) -> Option<&mut UiElement> {
        if self.som_index == Some(target_index) {
            return Some(self);
        }
        for child in &mut self.children {
            if let Some(found) = child.find_by_som_mut(target_index) {
                return Some(found);
            }
        }
        None
    }

    /// Recursively locates an element by its identifier.
    pub fn find_by_id(&self, target_id: &str) -> Option<&UiElement> {
        if self.id == target_id {
            return Some(self);
        }
        for child in &self.children {
            if let Some(found) = child.find_by_id(target_id) {
                return Some(found);
            }
        }
        None
    }

    /// Recursively locates a mutable element by its identifier.
    pub fn find_by_id_mut(&mut self, target_id: &str) -> Option<&mut UiElement> {
        if self.id == target_id {
            return Some(self);
        }
        for child in &mut self.children {
            if let Some(found) = child.find_by_id_mut(target_id) {
                return Some(found);
            }
        }
        None
    }
}

/// Hierarchical UI tree rooted at a specific desktop window.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UiTree {
    /// Win32 HWND of the root target window.
    pub root_hwnd: u64,
    /// Root element node (typically Role::Window or Role::Dialog).
    pub root: UiElement,
    /// Total number of nodes in the hierarchy.
    pub total_nodes: usize,
    /// Number of interactive elements assigned Set-of-Mark indices.
    pub interactive_count: usize,
    /// Timestamp when this tree was extracted (UNIX epoch milliseconds).
    pub timestamp_ms: u64,
}

impl UiTree {
    /// Creates a new `UiTree` with automatic node counts.
    pub fn new(root_hwnd: u64, root: UiElement) -> Self {
        let all = root.flatten();
        let total = all.len();
        let interactive = all.iter().filter(|e| e.som_index.is_some()).count();
        Self {
            root_hwnd,
            root,
            total_nodes: total,
            interactive_count: interactive,
            timestamp_ms: crate::audit::current_unix_ms(),
        }
    }

    /// Creates an empty UI tree for an unknown, closed, or degenerate window.
    pub fn empty(root_hwnd: u64) -> Self {
        let root = UiElement::new(
            "root_empty",
            UiRole::Window,
            "Empty Window",
            CuaRect::default(),
        );
        Self {
            root_hwnd,
            root,
            total_nodes: 1,
            interactive_count: 0,
            timestamp_ms: crate::audit::current_unix_ms(),
        }
    }

    /// Recounts `total_nodes` and `interactive_count` from the current tree structure.
    pub fn recount(&mut self) {
        let all = self.root.flatten();
        self.total_nodes = all.len();
        self.interactive_count = all.iter().filter(|e| e.som_index.is_some()).count();
    }

    /// Recursively executes a mutating closure over every element in preorder traversal.
    #[inline]
    pub fn for_each_mut<F: FnMut(&mut UiElement)>(&mut self, mut f: F) {
        self.root.for_each_mut(&mut f);
    }

    /// Recursively clears all Set-of-Mark indices across the subtree.
    #[inline]
    pub fn clear_som_indices(&mut self) {
        self.root.clear_som_indices();
        self.interactive_count = 0;
    }

    /// Locates an element by its assigned Set-of-Mark numeric index.
    #[inline]
    pub fn find_by_som(&self, index: usize) -> Option<&UiElement> {
        self.root.find_by_som(index)
    }

    /// Locates a mutable element by its assigned Set-of-Mark numeric index.
    #[inline]
    pub fn find_by_som_mut(&mut self, index: usize) -> Option<&mut UiElement> {
        self.root.find_by_som_mut(index)
    }

    /// Locates an element by its identifier.
    #[inline]
    pub fn find_by_id(&self, id: &str) -> Option<&UiElement> {
        self.root.find_by_id(id)
    }

    /// Locates a mutable element by its identifier.
    #[inline]
    pub fn find_by_id_mut(&mut self, id: &str) -> Option<&mut UiElement> {
        self.root.find_by_id_mut(id)
    }

    /// Returns a flat list of references to all elements in the tree.
    #[inline]
    pub fn all_elements(&self) -> Vec<&UiElement> {
        self.root.flatten()
    }

    /// Returns a flat list of references to all Set-of-Mark indexed elements, sorted by index.
    pub fn interactive_elements(&self) -> Vec<&UiElement> {
        let mut list: Vec<&UiElement> = self
            .root
            .flatten()
            .into_iter()
            .filter(|e| e.som_index.is_some())
            .collect();
        list.sort_by_key(|e| e.som_index.unwrap_or(usize::MAX));
        list
    }

    /// Generates a compact markdown prompt string for LLM tool context.
    /// Sanitizes newlines and carriage returns from element names and values to guarantee single-line entries.
    /// Example output:
    /// `[1] Button "OK" bounds=[x=200, y=300, w=80, h=30]`
    pub fn format_som_prompt(&self) -> String {
        let interactive = self.interactive_elements();
        if interactive.is_empty() {
            return "No interactive UI elements detected.".to_string();
        }

        let mut lines = Vec::with_capacity(interactive.len());
        for elem in interactive {
            let idx = elem.som_index.unwrap_or(0);
            let role_str = format!("{:?}", elem.role);
            let clean_name = elem.name.replace(['\r', '\n'], " ").trim().to_string();
            let val_str = if let Some(ref val) = elem.value {
                let clean_val = val.replace(['\r', '\n'], " ").trim().to_string();
                format!(" value=\"{}\"", clean_val)
            } else {
                String::new()
            };
            lines.push(format!(
                "[{}] {} \"{}\"{} bounds=[x={}, y={}, w={}, h={}]",
                idx,
                role_str,
                clean_name,
                val_str,
                elem.bounds.x,
                elem.bounds.y,
                elem.bounds.width,
                elem.bounds.height
            ));
        }
        lines.join("\n")
    }
}

/// Operational perception mode indicating semantic or visual fallback path.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PerceptionMode {
    Semantic,
    VisualFallback,
}

/// Unified perception output providing structured semantic tree or localized ROI fallback.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum PerceptionResult {
    /// Target exposed a structured semantic hierarchy with indexed Set-of-Mark controls.
    Semantic { tree: UiTree, som_count: usize },
    /// Target exposed an empty or insufficient semantic tree (e.g. canvas/game); fell back to visual ROI.
    VisualFallback {
        window_bounds: CuaRect,
        frame: Option<RawFrame>,
        reason: String,
    },
}

impl PerceptionResult {
    /// Returns the active perception mode.
    #[inline]
    pub fn mode(&self) -> PerceptionMode {
        match self {
            Self::Semantic { .. } => PerceptionMode::Semantic,
            Self::VisualFallback { .. } => PerceptionMode::VisualFallback,
        }
    }

    /// True if result contains a semantic UI tree.
    #[inline]
    pub fn is_semantic(&self) -> bool {
        matches!(self, Self::Semantic { .. })
    }

    /// True if result fell back to visual ROI capture.
    #[inline]
    pub fn is_visual_fallback(&self) -> bool {
        matches!(self, Self::VisualFallback { .. })
    }

    /// Returns a reference to the semantic UI tree if available.
    #[inline]
    pub fn tree(&self) -> Option<&UiTree> {
        match self {
            Self::Semantic { tree, .. } => Some(tree),
            Self::VisualFallback { .. } => None,
        }
    }

    /// Returns the count of Set-of-Mark indexed elements (0 for fallback).
    #[inline]
    pub fn som_count(&self) -> usize {
        match self {
            Self::Semantic { som_count, .. } => *som_count,
            Self::VisualFallback { .. } => 0,
        }
    }
}
