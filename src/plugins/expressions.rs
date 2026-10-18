//! Calculator syntax is shared by dimensions, features, variables and agents.
use rhai::{Engine, EvalAltResult};
type Unary = (&'static str, fn(f64) -> f64);
type Binary = (&'static str, fn(f64, f64) -> f64);

fn checked(value: f64) -> Result<f64, Box<EvalAltResult>> {
    if value.is_finite() {
        Ok(value)
    } else {
        Err("Outside the function's real, finite domain".into())
    }
}

pub(super) fn functions(engine: &mut Engine) {
    let unary: &[Unary] = &[
        ("sin", |x| x.to_radians().sin()),
        ("cos", |x| x.to_radians().cos()),
        ("tan", |x| x.to_radians().tan()),
        ("asin", |x| x.asin().to_degrees()),
        ("acos", |x| x.acos().to_degrees()),
        ("atan", |x| x.atan().to_degrees()),
        ("sin_rad", f64::sin),
        ("cos_rad", f64::cos),
        ("tan_rad", f64::tan),
        ("sqrt", f64::sqrt),
        ("root", f64::sqrt),
        ("cbrt", f64::cbrt),
        ("log", f64::ln),
        ("ln", f64::ln),
        ("log10", f64::log10),
        ("exp", f64::exp),
        ("abs", f64::abs),
        ("floor", f64::floor),
        ("ceil", f64::ceil),
        ("round", f64::round),
        ("sinh", f64::sinh),
        ("cosh", f64::cosh),
        ("tanh", f64::tanh),
        ("asinh", f64::asinh),
        ("acosh", f64::acosh),
        ("atanh", f64::atanh),
    ];
    for &(name, operation) in unary {
        engine.register_fn(name, move |x: f64| checked(operation(checked(x)?)));
    }
    let binary: &[Binary] = &[
        ("atan2", |y, x| y.atan2(x).to_degrees()),
        ("pow", f64::powf),
        ("min", f64::min),
        ("max", f64::max),
        ("root", |x, degree| {
            if degree == 0. {
                return f64::NAN;
            }
            if x < 0. && degree.fract() == 0. && degree % 2. != 0. {
                -(-x).powf(1. / degree)
            } else {
                x.powf(1. / degree)
            }
        }),
    ];
    for &(name, operation) in binary {
        engine.register_fn(name, move |a: f64, b: f64| {
            checked(operation(checked(a)?, checked(b)?))
        });
    }
}

#[derive(Clone)]
enum Token {
    Number(String),
    Name(String),
    Open,
    Close,
    Operator(char),
    Comma,
}
impl Token {
    fn ends_value(&self) -> bool {
        matches!(self, Self::Number(_) | Self::Name(_) | Self::Close)
    }
    fn starts_value(&self) -> bool {
        matches!(self, Self::Number(_) | Self::Name(_) | Self::Open)
    }
}

struct Parser<'a> {
    tokens: &'a [Token],
    index: usize,
}
impl Parser<'_> {
    fn expression(&mut self, precedence: u8, depth: usize) -> Result<String, String> {
        if depth > 64 {
            return Err("Expression is nested too deeply".into());
        }
        let mut left = match self.tokens.get(self.index) {
            Some(Token::Operator(sign @ ('+' | '-'))) => {
                let negative = *sign == '-';
                self.index += 1;
                let value = self.expression(30, depth + 1)?;
                if negative {
                    format!("(0-{value})")
                } else {
                    value
                }
            }
            Some(Token::Number(s)) => {
                self.index += 1;
                s.clone()
            }
            Some(Token::Name(name)) => {
                self.index += 1;
                let name = name.trim_start_matches('#');
                if matches!(self.tokens.get(self.index), Some(Token::Open)) {
                    self.index += 1;
                    let mut arguments = Vec::new();
                    loop {
                        arguments.push(self.expression(0, depth + 1)?);
                        if matches!(self.tokens.get(self.index), Some(Token::Comma)) {
                            self.index += 1;
                        } else {
                            break;
                        }
                    }
                    self.close()?;
                    format!("{name}({})", arguments.join(","))
                } else {
                    name.into()
                }
            }
            Some(Token::Open) => {
                self.index += 1;
                let value = self.expression(0, depth + 1)?;
                self.close()?;
                value
            }
            _ => return Err("Expected a number, variable or function".into()),
        };
        while let Some(Token::Operator(operator)) = self.tokens.get(self.index) {
            let operator = *operator;
            let priority = match operator {
                '+' | '-' => 10,
                '*' | '/' | '%' => 20,
                '^' => 40,
                _ => unreachable!(),
            };
            if priority < precedence {
                break;
            }
            self.index += 1;
            let right = self.expression(priority + u8::from(operator != '^'), depth + 1)?;
            left = format!("({left}{operator}{right})");
        }
        Ok(left)
    }
    fn close(&mut self) -> Result<(), String> {
        if !matches!(self.tokens.get(self.index), Some(Token::Close)) {
            return Err("Expected a closing parenthesis".into());
        }
        self.index += 1;
        Ok(())
    }
}

pub(super) fn normalize(expression: &str) -> Result<String, String> {
    let source = expression
        .replace(['×', '·'], "*")
        .replace('÷', "/")
        .replace('π', "pi")
        .replace('√', "sqrt")
        .replace('°', "deg");
    let b = source.as_bytes();
    let mut tokens = Vec::new();
    let mut i = 0;
    while i < b.len() {
        let start = i;
        match b[i] {
            c if c.is_ascii_whitespace() => {
                i += 1;
                continue;
            }
            b'0'..=b'9' | b'.' => {
                i += 1;
                while i < b.len() && (b[i].is_ascii_digit() || b[i] == b'.') {
                    i += 1;
                }
                if i < b.len() && matches!(b[i], b'e' | b'E') {
                    let mut end = i + 1;
                    if end < b.len() && matches!(b[end], b'+' | b'-') {
                        end += 1;
                    }
                    let digits = end;
                    while end < b.len() && b[end].is_ascii_digit() {
                        end += 1;
                    }
                    if end > digits {
                        i = end;
                    }
                }
                tokens.push(Token::Number(source[start..i].into()));
                continue;
            }
            b'#' | b'a'..=b'z' | b'A'..=b'Z' | b'_' => {
                i += 1;
                while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'_') {
                    i += 1;
                }
                let name = &source[start..i];
                if name == "#" {
                    return Err("A variable name must follow #".into());
                }
                tokens.push(Token::Name(name.into()));
                continue;
            }
            b'(' => tokens.push(Token::Open),
            b')' => tokens.push(Token::Close),
            b',' => tokens.push(Token::Comma),
            b'*' if b.get(i + 1) == Some(&b'*') => {
                tokens.push(Token::Operator('^'));
                i += 2;
                continue;
            }
            c @ (b'+' | b'-' | b'*' | b'/' | b'^' | b'%') => {
                tokens.push(Token::Operator(c as char))
            }
            _ => {
                return Err(
                    "Use numbers, variables, math functions and arithmetic operators".into(),
                );
            }
        }
        i += 1;
    }
    // A spaced x between operands is multiplication; #x always names a variable.
    for i in 1..tokens.len().saturating_sub(1) {
        if matches!(&tokens[i], Token::Name(name) if name == "x")
            && tokens[i - 1].ends_value()
            && tokens[i + 1].starts_value()
        {
            tokens[i] = Token::Operator('*');
        }
    }
    let mut explicit = Vec::new();
    for (i, token) in tokens.iter().enumerate() {
        if let Some(previous) = i.checked_sub(1).map(|n| &tokens[n])
            && previous.ends_value()
            && token.starts_value()
            && !matches!((previous, token), (Token::Name(_), Token::Open))
        {
            explicit.push(Token::Operator('*'));
        }
        explicit.push(token.clone());
    }
    let mut parser = Parser {
        tokens: &explicit,
        index: 0,
    };
    let output = parser.expression(0, 0)?;
    if parser.index != explicit.len() {
        return Err("Unexpected expression token".into());
    }
    Ok(output)
}
