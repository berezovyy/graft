use std::collections::HashMap;

use owo_colors::OwoColorize;

use crate::workspace::Workspace;

pub struct WorkspaceTree {
    pub name: String,
    pub origin: String,
    pub file_count: usize,
    pub children: Vec<WorkspaceTree>,
    pub is_current: bool,
}

/// Build a hierarchy of workspace trees from a flat list of workspaces.
pub fn build_hierarchy(workspaces: &[&Workspace]) -> Vec<WorkspaceTree> {
    let current_ws = std::env::var("GRAFT_WORKSPACE").ok();

    let ws_map: HashMap<&str, &&Workspace> = workspaces
        .iter()
        .map(|ws| (ws.name.as_str(), ws))
        .collect();

    let mut children_map: HashMap<Option<&str>, Vec<&Workspace>> = HashMap::new();
    for ws in workspaces {
        let parent_key = ws.parent.as_deref();
        // Only treat as child if the parent actually exists in our workspace list
        let effective_parent = match parent_key {
            Some(p) if ws_map.contains_key(p) => Some(p),
            _ => None,
        };
        children_map
            .entry(effective_parent)
            .or_default()
            .push(ws);
    }

    fn build_node(
        ws: &Workspace,
        children_map: &HashMap<Option<&str>, Vec<&Workspace>>,
        current_ws: &Option<String>,
    ) -> WorkspaceTree {
        let mut children: Vec<WorkspaceTree> = children_map
            .get(&Some(ws.name.as_str()))
            .unwrap_or(&Vec::new())
            .iter()
            .map(|child| build_node(child, children_map, current_ws))
            .collect();
        children.sort_by(|a, b| a.name.cmp(&b.name));

        let origin = if let Some(ref parent_name) = ws.parent {
            parent_name.clone()
        } else {
            ws.base.display().to_string()
        };

        WorkspaceTree {
            name: ws.name.clone(),
            origin,
            file_count: ws.count_upper_files(),
            children,
            is_current: current_ws.as_deref() == Some(&ws.name),
        }
    }

    let mut roots: Vec<WorkspaceTree> = children_map
        .get(&None)
        .unwrap_or(&Vec::new())
        .iter()
        .map(|ws| build_node(ws, &children_map, &current_ws))
        .collect();
    roots.sort_by(|a, b| a.name.cmp(&b.name));
    roots
}

/// Format a hierarchy of workspace trees into a string with box-drawing characters.
pub fn format_hierarchy(forest: &[WorkspaceTree]) -> String {
    if forest.is_empty() {
        return "no workspaces".to_string();
    }

    let mut output = String::new();

    for (i, root) in forest.iter().enumerate() {
        if i > 0 {
            output.push('\n');
        }
        let current_marker = if root.is_current {
            format!(" {}", "*".green().bold())
        } else {
            String::new()
        };
        let file_info = format!(
            "({} file{})",
            root.file_count,
            if root.file_count == 1 { "" } else { "s" },
        );
        output.push_str(&format!(
            "{}/{} {}\n",
            root.name.bold(),
            current_marker,
            file_info.dimmed(),
        ));
        format_children(&root.children, "", &mut output);
    }

    if output.ends_with('\n') {
        output.pop();
    }
    output
}

fn format_children(children: &[WorkspaceTree], prefix: &str, output: &mut String) {
    for (i, child) in children.iter().enumerate() {
        let is_last = i == children.len() - 1;
        let connector = if is_last { "└── " } else { "├── " };
        let current_marker = if child.is_current {
            format!(" {}", "*".green().bold())
        } else {
            String::new()
        };
        let file_info = format!(
            "({} file{})",
            child.file_count,
            if child.file_count == 1 { "" } else { "s" },
        );

        output.push_str(&format!(
            "{}{}{}{} {}\n",
            prefix,
            connector,
            child.name.bold(),
            current_marker,
            file_info.dimmed(),
        ));

        let child_prefix = if is_last {
            format!("{}    ", prefix)
        } else {
            format!("{}│   ", prefix)
        };
        format_children(&child.children, &child_prefix, output);
    }
}
