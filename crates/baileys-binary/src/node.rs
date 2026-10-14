use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Represents the content of a WhatsApp Binary XML node.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum BinaryNodeContent {
    Nodes(Vec<BinaryNode>),
    Bytes(Vec<u8>),
    String(String),
}

/// Represents a WhatsApp Binary XML stanza node: `<tag attr="val">content</tag>`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BinaryNode {
    pub tag: String,
    #[serde(default)]
    pub attrs: HashMap<String, String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<BinaryNodeContent>,
}

impl BinaryNode {
    pub fn new(tag: impl Into<String>) -> Self {
        Self {
            tag: tag.into(),
            attrs: HashMap::new(),
            content: None,
        }
    }

    pub fn with_attr(mut self, key: impl Into<String>, val: impl Into<String>) -> Self {
        self.attrs.insert(key.into(), val.into());
        self
    }

    pub fn with_child(mut self, child: BinaryNode) -> Self {
        match &mut self.content {
            Some(BinaryNodeContent::Nodes(children)) => {
                children.push(child);
            }
            _ => {
                self.content = Some(BinaryNodeContent::Nodes(vec![child]));
            }
        }
        self
    }

    pub fn with_content(mut self, content: impl Into<BinaryNodeContent>) -> Self {
        self.content = Some(content.into());
        self
    }

    pub fn with_string_content(mut self, s: impl Into<String>) -> Self {
        self.content = Some(BinaryNodeContent::String(s.into()));
        self
    }

    pub fn with_bytes_content(mut self, b: impl Into<Vec<u8>>) -> Self {
        self.content = Some(BinaryNodeContent::Bytes(b.into()));
        self
    }

    pub fn with_children(mut self, children: Vec<BinaryNode>) -> Self {
        self.content = Some(BinaryNodeContent::Nodes(children));
        self
    }

    pub fn get_attr(&self, key: &str) -> Option<&str> {
        self.attrs.get(key).map(|s| s.as_str())
    }

    pub fn get_child(&self, tag: &str) -> Option<&BinaryNode> {
        self.get_child_by_tag(tag)
    }

    pub fn get_child_by_tag(&self, tag: &str) -> Option<&BinaryNode> {
        match &self.content {
            Some(BinaryNodeContent::Nodes(children)) => {
                children.iter().find(|child| child.tag == tag)
            }
            _ => None,
        }
    }

    pub fn get_children(&self) -> &[BinaryNode] {
        match &self.content {
            Some(BinaryNodeContent::Nodes(children)) => children.as_slice(),
            _ => &[],
        }
    }

    pub fn get_children_by_tag<'a>(&'a self, tag: &'a str) -> impl Iterator<Item = &'a BinaryNode> {
        self.get_children()
            .iter()
            .filter(move |child| child.tag == tag)
    }

    pub fn content_as_bytes(&self) -> Option<&[u8]> {
        match &self.content {
            Some(BinaryNodeContent::Bytes(b)) => Some(b.as_slice()),
            Some(BinaryNodeContent::String(s)) => Some(s.as_bytes()),
            _ => None,
        }
    }

    pub fn as_bytes(&self) -> Option<&[u8]> {
        self.content_as_bytes()
    }

    pub fn content_as_str(&self) -> Option<&str> {
        match &self.content {
            Some(BinaryNodeContent::String(s)) => Some(s.as_str()),
            Some(BinaryNodeContent::Bytes(b)) => std::str::from_utf8(b).ok(),
            _ => None,
        }
    }

    pub fn content_as_string(&self) -> Option<&str> {
        self.content_as_str()
    }
}

impl From<Vec<u8>> for BinaryNodeContent {
    fn from(b: Vec<u8>) -> Self {
        Self::Bytes(b)
    }
}

impl From<&[u8]> for BinaryNodeContent {
    fn from(b: &[u8]) -> Self {
        Self::Bytes(b.to_vec())
    }
}

impl From<String> for BinaryNodeContent {
    fn from(s: String) -> Self {
        Self::String(s)
    }
}

impl From<&str> for BinaryNodeContent {
    fn from(s: &str) -> Self {
        Self::String(s.to_string())
    }
}

impl From<Vec<BinaryNode>> for BinaryNodeContent {
    fn from(nodes: Vec<BinaryNode>) -> Self {
        Self::Nodes(nodes)
    }
}
