//! REP-149 conditional expression parser and evaluator.
//!
//! Grammar (from the REP-149 spec):
//! ```text
//! expr     := or_expr
//! or_expr  := and_expr ('or' and_expr)*
//! and_expr := atom ('and' atom)*
//! atom     := comparison | '(' expr ')'
//! compare  := value op value
//! op       := '==' | '!=' | '<' | '<=' | '>' | '>='
//! value    := '$' IDENT | QUOTED_STRING | UNQUOTED_STRING
//! ```
//!
//! Variables are substituted before evaluation. Unresolved variables
//! cause the expression to evaluate conservatively to `true` (never
//! silently drop a dependency).

use std::collections::HashMap;

// ── AST ──────────────────────────────────────────────────────────────────────

#[derive(Debug, PartialEq)]
enum Expr {
    Compare { lhs: Value, op: CmpOp, rhs: Value },
    And(Vec<Expr>),
    Or(Vec<Expr>),
}

#[derive(Debug, PartialEq)]
enum Value {
    Var(String),
    Literal(String),
}

#[derive(Debug, PartialEq, Clone, Copy)]
enum CmpOp {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}

// ── Tokeniser ────────────────────────────────────────────────────────────────

#[derive(Debug, PartialEq)]
enum Token {
    LParen,
    RParen,
    And,
    Or,
    Op(CmpOp),
    Var(String),     // $IDENT
    Literal(String), // quoted or unquoted string
}

fn tokenise(input: &str) -> Vec<Token> {
    let mut tokens = Vec::new();
    let mut chars = input.chars().peekable();

    while let Some(&c) = chars.peek() {
        match c {
            ' ' | '\t' | '\n' | '\r' => {
                chars.next();
            }
            '(' => {
                tokens.push(Token::LParen);
                chars.next();
            }
            ')' => {
                tokens.push(Token::RParen);
                chars.next();
            }
            '=' => {
                chars.next();
                if chars.peek() == Some(&'=') {
                    chars.next();
                    tokens.push(Token::Op(CmpOp::Eq));
                }
            }
            '!' => {
                chars.next();
                if chars.peek() == Some(&'=') {
                    chars.next();
                    tokens.push(Token::Op(CmpOp::Ne));
                }
            }
            '<' => {
                chars.next();
                if chars.peek() == Some(&'=') {
                    chars.next();
                    tokens.push(Token::Op(CmpOp::Le));
                } else {
                    tokens.push(Token::Op(CmpOp::Lt));
                }
            }
            '>' => {
                chars.next();
                if chars.peek() == Some(&'=') {
                    chars.next();
                    tokens.push(Token::Op(CmpOp::Ge));
                } else {
                    tokens.push(Token::Op(CmpOp::Gt));
                }
            }
            '$' => {
                chars.next();
                let mut name = String::new();
                while let Some(&ch) = chars.peek() {
                    if ch.is_alphanumeric() || ch == '_' {
                        name.push(ch);
                        chars.next();
                    } else {
                        break;
                    }
                }
                tokens.push(Token::Var(name));
            }
            '"' | '\'' => {
                let quote = c;
                chars.next();
                let mut s = String::new();
                while let Some(&ch) = chars.peek() {
                    if ch == quote {
                        chars.next();
                        break;
                    }
                    s.push(ch);
                    chars.next();
                }
                tokens.push(Token::Literal(s));
            }
            _ => {
                // Unquoted literal: alphanumeric, underscore, dash, dot
                let mut s = String::new();
                while let Some(&ch) = chars.peek() {
                    if ch.is_alphanumeric() || ch == '_' || ch == '-' || ch == '.' {
                        s.push(ch);
                        chars.next();
                    } else {
                        break;
                    }
                }
                if s.is_empty() {
                    // Skip unknown character
                    chars.next();
                    continue;
                }
                // Check for keywords
                match s.as_str() {
                    "and" => tokens.push(Token::And),
                    "or" => tokens.push(Token::Or),
                    _ => tokens.push(Token::Literal(s)),
                }
            }
        }
    }
    tokens
}

// ── Parser ───────────────────────────────────────────────────────────────────

struct Parser {
    tokens: Vec<Token>,
    pos: usize,
}

impl Parser {
    fn new(tokens: Vec<Token>) -> Self {
        Self { tokens, pos: 0 }
    }

    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.pos)
    }

    fn advance(&mut self) {
        self.pos += 1;
    }

    fn parse_expr(&mut self) -> Option<Expr> {
        self.parse_or()
    }

    fn parse_or(&mut self) -> Option<Expr> {
        let mut left = self.parse_and()?;
        while self.peek() == Some(&Token::Or) {
            self.advance();
            let right = self.parse_and()?;
            left = match left {
                Expr::Or(mut exprs) => {
                    exprs.push(right);
                    Expr::Or(exprs)
                }
                _ => Expr::Or(vec![left, right]),
            };
        }
        Some(left)
    }

    fn parse_and(&mut self) -> Option<Expr> {
        let mut left = self.parse_atom()?;
        while self.peek() == Some(&Token::And) {
            self.advance();
            let right = self.parse_atom()?;
            left = match left {
                Expr::And(mut exprs) => {
                    exprs.push(right);
                    Expr::And(exprs)
                }
                _ => Expr::And(vec![left, right]),
            };
        }
        Some(left)
    }

    fn parse_atom(&mut self) -> Option<Expr> {
        if self.peek() == Some(&Token::LParen) {
            self.advance();
            let expr = self.parse_expr()?;
            if self.peek() == Some(&Token::RParen) {
                self.advance();
            }
            return Some(expr);
        }
        self.parse_comparison()
    }

    fn parse_comparison(&mut self) -> Option<Expr> {
        let lhs = self.parse_value()?;
        let op = match self.peek()? {
            Token::Op(op) => *op,
            _ => return None,
        };
        self.advance();
        let rhs = self.parse_value()?;
        Some(Expr::Compare { lhs, op, rhs })
    }

    fn parse_value(&mut self) -> Option<Value> {
        match self.peek()? {
            Token::Var(_) | Token::Literal(_) => {
                let tok = &self.tokens[self.pos];
                let val = match tok {
                    Token::Var(name) => Value::Var(name.clone()),
                    Token::Literal(s) => Value::Literal(s.clone()),
                    _ => unreachable!(),
                };
                self.advance();
                Some(val)
            }
            _ => None,
        }
    }
}

// ── Evaluator ────────────────────────────────────────────────────────────────

/// Evaluate a REP-149 condition expression.
///
/// Variables are substituted from `vars`. If any variable remains unresolved
/// after substitution, the expression evaluates to `true` (conservative).
///
/// Default variables: `$ROS_VERSION` = `"2"`.
pub fn eval_condition(condition: &str) -> bool {
    let mut vars = HashMap::new();
    vars.insert("ROS_VERSION".to_owned(), "2".to_owned());
    eval_condition_with_vars(condition, &vars)
}

/// Evaluate with custom variable bindings.
pub fn eval_condition_with_vars(condition: &str, vars: &HashMap<String, String>) -> bool {
    let tokens = tokenise(condition);
    if tokens.is_empty() {
        return true;
    }

    let mut parser = Parser::new(tokens);
    match parser.parse_expr() {
        Some(expr) => eval_expr(&expr, vars),
        None => true, // parse error → conservative
    }
}

fn eval_expr(expr: &Expr, vars: &HashMap<String, String>) -> bool {
    match expr {
        Expr::Compare { lhs, op, rhs } => {
            let lhs_val = resolve_value(lhs, vars);
            let rhs_val = resolve_value(rhs, vars);
            // If either side is an unresolved variable, be conservative.
            if lhs_val.is_none() || rhs_val.is_none() {
                return true;
            }
            let l = lhs_val.unwrap();
            let r = rhs_val.unwrap();
            match op {
                CmpOp::Eq => l == r,
                CmpOp::Ne => l != r,
                CmpOp::Lt => l < r,
                CmpOp::Le => l <= r,
                CmpOp::Gt => l > r,
                CmpOp::Ge => l >= r,
            }
        }
        Expr::And(exprs) => exprs.iter().all(|e| eval_expr(e, vars)),
        Expr::Or(exprs) => exprs.iter().any(|e| eval_expr(e, vars)),
    }
}

/// Resolve a value. Returns `None` for unresolved variables.
fn resolve_value(val: &Value, vars: &HashMap<String, String>) -> Option<String> {
    match val {
        Value::Literal(s) => Some(s.clone()),
        Value::Var(name) => vars.get(name).cloned(),
    }
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── Tokeniser ────────────────────────────────────────────────────────

    #[test]
    fn tokenise_simple_comparison() {
        let tokens = tokenise("$ROS_VERSION == 2");
        assert_eq!(
            tokens,
            vec![
                Token::Var("ROS_VERSION".into()),
                Token::Op(CmpOp::Eq),
                Token::Literal("2".into()),
            ]
        );
    }

    #[test]
    fn tokenise_quoted_strings() {
        let tokens = tokenise("$ROS_DISTRO == \"humble\"");
        assert_eq!(
            tokens,
            vec![
                Token::Var("ROS_DISTRO".into()),
                Token::Op(CmpOp::Eq),
                Token::Literal("humble".into()),
            ]
        );
    }

    #[test]
    fn tokenise_and_or() {
        let tokens = tokenise("$ROS_VERSION == 2 and $ARCH == arm64");
        assert_eq!(
            tokens,
            vec![
                Token::Var("ROS_VERSION".into()),
                Token::Op(CmpOp::Eq),
                Token::Literal("2".into()),
                Token::And,
                Token::Var("ARCH".into()),
                Token::Op(CmpOp::Eq),
                Token::Literal("arm64".into()),
            ]
        );
    }

    #[test]
    fn tokenise_parens() {
        let tokens = tokenise("($A == 1 or $B == 2) and $C == 3");
        assert!(tokens.contains(&Token::LParen));
        assert!(tokens.contains(&Token::RParen));
        assert!(tokens.contains(&Token::Or));
        assert!(tokens.contains(&Token::And));
    }

    #[test]
    fn tokenise_all_operators() {
        let toks = tokenise("== != < <= > >=");
        assert_eq!(
            toks,
            vec![
                Token::Op(CmpOp::Eq),
                Token::Op(CmpOp::Ne),
                Token::Op(CmpOp::Lt),
                Token::Op(CmpOp::Le),
                Token::Op(CmpOp::Gt),
                Token::Op(CmpOp::Ge),
            ]
        );
    }

    #[test]
    fn tokenise_empty() {
        assert!(tokenise("").is_empty());
        assert!(tokenise("   ").is_empty());
    }

    // ── Evaluator (via eval_condition) ───────────────────────────────────

    #[test]
    fn simple_ros_version() {
        assert!(eval_condition("$ROS_VERSION == 2"));
        assert!(!eval_condition("$ROS_VERSION == 1"));
        assert!(eval_condition("$ROS_VERSION != 1"));
        assert!(!eval_condition("$ROS_VERSION != 2"));
    }

    #[test]
    fn and_expression() {
        // Both true
        assert!(eval_condition("$ROS_VERSION == 2 and $ROS_VERSION != 1"));
        // First false
        assert!(!eval_condition("$ROS_VERSION == 1 and $ROS_VERSION != 1"));
        // Second false
        assert!(!eval_condition("$ROS_VERSION == 2 and $ROS_VERSION == 1"));
    }

    #[test]
    fn or_expression() {
        // First true
        assert!(eval_condition("$ROS_VERSION == 2 or $ROS_VERSION == 1"));
        // Second true
        assert!(eval_condition("$ROS_VERSION == 1 or $ROS_VERSION == 2"));
        // Both false
        assert!(!eval_condition("$ROS_VERSION == 1 or $ROS_VERSION == 3"));
    }

    #[test]
    fn and_binds_tighter_than_or() {
        // "$V == 1 or $V == 2 and $V == 2"
        // Should parse as: ($V == 1) or (($V == 2) and ($V == 2))
        // = false or (true and true) = true
        assert!(eval_condition(
            "$ROS_VERSION == 1 or $ROS_VERSION == 2 and $ROS_VERSION == 2"
        ));
    }

    #[test]
    fn parenthesised_grouping() {
        // "($V == 1 or $V == 2) and $V == 2"
        // = (false or true) and true = true
        assert!(eval_condition(
            "($ROS_VERSION == 1 or $ROS_VERSION == 2) and $ROS_VERSION == 2"
        ));
        // "($V == 1 or $V == 2) and $V == 1"
        // = (false or true) and false = false
        assert!(!eval_condition(
            "($ROS_VERSION == 1 or $ROS_VERSION == 2) and $ROS_VERSION == 1"
        ));
    }

    #[test]
    fn unresolved_variable_is_conservative() {
        // Unknown variable → true (include the dep)
        assert!(eval_condition("$PLATFORM == jetson"));
        assert!(eval_condition("$ROS_DISTRO == humble"));
    }

    #[test]
    fn unresolved_in_and_still_conservative() {
        // $ROS_VERSION == 2 and $UNKNOWN == foo
        // = true and true(conservative) = true
        assert!(eval_condition("$ROS_VERSION == 2 and $UNKNOWN == foo"));
    }

    #[test]
    fn unresolved_in_or_with_known_false() {
        // $ROS_VERSION == 1 or $UNKNOWN == foo
        // = false or true(conservative) = true
        assert!(eval_condition("$ROS_VERSION == 1 or $UNKNOWN == foo"));
    }

    #[test]
    fn quoted_string_values() {
        let mut vars = HashMap::new();
        vars.insert("DISTRO".into(), "humble".into());
        assert!(eval_condition_with_vars("$DISTRO == \"humble\"", &vars));
        assert!(!eval_condition_with_vars("$DISTRO == \"jazzy\"", &vars));
    }

    #[test]
    fn comparison_operators() {
        let mut vars = HashMap::new();
        vars.insert("V".into(), "2".into());
        assert!(eval_condition_with_vars("$V >= 1", &vars));
        assert!(eval_condition_with_vars("$V >= 2", &vars));
        assert!(!eval_condition_with_vars("$V >= 3", &vars));
        assert!(eval_condition_with_vars("$V > 1", &vars));
        assert!(!eval_condition_with_vars("$V > 2", &vars));
        assert!(eval_condition_with_vars("$V < 3", &vars));
        assert!(!eval_condition_with_vars("$V < 2", &vars));
        assert!(eval_condition_with_vars("$V <= 2", &vars));
        assert!(!eval_condition_with_vars("$V <= 1", &vars));
    }

    #[test]
    fn malformed_input_is_conservative() {
        assert!(eval_condition(""));
        assert!(eval_condition("   "));
        assert!(eval_condition("not_a_valid_expression"));
        assert!(eval_condition("== =="));
    }

    #[test]
    fn backwards_compatible_with_existing_fixture() {
        // These are the conditions from conditional_deps.xml
        assert!(!eval_condition("$ROS_VERSION == 1"));
        assert!(eval_condition("$ROS_VERSION == 2"));
    }
}
