//! AccessKit tree serialization with stable node references.

use accesskit::{Node, NodeId, Role, TreeUpdate};
use std::collections::HashMap;

/// Serialized AccessKit tree with node references.
#[derive(Debug, Clone)]
pub struct SerializedTree {
    nodes: HashMap<NodeId, NodeInfo>,
    root_id: Option<NodeId>,
}

/// Information about a single node.
#[derive(Debug, Clone)]
pub struct NodeInfo {
    pub id: NodeId,
    pub role: Role,
    pub name: Option<String>,
    pub value: Option<String>,
    pub children: Vec<NodeId>,
    pub checked: Option<bool>,
    pub selected: Option<bool>,
    pub expanded: Option<bool>,
    pub disabled: bool,
}

impl SerializedTree {
    /// Create an empty tree.
    pub fn new() -> Self {
        Self {
            nodes: HashMap::new(),
            root_id: None,
        }
    }

    /// Update from an AccessKit TreeUpdate.
    pub fn update(&mut self, update: &TreeUpdate) {
        // Update tree root if provided
        if let Some(ref tree) = update.tree {
            self.root_id = Some(tree.root);
        }

        // Update nodes
        for (id, node) in &update.nodes {
            let info = NodeInfo::from_accesskit(*id, node);
            self.nodes.insert(*id, info);
        }
    }

    /// Get a node by ID.
    pub fn get(&self, id: NodeId) -> Option<&NodeInfo> {
        self.nodes.get(&id)
    }

    /// Get all node IDs.
    pub fn node_ids(&self) -> impl Iterator<Item = NodeId> + '_ {
        self.nodes.keys().copied()
    }

    /// Get the root node ID.
    pub fn root_id(&self) -> Option<NodeId> {
        self.root_id
    }

    /// Get node count.
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// Format the tree as a text representation with refs.
    pub fn format_tree(&self) -> String {
        let Some(root_id) = self.root_id else {
            return "(empty tree)".to_string();
        };

        let mut output = String::new();
        self.format_node(&mut output, root_id, 0);
        output
    }

    fn format_node(&self, output: &mut String, id: NodeId, depth: usize) {
        let Some(node) = self.nodes.get(&id) else {
            return;
        };

        let indent = "  ".repeat(depth);
        let role_str = format_role(node.role);
        let ref_str = format!("[ref=n{}]", id.0);

        // Build the line
        output.push_str(&indent);
        output.push_str("- ");
        output.push_str(&role_str);

        // Add name if present
        if let Some(ref name) = node.name {
            if !name.is_empty() {
                output.push_str(&format!(" \"{}\"", name));
            }
        }

        output.push(' ');
        output.push_str(&ref_str);

        // Add value if present
        if let Some(ref value) = node.value {
            if !value.is_empty() {
                output.push_str(&format!(": \"{}\"", value));
            }
        }

        // Add state indicators
        if let Some(checked) = node.checked {
            output.push_str(if checked { " [checked]" } else { " [unchecked]" });
        }
        if let Some(selected) = node.selected {
            if selected {
                output.push_str(" [selected]");
            }
        }
        if let Some(expanded) = node.expanded {
            output.push_str(if expanded {
                " [expanded]"
            } else {
                " [collapsed]"
            });
        }
        if node.disabled {
            output.push_str(" [disabled]");
        }

        output.push('\n');

        // Recurse to children
        for child_id in &node.children {
            self.format_node(output, *child_id, depth + 1);
        }
    }

    /// Clear the tree.
    pub fn clear(&mut self) {
        self.nodes.clear();
        self.root_id = None;
    }
}

impl Default for SerializedTree {
    fn default() -> Self {
        Self::new()
    }
}

impl NodeInfo {
    fn from_accesskit(id: NodeId, node: &Node) -> Self {
        // Convert Toggled enum to bool
        let checked = node.toggled().map(|t| match t {
            accesskit::Toggled::True => true,
            accesskit::Toggled::False => false,
            accesskit::Toggled::Mixed => true, // Treat mixed as checked
        });

        Self {
            id,
            role: node.role(),
            name: node.label().map(|s| s.to_string()),
            value: node.value().map(|s| s.to_string()),
            children: node.children().to_vec(),
            checked,
            selected: node.is_selected(),
            expanded: node.is_expanded(),
            disabled: node.is_disabled(),
        }
    }
}

/// Format AccessKit role as a readable string.
fn format_role(role: Role) -> String {
    match role {
        Role::Window => "window",
        Role::Button => "button",
        Role::CheckBox => "checkbox",
        Role::RadioButton => "radio_button",
        Role::Slider => "slider",
        Role::SpinButton => "spin_button",
        Role::TextInput => "text_input",
        Role::MultilineTextInput => "multiline_text_input",
        Role::Label => "label",
        Role::Link => "link",
        Role::Image => "image",
        Role::List => "list",
        Role::ListItem => "list_item",
        Role::Tree => "tree",
        Role::TreeItem => "tree_item",
        Role::Tab => "tab",
        Role::TabList => "tab_list",
        Role::TabPanel => "tab_panel",
        Role::Menu => "menu",
        Role::MenuItem => "menu_item",
        Role::MenuBar => "menu_bar",
        Role::ProgressIndicator => "progress",
        Role::ScrollBar => "scrollbar",
        Role::Group => "group",
        Role::GenericContainer => "container",
        Role::Paragraph => "paragraph",
        Role::Heading => "heading",
        Role::Table => "table",
        Role::Row => "row",
        Role::Cell => "cell",
        Role::ColumnHeader => "column_header",
        Role::RowHeader => "row_header",
        Role::ComboBox => "combobox",
        Role::Dialog => "dialog",
        Role::AlertDialog => "alert_dialog",
        Role::Tooltip => "tooltip",
        Role::Unknown => "unknown",
        _ => "element",
    }
    .to_string()
}
