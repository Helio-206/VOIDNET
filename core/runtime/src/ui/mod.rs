use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    time::Instant,
};
use thiserror::Error;
use void_transport::event::TransportEvent;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SurfaceDocument {
    pub title: Option<String>,
    pub children: Vec<SurfaceNode>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SurfaceNode {
    pub kind: SurfaceNodeKind,
    pub properties: BTreeMap<String, String>,
    pub children: Vec<SurfaceNode>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SurfaceNodeKind {
    Page,
    Column,
    Row,
    Text,
    Input,
    Button,
    Spacer,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeUiTree {
    pub title: Option<String>,
    pub root: RuntimeUiNode,
    pub node_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeUiNode {
    pub node_id: String,
    pub kind: SurfaceNodeKind,
    pub properties: BTreeMap<String, String>,
    pub children: Vec<RuntimeUiNode>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderedAction {
    pub index: usize,
    pub node_id: String,
    pub label: String,
    pub action: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TerminalRenderedSurface {
    pub output: String,
    pub actions: Vec<RenderedAction>,
    pub render_duration_ms: u128,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeActionRequest {
    pub action: String,
    pub input_state: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeActionResult {
    pub summary: String,
    pub events: Vec<TransportEvent>,
}

pub fn parse_surface_document(source: &str) -> Result<SurfaceDocument, RuntimeUiError> {
    let tokens = tokenize(source)?;
    Parser::new(tokens).parse_document()
}

pub fn build_runtime_tree(document: &SurfaceDocument) -> Result<RuntimeUiTree, RuntimeUiError> {
    let mut next_id = 0usize;
    let root = build_runtime_node(
        &SurfaceNode {
            kind: SurfaceNodeKind::Page,
            properties: BTreeMap::new(),
            children: document.children.clone(),
        },
        &mut next_id,
    )?;
    Ok(RuntimeUiTree {
        title: document.title.clone(),
        root,
        node_count: next_id,
    })
}

pub fn render_terminal_surface(
    tree: &RuntimeUiTree,
    bindings: &BTreeMap<String, String>,
    input_state: &BTreeMap<String, String>,
) -> Result<TerminalRenderedSurface, RuntimeUiError> {
    let started = Instant::now();
    let mut lines = Vec::new();
    let mut actions = Vec::new();

    if let Some(title) = &tree.title {
        lines.push(format!("== {title} =="));
    }

    render_node(&tree.root, 0, bindings, input_state, &mut lines, &mut actions)?;
    lines.push(String::new());
    lines.push("Commands: set <input_id> <value> | press <index> | refresh | quit".to_string());

    Ok(TerminalRenderedSurface {
        output: lines.join("\n"),
        actions,
        render_duration_ms: started.elapsed().as_millis(),
    })
}

pub fn count_bound_nodes(tree: &RuntimeUiTree, binding_keys: &[String]) -> usize {
    count_bound_nodes_in_subtree(&tree.root, binding_keys)
}

fn count_bound_nodes_in_subtree(node: &RuntimeUiNode, binding_keys: &[String]) -> usize {
    let self_count = node
        .properties
        .get("bind")
        .map(|binding| usize::from(binding_keys.iter().any(|key| key == binding)))
        .unwrap_or_default();
    self_count
        + node
            .children
            .iter()
            .map(|child| count_bound_nodes_in_subtree(child, binding_keys))
            .sum::<usize>()
}

fn build_runtime_node(
    source: &SurfaceNode,
    next_id: &mut usize,
) -> Result<RuntimeUiNode, RuntimeUiError> {
    *next_id += 1;
    let node_id = format!("node-{}", *next_id);
    let children = source
        .children
        .iter()
        .map(|child| build_runtime_node(child, next_id))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(RuntimeUiNode {
        node_id,
        kind: source.kind,
        properties: source.properties.clone(),
        children,
    })
}

fn render_node(
    node: &RuntimeUiNode,
    depth: usize,
    bindings: &BTreeMap<String, String>,
    input_state: &BTreeMap<String, String>,
    lines: &mut Vec<String>,
    actions: &mut Vec<RenderedAction>,
) -> Result<(), RuntimeUiError> {
    let indent = "  ".repeat(depth);
    match node.kind {
        SurfaceNodeKind::Page | SurfaceNodeKind::Column => {
            for child in &node.children {
                render_node(child, depth, bindings, input_state, lines, actions)?;
            }
        }
        SurfaceNodeKind::Row => {
            let mut parts = Vec::new();
            for child in &node.children {
                match child.kind {
                    SurfaceNodeKind::Button => {
                        let label = required_property(&child.properties, "label")?.to_string();
                        let action = required_property(&child.properties, "action")?.to_string();
                        let index = actions.len() + 1;
                        actions.push(RenderedAction {
                            index,
                            node_id: child.node_id.clone(),
                            label: label.clone(),
                            action,
                        });
                        parts.push(format!("[{index}] {label}"));
                    }
                    SurfaceNodeKind::Text => {
                        let value = if let Some(bind) = child.properties.get("bind") {
                            bindings
                                .get(bind)
                                .cloned()
                                .unwrap_or_else(|| format!("<{bind}:unbound>"))
                        } else {
                            child
                                .properties
                                .get("value")
                                .cloned()
                                .unwrap_or_default()
                        };
                        parts.push(value.lines().next().unwrap_or_default().to_string());
                    }
                    SurfaceNodeKind::Input => {
                        let id = required_property(&child.properties, "id")?;
                        let placeholder = child
                            .properties
                            .get("placeholder")
                            .cloned()
                            .unwrap_or_default();
                        let current = input_state.get(id).cloned().unwrap_or(placeholder);
                        parts.push(format!("{id}: {current}"));
                    }
                    _ => {
                        let mut child_lines = Vec::new();
                        render_node(
                            child,
                            0,
                            bindings,
                            input_state,
                            &mut child_lines,
                            actions,
                        )?;
                        if let Some(line) = child_lines.into_iter().find(|line| !line.trim().is_empty()) {
                            parts.push(line.trim().to_string());
                        }
                    }
                }
            }
            if !parts.is_empty() {
                lines.push(format!("{indent}{}", parts.join(" | ")));
            }
        }
        SurfaceNodeKind::Text => {
            let value = if let Some(bind) = node.properties.get("bind") {
                bindings.get(bind).cloned().unwrap_or_else(|| format!("<{bind}:unbound>"))
            } else {
                node.properties
                    .get("value")
                    .cloned()
                    .unwrap_or_default()
            };
            for line in value.lines() {
                lines.push(format!("{indent}{line}"));
            }
            if value.is_empty() {
                lines.push(format!("{indent}"));
            }
        }
        SurfaceNodeKind::Input => {
            let id = required_property(&node.properties, "id")?;
            let placeholder = node.properties.get("placeholder").cloned().unwrap_or_default();
            let current = input_state.get(id).cloned().unwrap_or_else(|| placeholder.clone());
            lines.push(format!("{indent}{id}: {current}"));
        }
        SurfaceNodeKind::Button => {
            let label = required_property(&node.properties, "label")?.to_string();
            let action = required_property(&node.properties, "action")?.to_string();
            let index = actions.len() + 1;
            actions.push(RenderedAction {
                index,
                node_id: node.node_id.clone(),
                label: label.clone(),
                action: action.clone(),
            });
            lines.push(format!("{indent}[{index}] {label}"));
        }
        SurfaceNodeKind::Spacer => {
            let size = node
                .properties
                .get("size")
                .and_then(|value| value.parse::<usize>().ok())
                .unwrap_or(1);
            for _ in 0..size {
                lines.push(String::new());
            }
        }
    }
    Ok(())
}

fn required_property<'a>(
    properties: &'a BTreeMap<String, String>,
    key: &'static str,
) -> Result<&'a str, RuntimeUiError> {
    properties
        .get(key)
        .map(String::as_str)
        .ok_or(RuntimeUiError::MissingProperty(key))
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Token {
    Ident(String),
    String(String),
    Equals,
    LBrace,
    RBrace,
}

fn tokenize(source: &str) -> Result<Vec<Token>, RuntimeUiError> {
    let mut tokens = Vec::new();
    let mut chars = source.char_indices().peekable();

    while let Some((offset, ch)) = chars.next() {
        match ch {
            '{' => tokens.push(Token::LBrace),
            '}' => tokens.push(Token::RBrace),
            '=' => tokens.push(Token::Equals),
            '"' => tokens.push(Token::String(read_string(&mut chars, offset)?)),
            ch if ch.is_whitespace() => {}
            ch if is_ident_start(ch) => {
                let mut ident = String::from(ch);
                while let Some((_, next)) = chars.peek().copied() {
                    if is_ident_continue(next) {
                        ident.push(next);
                        chars.next();
                    } else {
                        break;
                    }
                }
                tokens.push(Token::Ident(ident));
            }
            _ => {
                return Err(RuntimeUiError::UnexpectedChar { ch, offset });
            }
        }
    }

    Ok(tokens)
}

fn read_string<I>(
    chars: &mut std::iter::Peekable<I>,
    start: usize,
) -> Result<String, RuntimeUiError>
where
    I: Iterator<Item = (usize, char)>,
{
    let mut value = String::new();
    while let Some((_, ch)) = chars.next() {
        match ch {
            '"' => return Ok(value),
            '\\' => {
                let escaped = chars
                    .next()
                    .ok_or(RuntimeUiError::UnterminatedString { start })?
                    .1;
                match escaped {
                    '"' => value.push('"'),
                    '\\' => value.push('\\'),
                    'n' => value.push('\n'),
                    't' => value.push('\t'),
                    other => return Err(RuntimeUiError::InvalidEscape { ch: other, start }),
                }
            }
            other => value.push(other),
        }
    }
    Err(RuntimeUiError::UnterminatedString { start })
}

fn is_ident_start(ch: char) -> bool {
    ch.is_ascii_alphabetic() || ch == '_'
}

fn is_ident_continue(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '.')
}

struct Parser {
    tokens: Vec<Token>,
    position: usize,
}

impl Parser {
    fn new(tokens: Vec<Token>) -> Self {
        Self { tokens, position: 0 }
    }

    fn parse_document(mut self) -> Result<SurfaceDocument, RuntimeUiError> {
        self.expect_ident("page")?;
        self.expect(Token::LBrace)?;
        let mut title = None;
        let mut children = Vec::new();

        while !self.peek_is(&Token::RBrace) {
            let ident = self.next_ident()?;
            if ident == "title" {
                title = Some(self.next_string()?);
            } else {
                children.push(self.parse_node_after_ident(ident)?);
            }
        }

        self.expect(Token::RBrace)?;
        self.expect_eof()?;
        Ok(SurfaceDocument { title, children })
    }

    fn parse_node_after_ident(&mut self, ident: String) -> Result<SurfaceNode, RuntimeUiError> {
        match ident.as_str() {
            "column" => self.parse_container(SurfaceNodeKind::Column),
            "row" => self.parse_container(SurfaceNodeKind::Row),
            "text" => self.parse_text(),
            "input" => self.parse_inline(SurfaceNodeKind::Input),
            "button" => self.parse_button(),
            "spacer" => self.parse_inline(SurfaceNodeKind::Spacer),
            other => Err(RuntimeUiError::UnknownNode(other.to_string())),
        }
    }

    fn parse_container(&mut self, kind: SurfaceNodeKind) -> Result<SurfaceNode, RuntimeUiError> {
        self.expect(Token::LBrace)?;
        let mut children = Vec::new();
        while !self.peek_is(&Token::RBrace) {
            let ident = self.next_ident()?;
            children.push(self.parse_node_after_ident(ident)?);
        }
        self.expect(Token::RBrace)?;
        Ok(SurfaceNode {
            kind,
            properties: BTreeMap::new(),
            children,
        })
    }

    fn parse_text(&mut self) -> Result<SurfaceNode, RuntimeUiError> {
        let mut properties = BTreeMap::new();
        if self.peek_string() {
            properties.insert("value".to_string(), self.next_string()?);
        }
        properties.extend(self.parse_attributes()?);
        if !properties.contains_key("value") && !properties.contains_key("bind") {
            return Err(RuntimeUiError::MissingProperty("value|bind"));
        }
        Ok(SurfaceNode {
            kind: SurfaceNodeKind::Text,
            properties,
            children: Vec::new(),
        })
    }

    fn parse_inline(&mut self, kind: SurfaceNodeKind) -> Result<SurfaceNode, RuntimeUiError> {
        let properties = self.parse_attributes()?;
        Ok(SurfaceNode {
            kind,
            properties,
            children: Vec::new(),
        })
    }

    fn parse_button(&mut self) -> Result<SurfaceNode, RuntimeUiError> {
        self.expect(Token::LBrace)?;
        let mut properties = BTreeMap::new();
        while !self.peek_is(&Token::RBrace) {
            let key = self.next_ident()?;
            match key.as_str() {
                "label" | "action" => {
                    properties.insert(key, self.next_string()?);
                }
                other => return Err(RuntimeUiError::UnknownProperty(other.to_string())),
            }
        }
        self.expect(Token::RBrace)?;
        if !properties.contains_key("label") {
            return Err(RuntimeUiError::MissingProperty("label"));
        }
        if !properties.contains_key("action") {
            return Err(RuntimeUiError::MissingProperty("action"));
        }
        Ok(SurfaceNode {
            kind: SurfaceNodeKind::Button,
            properties,
            children: Vec::new(),
        })
    }

    fn parse_attributes(&mut self) -> Result<BTreeMap<String, String>, RuntimeUiError> {
        let mut properties = BTreeMap::new();
        while self.peek_attribute() {
            let key = self.next_ident()?;
            self.expect(Token::Equals)?;
            let value = if self.peek_string() {
                self.next_string()?
            } else {
                self.next_ident()?
            };
            properties.insert(key, value);
        }
        Ok(properties)
    }

    fn next(&mut self) -> Result<Token, RuntimeUiError> {
        let token = self
            .tokens
            .get(self.position)
            .cloned()
            .ok_or(RuntimeUiError::UnexpectedEof)?;
        self.position += 1;
        Ok(token)
    }

    fn next_ident(&mut self) -> Result<String, RuntimeUiError> {
        match self.next()? {
            Token::Ident(value) => Ok(value),
            token => Err(RuntimeUiError::UnexpectedToken(format!("{token:?}"))),
        }
    }

    fn next_string(&mut self) -> Result<String, RuntimeUiError> {
        match self.next()? {
            Token::String(value) => Ok(value),
            token => Err(RuntimeUiError::UnexpectedToken(format!("{token:?}"))),
        }
    }

    fn expect(&mut self, expected: Token) -> Result<(), RuntimeUiError> {
        let token = self.next()?;
        if token == expected {
            Ok(())
        } else {
            Err(RuntimeUiError::UnexpectedToken(format!("{token:?}")))
        }
    }

    fn expect_ident(&mut self, expected: &'static str) -> Result<(), RuntimeUiError> {
        let actual = self.next_ident()?;
        if actual == expected {
            Ok(())
        } else {
            Err(RuntimeUiError::ExpectedIdent { expected, actual })
        }
    }

    fn expect_eof(&self) -> Result<(), RuntimeUiError> {
        if self.position == self.tokens.len() {
            Ok(())
        } else {
            Err(RuntimeUiError::UnexpectedToken(format!("{:?}", self.tokens[self.position])))
        }
    }

    fn peek_is(&self, expected: &Token) -> bool {
        self.tokens.get(self.position) == Some(expected)
    }

    fn peek_string(&self) -> bool {
        matches!(self.tokens.get(self.position), Some(Token::String(_)))
    }

    fn peek_attribute(&self) -> bool {
        matches!(
            (self.tokens.get(self.position), self.tokens.get(self.position + 1)),
            (Some(Token::Ident(_)), Some(Token::Equals))
        )
    }
}

#[derive(Debug, Error)]
pub enum RuntimeUiError {
    #[error("unexpected character {ch:?} at byte offset {offset}")]
    UnexpectedChar { ch: char, offset: usize },
    #[error("unterminated string starting at byte offset {start}")]
    UnterminatedString { start: usize },
    #[error("invalid escape {ch:?} in string starting at byte offset {start}")]
    InvalidEscape { ch: char, start: usize },
    #[error("unexpected end of VOID UI document")]
    UnexpectedEof,
    #[error("unexpected token: {0}")]
    UnexpectedToken(String),
    #[error("expected identifier {expected}, got {actual}")]
    ExpectedIdent { expected: &'static str, actual: String },
    #[error("unknown VOID UI node {0}")]
    UnknownNode(String),
    #[error("unknown VOID UI property {0}")]
    UnknownProperty(String),
    #[error("missing required property {0}")]
    MissingProperty(&'static str),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_surface_document() {
        let source = r#"
page {
  title "VOIDChat"
  column {
    text "Secure mesh communication"
    input id="message" placeholder="Message"
    button {
      label "Send"
      action "chat.send"
    }
  }
}
"#;
        let document = parse_surface_document(source).unwrap();
        let tree = build_runtime_tree(&document).unwrap();

        assert_eq!(document.title.as_deref(), Some("VOIDChat"));
        assert_eq!(tree.node_count, 5);
    }

    #[test]
    fn rejects_malformed_document() {
        let source = r#"page { column { text } }"#;
        assert!(parse_surface_document(source).is_err());
    }

    #[test]
    fn renders_terminal_surface_with_bindings() {
        let source = r#"
page {
  title "VOIDChat"
  column {
    text bind="chat.status"
    input id="message" placeholder="Message"
        row {
            button {
                label "Send"
                action "chat.send"
            }
            button {
                label "Refresh"
                action "surface.refresh"
            }
    }
  }
}
"#;
        let document = parse_surface_document(source).unwrap();
        let tree = build_runtime_tree(&document).unwrap();
        let rendered = render_terminal_surface(
            &tree,
            &BTreeMap::from([("chat.status".to_string(), "ready".to_string())]),
            &BTreeMap::new(),
        )
        .unwrap();

        assert!(rendered.output.contains("ready"));
        assert!(rendered.output.contains("[1] Send"));
        assert!(rendered.output.contains("[2] Refresh"));
    }
}