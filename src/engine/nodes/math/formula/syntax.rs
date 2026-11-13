//! The grammar of a formula, as a tree of names. Names are not looked up here:
//! the node does that afterwards, in evaluation order, so that a syntax error
//! anywhere in the text is reported before a name that does not exist.

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Op {
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    Pow,
}

#[derive(Debug)]
pub enum Ast {
    Number(f64),
    Name(String),
    Neg(Box<Ast>),
    Binary(Op, Box<Ast>, Box<Ast>),
    Call(String, Vec<Ast>),
}

/// Deep enough for any formula a person writes, shallow enough that the
/// recursive descent cannot overflow the stack of the thread that parses.
const MAX_DEPTH: usize = 100;

/// Parse an expression:
///
/// ```text
/// sum    = term (('+' | '-') term)*
/// term   = unary (('*' | '/' | '%') unary)*
/// unary  = ('-' | '+') unary | power
/// power  = atom ('^' unary)?
/// atom   = number | name | name '(' sum (',' sum)* ')' | '(' sum ')'
/// ```
///
/// A sign binds tighter than `*` and looser than `^` (`-x^2` is `-(x^2)`), and
/// `^` groups to the right (`2^3^2` is `2^9`).
pub fn parse(source: &str) -> Result<Ast, String> {
    let mut p = Parser { src: source.as_bytes(), pos: 0, depth: 0 };
    let ast = p.sum()?;
    p.skip_space();
    match p.peek() {
        None => Ok(ast),
        Some(b')') => Err(format!("unexpected ')' at byte {}", p.pos)),
        Some(_) => Err(format!("unexpected character at byte {}", p.pos)),
    }
}

struct Parser<'a> {
    src: &'a [u8],
    pos: usize,
    depth: usize,
}

impl Parser<'_> {
    fn peek(&self) -> Option<u8> {
        self.src.get(self.pos).copied()
    }

    fn skip_space(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\t' | b'\r' | b'\n')) {
            self.pos += 1;
        }
    }

    /// Consume `c` if it is next, after any white space.
    fn eat(&mut self, c: u8) -> bool {
        self.skip_space();
        if self.peek() == Some(c) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    fn nested(&mut self, f: fn(&mut Self) -> Result<Ast, String>) -> Result<Ast, String> {
        if self.depth >= MAX_DEPTH {
            return Err("the formula is nested too deeply".to_string());
        }
        self.depth += 1;
        let result = f(self);
        self.depth -= 1;
        result
    }

    fn sum(&mut self) -> Result<Ast, String> {
        let mut left = self.term()?;
        loop {
            let op = if self.eat(b'+') {
                Op::Add
            } else if self.eat(b'-') {
                Op::Sub
            } else {
                return Ok(left);
            };
            let right = self.term()?;
            left = Ast::Binary(op, Box::new(left), Box::new(right));
        }
    }

    fn term(&mut self) -> Result<Ast, String> {
        let mut left = self.unary()?;
        loop {
            let op = if self.eat(b'*') {
                Op::Mul
            } else if self.eat(b'/') {
                Op::Div
            } else if self.eat(b'%') {
                Op::Rem
            } else {
                return Ok(left);
            };
            let right = self.unary()?;
            left = Ast::Binary(op, Box::new(left), Box::new(right));
        }
    }

    fn unary(&mut self) -> Result<Ast, String> {
        if self.eat(b'-') {
            let inner = self.nested(Self::unary)?;
            return Ok(Ast::Neg(Box::new(inner)));
        }
        if self.eat(b'+') {
            return self.nested(Self::unary);
        }
        let base = self.atom()?;
        if self.eat(b'^') {
            let exponent = self.nested(Self::unary)?;
            return Ok(Ast::Binary(Op::Pow, Box::new(base), Box::new(exponent)));
        }
        Ok(base)
    }

    fn atom(&mut self) -> Result<Ast, String> {
        self.skip_space();
        match self.peek() {
            None => Err("missing argument at the end of the expression".to_string()),
            Some(b'(') => {
                self.pos += 1;
                let inner = self.nested(Self::sum)?;
                self.close_bracket()?;
                Ok(inner)
            }
            Some(c) if c.is_ascii_digit() => self.number(),
            Some(c) if c.is_ascii_alphabetic() || c == b'_' => self.name_or_call(),
            Some(_) => Err(format!("unexpected character at byte {}", self.pos)),
        }
    }

    fn close_bracket(&mut self) -> Result<(), String> {
        if self.eat(b')') {
            Ok(())
        } else if self.peek().is_none() {
            Err("missing ')'".to_string())
        } else {
            Err(format!("unexpected character at byte {}", self.pos))
        }
    }

    fn digits(&mut self) -> usize {
        let start = self.pos;
        while self.peek().is_some_and(|c| c.is_ascii_digit()) {
            self.pos += 1;
        }
        self.pos - start
    }

    /// `12`, `1.`, `1.5`, `1e3`, `2.5E-2`: digits first, and an `e` must be
    /// followed by an exponent.
    fn number(&mut self) -> Result<Ast, String> {
        let start = self.pos;
        self.digits();
        if self.peek() == Some(b'.') {
            self.pos += 1;
            self.digits();
        }
        if matches!(self.peek(), Some(b'e' | b'E')) {
            self.pos += 1;
            if matches!(self.peek(), Some(b'+' | b'-')) {
                self.pos += 1;
            }
            if self.digits() == 0 {
                return Err(format!("unexpected character at byte {}", self.pos));
            }
        }
        // Only ASCII was consumed, so this is valid text.
        let text = String::from_utf8_lossy(&self.src[start..self.pos]);
        text.parse::<f64>().map(Ast::Number).map_err(|_| format!("bad number at byte {start}"))
    }

    fn name_or_call(&mut self) -> Result<Ast, String> {
        let start = self.pos;
        while self.peek().is_some_and(|c| c.is_ascii_alphanumeric() || c == b'_') {
            self.pos += 1;
        }
        let name = String::from_utf8_lossy(&self.src[start..self.pos]).into_owned();
        if !self.eat(b'(') {
            return Ok(Ast::Name(name));
        }
        let mut args = vec![self.nested(Self::sum)?];
        while self.eat(b',') {
            args.push(self.nested(Self::sum)?);
        }
        self.close_bracket()?;
        Ok(Ast::Call(name, args))
    }
}
