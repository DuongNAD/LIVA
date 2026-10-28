//! Zero-dependency XML parser for Android `uiautomator dump` hierarchies.
//!
//! Converts raw Android UI hierarchy XML into structured `AdbUiTree` and LIVA's
//! native `UiTree` / `UiElement` perception models with Set-of-Mark (SoM) indexing
//! and coordinate bounds center math.

use crate::perception::{SoMIndexer, UiElement, UiRole, UiState, UiTree};
use crate::types::{CuaError, CuaRect};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Parses an Android `uiautomator` bounds string in the format `"[x1,y1][x2,y2]"`
/// into a `CuaRect` (x, y, width, height).
///
/// # Examples
/// ```
/// use liva_cua::adb::parser::parse_adb_bounds;
/// let rect = parse_adb_bounds("[0,0][1080,2400]").unwrap();
/// assert_eq!(rect.x, 0);
/// assert_eq!(rect.y, 0);
/// assert_eq!(rect.width, 1080);
/// assert_eq!(rect.height, 2400);
/// assert_eq!(rect.center(), (540, 1200));
/// ```
pub fn parse_adb_bounds(s: &str) -> Option<CuaRect> {
    let s = s.trim();
    if !s.starts_with('[') || !s.ends_with(']') {
        return None;
    }

    let mid = s.find(']')?;
    let p1 = &s[1..mid];
    let rest = s[mid + 1..].trim();
    if !rest.starts_with('[') || !rest.ends_with(']') {
        return None;
    }
    let p2 = &rest[1..rest.len() - 1];

    let c1: Vec<&str> = p1.split(',').collect();
    let c2: Vec<&str> = p2.split(',').collect();

    if c1.len() != 2 || c2.len() != 2 {
        return None;
    }

    let x1: i32 = c1[0].trim().parse().ok()?;
    let y1: i32 = c1[1].trim().parse().ok()?;
    let x2: i32 = c2[0].trim().parse().ok()?;
    let y2: i32 = c2[1].trim().parse().ok()?;

    if x2 < x1 || y2 < y1 {
        return None;
    }

    let width = x2 - x1;
    let height = y2 - y1;

    Some(CuaRect::new(x1, y1, width, height))
}

/// Unescapes common standard XML entities in attribute values.
pub fn unescape_xml(s: &str) -> String {
    if !s.contains('&') {
        return s.to_string();
    }

    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '&' {
            let mut entity = String::with_capacity(8);
            let mut closed = false;
            for _ in 0..10 {
                if let Some(&ech) = chars.peek() {
                    chars.next();
                    if ech == ';' {
                        closed = true;
                        break;
                    }
                    entity.push(ech);
                } else {
                    break;
                }
            }

            if closed {
                match entity.as_str() {
                    "quot" => out.push('"'),
                    "amp" => out.push('&'),
                    "apos" => out.push('\''),
                    "lt" => out.push('<'),
                    "gt" => out.push('>'),
                    _ if entity.starts_with("#x") || entity.starts_with("#X") => {
                        if let Ok(code) = u32::from_str_radix(&entity[2..], 16) {
                            if let Some(c) = char::from_u32(code) {
                                out.push(c);
                                continue;
                            }
                        }
                        out.push('&');
                        out.push_str(&entity);
                        out.push(';');
                    }
                    _ if entity.starts_with('#') => {
                        if let Ok(code) = entity[1..].parse::<u32>() {
                            if let Some(c) = char::from_u32(code) {
                                out.push(c);
                                continue;
                            }
                        }
                        out.push('&');
                        out.push_str(&entity);
                        out.push(';');
                    }
                    _ => {
                        out.push('&');
                        out.push_str(&entity);
                        out.push(';');
                    }
                }
            } else {
                out.push('&');
                out.push_str(&entity);
            }
        } else {
            out.push(ch);
        }
    }

    out
}

/// Raw parsed Android UI node representing an element in the `uiautomator` hierarchy.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdbUiNode {
    pub index: usize,
    pub som_index: Option<usize>,
    pub text: String,
    pub resource_id: String,
    pub class_name: String,
    pub package_name: String,
    pub content_desc: String,
    pub checkable: bool,
    pub checked: bool,
    pub clickable: bool,
    pub enabled: bool,
    pub focusable: bool,
    pub focused: bool,
    pub scrollable: bool,
    pub long_clickable: bool,
    pub password: bool,
    pub selected: bool,
    pub bounds: CuaRect,
    pub center: (i32, i32),
    pub children: Vec<AdbUiNode>,
}

impl AdbUiNode {
    /// Recursively flattens this node and its children into a vector in preorder traversal.
    pub fn flatten(&self) -> Vec<&AdbUiNode> {
        let mut list = Vec::new();
        Self::collect_elements(self, &mut list);
        list
    }

    fn collect_elements<'a>(node: &'a AdbUiNode, list: &mut Vec<&'a AdbUiNode>) {
        list.push(node);
        for child in &node.children {
            Self::collect_elements(child, list);
        }
    }

    /// Determines whether this Android node qualifies as an interactive element for Set-of-Mark.
    pub fn is_interactive(&self) -> bool {
        if !self.enabled || self.bounds.width <= 0 || self.bounds.height <= 0 {
            return false;
        }

        // Offscreen coordinates sentinel
        if self.bounds.x <= -10000 || self.bounds.y <= -10000 {
            return false;
        }

        if self.clickable || self.long_clickable || self.checkable || self.scrollable {
            return true;
        }

        let lower_class = self.class_name.to_lowercase();
        if lower_class.contains("button")
            || lower_class.contains("edittext")
            || lower_class.contains("checkbox")
            || lower_class.contains("switch")
            || lower_class.contains("radiobutton")
            || lower_class.contains("spinner")
            || lower_class.contains("seekbar")
        {
            return true;
        }

        // If it has non-empty text or content description and is focusable
        if self.focusable && (!self.text.is_empty() || !self.content_desc.is_empty()) {
            return true;
        }

        false
    }

    /// Converts this `AdbUiNode` to a LIVA native `UiElement`.
    pub fn to_ui_element(&self) -> UiElement {
        let role = classify_android_role(
            &self.class_name,
            self.clickable || self.long_clickable,
            self.checkable,
            self.scrollable,
        );

        let name = if !self.text.is_empty() {
            self.text.clone()
        } else if !self.content_desc.is_empty() {
            self.content_desc.clone()
        } else if !self.resource_id.is_empty() {
            self.resource_id.clone()
        } else {
            self.class_name.clone()
        };

        let value = if !self.text.is_empty() && !self.content_desc.is_empty() {
            Some(self.content_desc.clone())
        } else {
            None
        };

        let id = if !self.resource_id.is_empty() {
            self.resource_id.clone()
        } else {
            format!("node_{}_{}_{}", self.index, self.bounds.x, self.bounds.y)
        };

        let state = UiState {
            is_enabled: self.enabled,
            is_visible: self.bounds.width > 0 && self.bounds.height > 0,
            is_focused: self.focused,
            is_selected: self.selected,
            is_clickable: self.clickable || self.long_clickable,
            is_checkable: self.checkable,
            is_checked: self.checked,
            is_scrollable: self.scrollable,
        };

        let children: Vec<UiElement> = self.children.iter().map(|c| c.to_ui_element()).collect();

        UiElement {
            id,
            role,
            name,
            value,
            bounds: self.bounds,
            state,
            children,
            som_index: self.som_index,
        }
    }
}

/// Classifies an Android widget class string into LIVA's semantic `UiRole`.
pub fn classify_android_role(
    class_name: &str,
    clickable: bool,
    checkable: bool,
    scrollable: bool,
) -> UiRole {
    let lower = class_name.to_lowercase();
    if lower.contains("checkbox") || lower.contains("switch") || lower.contains("togglebutton") {
        UiRole::CheckBox
    } else if lower.contains("radiobutton") {
        UiRole::RadioButton
    } else if lower.contains("button") || lower.contains("imagebutton") {
        UiRole::Button
    } else if lower.contains("edittext") || lower.contains("autocompletetextview") {
        UiRole::Edit
    } else if lower.contains("spinner") {
        UiRole::ComboBox
    } else if lower.contains("listview")
        || lower.contains("recyclerview")
        || lower.contains("gridview")
    {
        UiRole::ListBox
    } else if lower.contains("progressbar") || lower.contains("seekbar") {
        UiRole::ProgressBar
    } else if lower.contains("imageview") {
        UiRole::Image
    } else if lower.contains("textview") {
        if clickable {
            UiRole::Button
        } else {
            UiRole::Text
        }
    } else if lower.contains("scrollview") || scrollable {
        UiRole::Pane
    } else if lower.contains("framelayout")
        || lower.contains("linearlayout")
        || lower.contains("relativelayout")
        || lower.contains("constraintlayout")
    {
        UiRole::Group
    } else if clickable {
        UiRole::Button
    } else if checkable {
        UiRole::CheckBox
    } else {
        UiRole::Custom
    }
}

/// Set-of-Mark indexed Android interactive element summary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdbSomElement {
    pub som_index: usize,
    pub bounds: CuaRect,
    pub center: (i32, i32),
    pub role: String,
    pub label: String,
    pub resource_id: String,
    pub package_name: String,
    pub clickable: bool,
    pub scrollable: bool,
    pub enabled: bool,
}

/// Top-level Android UI hierarchy tree.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdbUiTree {
    pub root: AdbUiNode,
    pub som_elements: Vec<AdbSomElement>,
    pub screen_width: i32,
    pub screen_height: i32,
}

impl AdbUiTree {
    /// Converts this `AdbUiTree` to a LIVA native `UiTree` rooted at a synthetic window HWND,
    /// running the deterministic Set-of-Mark indexer across all nodes.
    pub fn to_ui_tree(&self, root_hwnd: u64) -> UiTree {
        let mut tree = UiTree::new(root_hwnd, self.root.to_ui_element());
        SoMIndexer::index_tree(&mut tree);
        tree
    }

    /// Locates an interactive element by its Set-of-Mark 1-based index.
    pub fn find_by_som(&self, som_index: usize) -> Option<&AdbSomElement> {
        self.som_elements.iter().find(|e| e.som_index == som_index)
    }
}

// ============================================================================
// Zero-Dependency XML Streaming Parser
// ============================================================================

enum XmlToken {
    Open(String, HashMap<String, String>),
    Close(String),
    SelfClosing(String, HashMap<String, String>),
}

/// Tokenizes Android uiautomator XML without external dependencies.
struct XmlTokenizer<'a> {
    chars: std::iter::Peekable<std::str::Chars<'a>>,
}

impl<'a> XmlTokenizer<'a> {
    fn new(xml: &'a str) -> Self {
        Self {
            chars: xml.chars().peekable(),
        }
    }

    fn skip_whitespace(&mut self) {
        while let Some(&ch) = self.chars.peek() {
            if ch.is_whitespace() {
                self.chars.next();
            } else {
                break;
            }
        }
    }

    fn next_token(&mut self) -> Option<XmlToken> {
        loop {
            self.skip_whitespace();
            let ch = self.chars.next()?;

            if ch != '<' {
                // Skip plain text outside tags
                continue;
            }

            // Check if comment <!-- ... -->
            if self.chars.peek() == Some(&'!') {
                self.chars.next();
                // Consume until '>'
                let mut dash_count = 0;
                for c in self.chars.by_ref() {
                    if c == '-' {
                        dash_count += 1;
                    } else if c == '>' && dash_count >= 2 {
                        break;
                    } else {
                        dash_count = 0;
                    }
                }
                continue;
            }

            // Check if processing instruction <? ... ?>
            if self.chars.peek() == Some(&'?') {
                self.chars.next();
                let mut prev = ' ';
                for c in self.chars.by_ref() {
                    if prev == '?' && c == '>' {
                        break;
                    }
                    prev = c;
                }
                continue;
            }

            // Check if closing tag </tag>
            if self.chars.peek() == Some(&'/') {
                self.chars.next();
                let mut tag_name = String::new();
                for c in self.chars.by_ref() {
                    if c == '>' {
                        break;
                    }
                    if !c.is_whitespace() {
                        tag_name.push(c);
                    }
                }
                return Some(XmlToken::Close(tag_name));
            }

            // Opening or self-closing tag
            let mut tag_name = String::new();
            while let Some(&c) = self.chars.peek() {
                if c.is_whitespace() || c == '>' || c == '/' {
                    break;
                }
                tag_name.push(self.chars.next().unwrap());
            }

            // Parse attributes key="value" or key='value'
            let mut attrs = HashMap::new();
            let mut is_self_closing = false;

            loop {
                self.skip_whitespace();
                match self.chars.peek() {
                    Some(&'>') => {
                        self.chars.next();
                        break;
                    }
                    Some(&'/') => {
                        self.chars.next();
                        if self.chars.peek() == Some(&'>') {
                            self.chars.next();
                            is_self_closing = true;
                            break;
                        }
                    }
                    None => break,
                    _ => {
                        // Read attribute key
                        let mut key = String::new();
                        while let Some(&c) = self.chars.peek() {
                            if c == '=' || c.is_whitespace() || c == '>' || c == '/' {
                                break;
                            }
                            key.push(self.chars.next().unwrap());
                        }

                        self.skip_whitespace();
                        if self.chars.peek() == Some(&'=') {
                            self.chars.next();
                            self.skip_whitespace();

                            if let Some(&quote) = self.chars.peek() {
                                if quote == '"' || quote == '\'' {
                                    self.chars.next();
                                    let mut val = String::new();
                                    for c in self.chars.by_ref() {
                                        if c == quote {
                                            break;
                                        }
                                        val.push(c);
                                    }
                                    let unescaped = unescape_xml(&val);
                                    attrs.insert(key, unescaped);
                                }
                            }
                        }
                    }
                }
            }

            if is_self_closing {
                return Some(XmlToken::SelfClosing(tag_name, attrs));
            } else {
                return Some(XmlToken::Open(tag_name, attrs));
            }
        }
    }
}

fn parse_node_from_attrs(attrs: &HashMap<String, String>, fallback_index: usize) -> AdbUiNode {
    let index: usize = attrs
        .get("index")
        .and_then(|s| s.parse().ok())
        .unwrap_or(fallback_index);

    let text = attrs.get("text").cloned().unwrap_or_default();
    let resource_id = attrs.get("resource-id").cloned().unwrap_or_default();
    let class_name = attrs.get("class").cloned().unwrap_or_default();
    let package_name = attrs.get("package").cloned().unwrap_or_default();
    let content_desc = attrs.get("content-desc").cloned().unwrap_or_default();

    let checkable = attrs.get("checkable").map(|s| s == "true").unwrap_or(false);
    let checked = attrs.get("checked").map(|s| s == "true").unwrap_or(false);
    let clickable = attrs.get("clickable").map(|s| s == "true").unwrap_or(false);
    let enabled = attrs.get("enabled").map(|s| s == "true").unwrap_or(true);
    let focusable = attrs.get("focusable").map(|s| s == "true").unwrap_or(false);
    let focused = attrs.get("focused").map(|s| s == "true").unwrap_or(false);
    let scrollable = attrs
        .get("scrollable")
        .map(|s| s == "true")
        .unwrap_or(false);
    let long_clickable = attrs
        .get("long-clickable")
        .map(|s| s == "true")
        .unwrap_or(false);
    let password = attrs.get("password").map(|s| s == "true").unwrap_or(false);
    let selected = attrs.get("selected").map(|s| s == "true").unwrap_or(false);

    let bounds = attrs
        .get("bounds")
        .and_then(|s| parse_adb_bounds(s))
        .unwrap_or_default();

    let center = bounds.center();

    AdbUiNode {
        index,
        som_index: None,
        text,
        resource_id,
        class_name,
        package_name,
        content_desc,
        checkable,
        checked,
        clickable,
        enabled,
        focusable,
        focused,
        scrollable,
        long_clickable,
        password,
        selected,
        bounds,
        center,
        children: Vec::new(),
    }
}

/// Parses raw Android `uiautomator dump` XML into an `AdbUiTree`.
pub fn parse_uiautomator_xml(xml: &str) -> Result<AdbUiTree, CuaError> {
    let mut tokenizer = XmlTokenizer::new(xml);

    struct StackEntry {
        node: AdbUiNode,
        children: Vec<AdbUiNode>,
    }

    let mut stack: Vec<StackEntry> = Vec::new();
    let mut root_node: Option<AdbUiNode> = None;
    let mut node_counter = 0;
    let mut screen_width = 1080;
    let mut screen_height = 2400;

    while let Some(token) = tokenizer.next_token() {
        match token {
            XmlToken::Open(tag, attrs) => {
                if tag == "hierarchy" {
                    // Try to infer rotation or dimensions if present
                    continue;
                }
                if tag == "node" {
                    node_counter += 1;
                    let node = parse_node_from_attrs(&attrs, node_counter);
                    stack.push(StackEntry {
                        node,
                        children: Vec::new(),
                    });
                }
            }
            XmlToken::SelfClosing(tag, attrs) => {
                if tag == "node" {
                    node_counter += 1;
                    let node = parse_node_from_attrs(&attrs, node_counter);
                    if let Some(parent) = stack.last_mut() {
                        parent.children.push(node);
                    } else if root_node.is_none() {
                        root_node = Some(node);
                    }
                }
            }
            XmlToken::Close(tag) => {
                if tag == "node" {
                    if let Some(mut entry) = stack.pop() {
                        entry.node.children = entry.children;
                        if let Some(parent) = stack.last_mut() {
                            parent.children.push(entry.node);
                        } else {
                            root_node = Some(entry.node);
                        }
                    }
                }
            }
        }
    }

    // If stack has lingering items, resolve bottom-up
    while let Some(mut entry) = stack.pop() {
        entry.node.children = entry.children;
        if let Some(parent) = stack.last_mut() {
            parent.children.push(entry.node);
        } else {
            root_node = Some(entry.node);
        }
    }

    let mut root = match root_node {
        Some(r) => r,
        None => {
            return Err(CuaError::InvalidParameter(
                "Provided XML contains no valid <node> hierarchy elements".into(),
            ));
        }
    };

    if root.bounds.width > 0 && root.bounds.height > 0 {
        screen_width = root.bounds.width;
        screen_height = root.bounds.height;
    }

    // Step 2: Extract interactive elements and assign Set-of-Mark (SoM) indices [1..N]
    // Traverse in reading order: top-to-bottom, left-to-right with row banding
    let mut candidates: Vec<(usize, i32, i32, AdbSomElement)> = Vec::new();
    let mut preorder_idx = 0;

    fn collect_som_candidates(
        node: &AdbUiNode,
        counter: &mut usize,
        candidates: &mut Vec<(usize, i32, i32, AdbSomElement)>,
    ) {
        let cur = *counter;
        *counter += 1;

        if node.is_interactive() {
            let role = classify_android_role(
                &node.class_name,
                node.clickable || node.long_clickable,
                node.checkable,
                node.scrollable,
            );
            let label = if !node.text.is_empty() {
                node.text.clone()
            } else if !node.content_desc.is_empty() {
                node.content_desc.clone()
            } else {
                node.resource_id.clone()
            };

            candidates.push((
                cur,
                node.bounds.x,
                node.bounds.y,
                AdbSomElement {
                    som_index: 0, // will assign below
                    bounds: node.bounds,
                    center: node.center,
                    role: format!("{:?}", role),
                    label,
                    resource_id: node.resource_id.clone(),
                    package_name: node.package_name.clone(),
                    clickable: node.clickable || node.long_clickable,
                    scrollable: node.scrollable,
                    enabled: node.enabled,
                },
            ));
        }

        for child in &node.children {
            collect_som_candidates(child, counter, candidates);
        }
    }

    collect_som_candidates(&root, &mut preorder_idx, &mut candidates);

    // Sort by y, then x, then preorder
    candidates.sort_by(|a, b| {
        a.2.cmp(&b.2)
            .then_with(|| a.1.cmp(&b.1))
            .then_with(|| a.0.cmp(&b.0))
    });

    // Row banding with 8px tolerance
    let mut bands: Vec<Vec<(usize, i32, i32, AdbSomElement)>> = Vec::new();
    for item in candidates {
        if let Some(band) = bands.last_mut() {
            let anchor_y = band[0].2;
            if (item.2 as i64 - anchor_y as i64).abs() <= 8 {
                band.push(item);
                continue;
            }
        }
        bands.push(vec![item]);
    }

    let mut ordered: Vec<(usize, AdbSomElement)> = Vec::new();
    for mut band in bands {
        band.sort_by(|a, b| {
            a.1.cmp(&b.1)
                .then_with(|| a.2.cmp(&b.2))
                .then_with(|| a.0.cmp(&b.0))
        });
        for item in band {
            ordered.push((item.0, item.3));
        }
    }

    let mut mark_map: HashMap<usize, usize> = HashMap::with_capacity(ordered.len());
    let mut som_elements: Vec<AdbSomElement> = Vec::with_capacity(ordered.len());

    for (idx, (p_idx, mut som_elem)) in ordered.into_iter().enumerate() {
        let mark = idx + 1;
        som_elem.som_index = mark;
        mark_map.insert(p_idx, mark);
        som_elements.push(som_elem);
    }

    // Apply mark to root node hierarchy
    let mut assign_counter = 0;
    fn assign_marks(node: &mut AdbUiNode, counter: &mut usize, map: &HashMap<usize, usize>) {
        let cur = *counter;
        *counter += 1;
        if let Some(&m) = map.get(&cur) {
            node.som_index = Some(m);
        }
        for child in &mut node.children {
            assign_marks(child, counter, map);
        }
    }
    assign_marks(&mut root, &mut assign_counter, &mark_map);

    Ok(AdbUiTree {
        root,
        som_elements,
        screen_width,
        screen_height,
    })
}

/// High-level convenience function parsing an Android `uiautomator dump` XML directly into
/// a LIVA native `UiTree` rooted at `root_hwnd`.
pub fn parse_uiautomator_to_ui_tree(xml: &str, root_hwnd: u64) -> Result<UiTree, CuaError> {
    let adb_tree = parse_uiautomator_xml(xml)?;
    Ok(adb_tree.to_ui_tree(root_hwnd))
}
