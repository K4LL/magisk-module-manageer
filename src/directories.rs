use std::collections::BTreeMap;
use std::process::Command;

#[derive(Debug)]
pub struct Node {
    children: BTreeMap<String, Node>,
}

impl Node {
    pub fn new() -> Self {
        Self {
            children: BTreeMap::new(),
        }
    }

    pub fn insert(&mut self, path: &str) {
        let mut current = self;

        for part in path.split('/').filter(|p| !p.is_empty()) {
            current = current.children.entry(part.to_string())
                .or_insert_with(Node::new);
        }
    }

    pub fn print(&self, indent: usize) {
        for (name, node) in &self.children {
            println!("{}{}", " ".repeat(indent), name);
            node.print(indent + 2);
        }
    }

    pub fn get(&self, path: &str) -> Option<&Node> {
        let mut current = self;

        for part in path.split('/').filter(|p| !p.is_empty()) {
            current = current.children.get(part)?;
        }

        Some(current)
    }
}

pub fn get_android_tree() -> Node {
    let output = Command::new("adb")
        .args(["shell", "su", "-c", "find / -xdev"])
        .output()
        .expect("Failed to run adb");

    let stdout = String::from_utf8_lossy(&output.stdout);
    
    let mut root = Node::new();

    for line in stdout.lines() {
        root.insert(line);
    }

    return root
}

pub fn list_directories() -> Node {
  let output = Command::new("adb")
        .args(["shell", "su", "-c", "find / -type d -xdev"])
        .output()
        .expect("Failed to run adb");

    let stdout = String::from_utf8_lossy(&output.stdout);
    
    let mut root = Node::new();

    for line in stdout.lines() {
        root.insert(line);
    }

    return root
}