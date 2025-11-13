use crate::{error::RollatoriumError, token::Token};

pub(crate) struct Lexer {
    chars: Vec<char>,
    pos: usize,
    annotation_mode: bool,
}

impl Lexer {
    pub fn new(input: &str) -> Self {
        Lexer {
            chars: input.chars().collect(),
            pos: 0,
            annotation_mode: false,
        }
    }

    fn is_at_end(&self) -> bool {
        self.pos >= self.chars.len()
    }

    fn peek(&self) -> char {
        self.peek_offset(0)
    }

    fn peek_offset(&self, offset: usize) -> char {
        let idx = self.pos + offset;
        *self.chars.get(idx).unwrap_or(&'\0')
    }

    fn advance(&mut self) {
        if self.pos < self.chars.len() {
            self.pos += 1;
        }
    }

    fn advance_by(&mut self, count: usize) {
        for _ in 0..count {
            self.advance();
        }
    }

    fn skip_ws(&mut self) {
        while !self.is_at_end() && self.peek().is_whitespace() {
            self.advance();
        }
    }

    fn starts_with(&self, pattern: &str) -> bool {
        pattern
            .chars()
            .enumerate()
            .all(|(idx, ch)| self.peek_offset(idx) == ch)
    }

    fn number(&mut self) -> crate::Result<Token> {
        let start = self.pos;
        let mut seen_digit = false;
        let mut seen_dot = false;

        while !self.is_at_end() {
            let c = self.peek();
            if c.is_ascii_digit() {
                seen_digit = true;
                self.advance();
            } else if c == '.' && !seen_dot {
                let next = self.peek_offset(1);
                if !next.is_ascii_digit() {
                    return Err(RollatoriumError::Lexer(format!(
                        "Invalid decimal literal starting at position {}",
                        start
                    )));
                }
                seen_dot = true;
                self.advance();
            } else {
                break;
            }
        }

        if !seen_digit {
            return Err(RollatoriumError::Lexer(format!(
                "Number literal missing digits at position {}",
                start
            )));
        }

        let num_str: String = self.chars[start..self.pos].iter().collect();
        match num_str.parse::<f64>() {
            Ok(value) => Ok(Token::Number(value)),
            Err(_) => Err(RollatoriumError::Lexer(format!(
                "Failed to parse number literal '{}'",
                num_str
            ))),
        }
    }

    pub fn next_token(&mut self) -> crate::Result<Token> {
        if !self.annotation_mode {
            self.skip_ws();
        }
        if self.is_at_end() {
            return Ok(Token::Eof);
        }

        if self.annotation_mode {
            let start = self.pos;
            while !self.is_at_end() {
                let c = self.peek();
                if c == ']' {
                    break;
                }
                self.advance();
            }

            if self.is_at_end() {
                return Err(RollatoriumError::Lexer(
                    "Unterminated annotation; missing closing ']'".into(),
                ));
            }

            let text: String = self.chars[start..self.pos].iter().collect();
            self.annotation_mode = false;
            return Ok(Token::AnnotationText(text.trim().to_string()));
        }

        if self.starts_with("//") {
            self.advance_by(2);
            return Ok(Token::DoubleSlash);
        }
        if self.starts_with("==") {
            self.advance_by(2);
            return Ok(Token::EqualEqual);
        }
        if self.starts_with("!=") {
            self.advance_by(2);
            return Ok(Token::NotEqual);
        }
        if self.starts_with(">=") {
            self.advance_by(2);
            return Ok(Token::GreaterEqual);
        }
        if self.starts_with("<=") {
            self.advance_by(2);
            return Ok(Token::LessEqual);
        }
        if self.starts_with("rr") {
            self.advance_by(2);
            return Ok(Token::Reroll);
        }
        if self.starts_with("ro") {
            self.advance_by(2);
            return Ok(Token::RerollOnce);
        }
        if self.starts_with("ra") {
            self.advance_by(2);
            return Ok(Token::RerollAdd);
        }
        if self.starts_with("mi") {
            self.advance_by(2);
            return Ok(Token::Min);
        }
        if self.starts_with("ma") {
            self.advance_by(2);
            return Ok(Token::Max);
        }

        if self.starts_with("d%") {
            self.advance_by(2);
            return Ok(Token::DicePercent);
        }

        let c = self.peek();
        match c {
            '+' => {
                self.advance();
                Ok(Token::Plus)
            }
            '-' => {
                self.advance();
                Ok(Token::Minus)
            }
            '*' => {
                self.advance();
                Ok(Token::Star)
            }
            '/' => {
                self.advance();
                Ok(Token::Slash)
            }
            '%' => {
                self.advance();
                Ok(Token::Percent)
            }
            '>' => {
                self.advance();
                Ok(Token::Greater)
            }
            '<' => {
                self.advance();
                Ok(Token::Less)
            }
            '(' => {
                self.advance();
                Ok(Token::LParen)
            }
            ')' => {
                self.advance();
                Ok(Token::RParen)
            }
            '{' => {
                self.advance();
                Ok(Token::SetStart)
            }
            '}' => {
                self.advance();
                Ok(Token::SetEnd)
            }
            '[' => {
                self.advance();
                self.annotation_mode = true;
                Ok(Token::AnnotationStart)
            }
            ']' => {
                self.advance();
                Ok(Token::AnnotationEnd)
            }
            ',' => {
                self.advance();
                Ok(Token::Comma)
            }
            'd' => {
                self.advance();
                Ok(Token::Dice)
            }
            'k' => {
                self.advance();
                Ok(Token::Keep)
            }
            'p' => {
                self.advance();
                Ok(Token::Drop)
            }
            'e' => {
                self.advance();
                Ok(Token::Explode)
            }
            '!' => {
                self.advance();
                Ok(Token::Explode)
            }
            'h' => {
                self.advance();
                Ok(Token::SelectorHigh)
            }
            'l' => {
                self.advance();
                Ok(Token::SelectorLow)
            }
            '=' => Err(RollatoriumError::Lexer(format!(
                "Unexpected '=' at position {}. Did you mean '=='?",
                self.pos
            ))),
            c if c.is_ascii_digit() || (c == '.' && self.peek_offset(1).is_ascii_digit()) => {
                self.number()
            }
            _ => Err(RollatoriumError::Lexer(format!(
                "Unexpected character '{}' at position {}",
                c, self.pos
            ))),
        }
    }
}
