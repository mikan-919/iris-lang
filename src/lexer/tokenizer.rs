use super::{Token, TokenKind};

pub struct Lexer<'a> {
    input: std::iter::Peekable<std::str::Chars<'a>>,
    line: usize,
    column: usize,
}

impl<'a> Lexer<'a> {
    pub fn new(input: &'a str) -> Self {
        Self {
            input: input.chars().peekable(),
            line: 1,
            column: 1,
        }
    }

    pub fn tokenize(&mut self) -> Vec<Token> {
        let mut tokens = Vec::new();

        while let Some(ch) = self.input.peek() {
            let ch = *ch;

            match ch {
                ' ' | '\t' => {
                    self.advance();
                }
                '\n' => {
                    self.advance();
                    self.line += 1;
                    self.column = 1;
                }
                '(' => {
                    self.advance();
                    tokens.push(Token::new(TokenKind::LParen, self.line, self.column));
                }
                ')' => {
                    self.advance();
                    tokens.push(Token::new(TokenKind::RParen, self.line, self.column));
                }
                '{' => {
                    self.advance();
                    tokens.push(Token::new(TokenKind::LBrace, self.line, self.column));
                }
                '}' => {
                    self.advance();
                    tokens.push(Token::new(TokenKind::RBrace, self.line, self.column));
                }
                ',' => {
                    self.advance();
                    tokens.push(Token::new(TokenKind::Comma, self.line, self.column));
                }
                '|' => {
                    self.advance();
                    tokens.push(Token::new(TokenKind::Pipe, self.line, self.column));
                }
                '@' => {
                    self.advance();
                    tokens.push(Token::new(TokenKind::At, self.line, self.column));
                }
                '-' => {
                    self.advance();
                    if self.peek() == Some('>') {
                        self.advance();
                        tokens.push(Token::new(TokenKind::Arrow, self.line, self.column));
                    } else {
                        self.error("unexpected '-'");
                    }
                }
                '=' => {
                    self.advance();
                    if self.peek() == Some(':') {
                        self.advance();
                        tokens.push(Token::new(TokenKind::Bind, self.line, self.column));
                    } else {
                        self.error("unexpected '='");
                    }
                }
                ':' => {
                    self.advance();
                    match self.peek() {
                        Some(':') => {
                            self.advance();
                            tokens.push(Token::new(TokenKind::Next, self.line, self.column));
                        }
                        Some('~') => {
                            self.advance();
                            tokens.push(Token::new(TokenKind::Await, self.line, self.column));
                        }
                        Some('^') => {
                            self.advance();
                            tokens.push(Token::new(TokenKind::Try, self.line, self.column));
                        }
                        Some('!') => {
                            self.advance();
                            tokens.push(Token::new(TokenKind::Force, self.line, self.column));
                        }
                        Some('?') => {
                            self.advance();
                            tokens.push(Token::new(TokenKind::Catch, self.line, self.column));
                        }
                        Some('|') => {
                            self.advance();
                            tokens.push(Token::new(TokenKind::Or, self.line, self.column));
                        }
                        Some('>') => {
                            self.advance();
                            tokens.push(Token::new(TokenKind::Tag, self.line, self.column));
                        }
                        Some('&') => {
                            self.advance();
                            tokens.push(Token::new(TokenKind::Join, self.line, self.column));
                        }
                        None | Some(' ') | Some('\t') | Some('\n') => {
                            tokens.push(Token::new(TokenKind::Colon, self.line, self.column));
                        }
                        Some(ch) if ch.is_alphabetic() || ch == '_' => {
                            tokens.push(Token::new(TokenKind::Colon, self.line, self.column));
                        }
                        _ => self.error("unexpected ':'"),
                    }
                }
                '0'..='9' => {
                    let num = self.read_number();
                    tokens.push(Token::new(TokenKind::Integer(num), self.line, self.column));
                }
                '"' => {
                    let s = self.read_string();
                    tokens.push(Token::new(TokenKind::String(s), self.line, self.column));
                }
                'a'..='z' | 'A'..='Z' | '_' => {
                    let ident = self.read_identifier();
                    let kind = match ident.as_str() {
                        "let" => TokenKind::Let,
                        "fn" => TokenKind::Fn,
                        "export" => TokenKind::Export,
                        "match" => TokenKind::Match,
                        "for" => TokenKind::For,
                        _ => TokenKind::Identifier(ident),
                    };
                    tokens.push(Token::new(kind, self.line, self.column));
                }
                _ => {
                    self.advance();
                    self.error(&format!("unexpected character: {}", ch));
                }
            }
        }

        tokens.push(Token::new(TokenKind::EOF, self.line, self.column));
        tokens
    }

    fn advance(&mut self) {
        self.input.next();
        self.column += 1;
    }

    fn peek(&mut self) -> Option<char> {
        self.input.peek().copied()
    }

    fn read_number(&mut self) -> i64 {
        let mut num_str = String::new();
        while let Some(ch) = self.peek() {
            if ch.is_ascii_digit() {
                num_str.push(ch);
                self.advance();
            } else {
                break;
            }
        }
        num_str.parse().unwrap_or(0)
    }

    fn read_string(&mut self) -> String {
        self.advance();
        let mut s = String::new();
        while let Some(ch) = self.peek() {
            if ch == '"' {
                self.advance();
                break;
            } else {
                s.push(ch);
                self.advance();
            }
        }
        s
    }

    fn read_identifier(&mut self) -> String {
        let mut ident = String::new();
        while let Some(ch) = self.peek() {
            if ch.is_alphanumeric() || ch == '_' {
                ident.push(ch);
                self.advance();
            } else {
                break;
            }
        }
        ident
    }

    fn error(&self, msg: &str) {
        eprintln!("Lexer error at {}:{}: {}", self.line, self.column, msg);
    }
}
