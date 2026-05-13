use crate::VoidUri;
use serde::{Deserialize, Serialize};
use std::str::FromStr;
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VoidUiPage {
    pub title: Option<String>,
    pub children: Vec<VoidUiNode>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum VoidUiNode {
    Column(Vec<VoidUiNode>),
    Row(Vec<VoidUiNode>),
    Panel(Vec<VoidUiNode>),
    List(Vec<VoidUiNode>),
    Item(Vec<VoidUiNode>),
    Text(String),
    Button { label: String, action: VoidUri },
    Input {
        id: String,
        placeholder: Option<String>,
        secure: bool,
    },
}

pub fn parse_void_ui(source: &str) -> Result<VoidUiPage, VoidUiParseError> {
    let tokens = tokenize(source)?;
    Parser::new(tokens).parse_page()
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Token {
    Ident(String),
    String(String),
    LBrace,
    RBrace,
}

fn tokenize(source: &str) -> Result<Vec<Token>, VoidUiParseError> {
    let mut tokens = Vec::new();
    let mut chars = source.char_indices().peekable();

    while let Some((offset, ch)) = chars.next() {
        match ch {
            '{' => tokens.push(Token::LBrace),
            '}' => tokens.push(Token::RBrace),
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
                return Err(VoidUiParseError::UnexpectedChar { ch, offset });
            }
        }
    }

    Ok(tokens)
}

fn read_string<I>(
    chars: &mut std::iter::Peekable<I>,
    start: usize,
) -> Result<String, VoidUiParseError>
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
                    .ok_or(VoidUiParseError::UnterminatedString { start })?
                    .1;
                match escaped {
                    '"' => value.push('"'),
                    '\\' => value.push('\\'),
                    'n' => value.push('\n'),
                    't' => value.push('\t'),
                    other => {
                        return Err(VoidUiParseError::InvalidEscape { ch: other, start });
                    }
                }
            }
            other => value.push(other),
        }
    }

    Err(VoidUiParseError::UnterminatedString { start })
}

fn is_ident_start(ch: char) -> bool {
    ch.is_ascii_alphabetic() || ch == '_'
}

fn is_ident_continue(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-')
}

struct Parser {
    tokens: Vec<Token>,
    position: usize,
}

impl Parser {
    fn new(tokens: Vec<Token>) -> Self {
        Self {
            tokens,
            position: 0,
        }
    }

    fn parse_page(mut self) -> Result<VoidUiPage, VoidUiParseError> {
        self.expect_ident("page")?;
        self.expect_lbrace()?;

        let mut title = None;
        let mut children = Vec::new();

        while !self.peek_rbrace() {
            let ident = self.next_ident()?;
            if ident == "title" {
                title = Some(self.next_string()?);
            } else {
                children.push(self.parse_node_after_ident(ident)?);
            }
        }

        self.expect_rbrace()?;
        self.expect_eof()?;

        Ok(VoidUiPage { title, children })
    }

    fn parse_nodes_until_rbrace(&mut self) -> Result<Vec<VoidUiNode>, VoidUiParseError> {
        let mut nodes = Vec::new();
        while !self.peek_rbrace() {
            let ident = self.next_ident()?;
            nodes.push(self.parse_node_after_ident(ident)?);
        }
        self.expect_rbrace()?;
        Ok(nodes)
    }

    fn parse_node_after_ident(&mut self, ident: String) -> Result<VoidUiNode, VoidUiParseError> {
        match ident.as_str() {
            "column" => {
                self.expect_lbrace()?;
                Ok(VoidUiNode::Column(self.parse_nodes_until_rbrace()?))
            }
            "row" => {
                self.expect_lbrace()?;
                Ok(VoidUiNode::Row(self.parse_nodes_until_rbrace()?))
            }
            "panel" => {
                self.expect_lbrace()?;
                Ok(VoidUiNode::Panel(self.parse_nodes_until_rbrace()?))
            }
            "list" => {
                self.expect_lbrace()?;
                Ok(VoidUiNode::List(self.parse_nodes_until_rbrace()?))
            }
            "item" => {
                self.expect_lbrace()?;
                Ok(VoidUiNode::Item(self.parse_nodes_until_rbrace()?))
            }
            "text" => Ok(VoidUiNode::Text(self.next_string()?)),
            "button" => self.parse_button(),
            "input" => self.parse_input(),
            other => Err(VoidUiParseError::UnknownNode(other.to_string())),
        }
    }

    fn parse_button(&mut self) -> Result<VoidUiNode, VoidUiParseError> {
        self.expect_lbrace()?;
        let mut label = None;
        let mut action = None;

        while !self.peek_rbrace() {
            match self.next_ident()?.as_str() {
                "label" => label = Some(self.next_string()?),
                "action" => action = Some(self.next_string()?),
                other => return Err(VoidUiParseError::UnknownProperty(other.to_string())),
            }
        }

        self.expect_rbrace()?;
        let label = label.ok_or(VoidUiParseError::MissingProperty("label"))?;
        let action = action.ok_or(VoidUiParseError::MissingProperty("action"))?;
        let action = VoidUri::from_str(&action)?;

        Ok(VoidUiNode::Button { label, action })
    }

    fn parse_input(&mut self) -> Result<VoidUiNode, VoidUiParseError> {
        self.expect_lbrace()?;
        let mut id = None;
        let mut placeholder = None;
        let mut secure = false;

        while !self.peek_rbrace() {
            match self.next_ident()?.as_str() {
                "id" => id = Some(self.next_string()?),
                "placeholder" => placeholder = Some(self.next_string()?),
                "secure" => secure = self.next_bool()?,
                other => return Err(VoidUiParseError::UnknownProperty(other.to_string())),
            }
        }

        self.expect_rbrace()?;
        Ok(VoidUiNode::Input {
            id: id.ok_or(VoidUiParseError::MissingProperty("id"))?,
            placeholder,
            secure,
        })
    }

    fn next(&mut self) -> Result<Token, VoidUiParseError> {
        let token = self
            .tokens
            .get(self.position)
            .cloned()
            .ok_or(VoidUiParseError::UnexpectedEof)?;
        self.position += 1;
        Ok(token)
    }

    fn next_ident(&mut self) -> Result<String, VoidUiParseError> {
        match self.next()? {
            Token::Ident(ident) => Ok(ident),
            token => Err(VoidUiParseError::UnexpectedToken(format!("{token:?}"))),
        }
    }

    fn next_string(&mut self) -> Result<String, VoidUiParseError> {
        match self.next()? {
            Token::String(value) => Ok(value),
            token => Err(VoidUiParseError::UnexpectedToken(format!("{token:?}"))),
        }
    }

    fn next_bool(&mut self) -> Result<bool, VoidUiParseError> {
        match self.next_ident()?.as_str() {
            "true" => Ok(true),
            "false" => Ok(false),
            other => Err(VoidUiParseError::InvalidBool(other.to_string())),
        }
    }

    fn expect_ident(&mut self, expected: &'static str) -> Result<(), VoidUiParseError> {
        let ident = self.next_ident()?;
        if ident == expected {
            Ok(())
        } else {
            Err(VoidUiParseError::ExpectedIdent {
                expected,
                actual: ident,
            })
        }
    }

    fn expect_lbrace(&mut self) -> Result<(), VoidUiParseError> {
        match self.next()? {
            Token::LBrace => Ok(()),
            token => Err(VoidUiParseError::UnexpectedToken(format!("{token:?}"))),
        }
    }

    fn expect_rbrace(&mut self) -> Result<(), VoidUiParseError> {
        match self.next()? {
            Token::RBrace => Ok(()),
            token => Err(VoidUiParseError::UnexpectedToken(format!("{token:?}"))),
        }
    }

    fn expect_eof(&self) -> Result<(), VoidUiParseError> {
        if self.position == self.tokens.len() {
            Ok(())
        } else {
            Err(VoidUiParseError::UnexpectedToken(format!(
                "{:?}",
                self.tokens[self.position]
            )))
        }
    }

    fn peek_rbrace(&self) -> bool {
        matches!(self.tokens.get(self.position), Some(Token::RBrace))
    }
}

#[derive(Debug, Error)]
pub enum VoidUiParseError {
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
    ExpectedIdent {
        expected: &'static str,
        actual: String,
    },
    #[error("unknown VOID UI node: {0}")]
    UnknownNode(String),
    #[error("unknown VOID UI property: {0}")]
    UnknownProperty(String),
    #[error("missing required VOID UI property: {0}")]
    MissingProperty(&'static str),
    #[error("invalid boolean literal: {0}")]
    InvalidBool(String),
    #[error("invalid VOID UI action URI: {0}")]
    InvalidAction(#[from] crate::ParseVoidUriError),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_minimal_page() {
        let page = parse_void_ui(
            r#"
            page {
              title "VOIDNET"
              column {
                text "A new layer has emerged."
                button {
                  label "Connect"
                  action "void://core/connect"
                }
              }
            }
            "#,
        )
        .unwrap();

        assert_eq!(page.title, Some("VOIDNET".to_string()));
        assert_eq!(page.children.len(), 1);
    }
}

