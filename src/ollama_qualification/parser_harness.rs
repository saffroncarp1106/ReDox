mod solution;
use solution::{parse_filter, Expr, ParseError};

fn nodes(expr: Expr) -> usize {
    match expr {
        Expr::Identifier(name) => { let _: String = name; 1 }
        Expr::Not(child) => 1 + nodes(*child),
        Expr::And(left, right) | Expr::Or(left, right) => 1 + nodes(*left) + nodes(*right),
    }
}

fn main() {
    let parse: fn(&str) -> Result<Expr, ParseError> = parse_filter;
    match parse("active && (!archived || urgent)") {
        Ok(tree) => println!("{}", nodes(tree)),
        Err(ParseError { offset, message }) => {
            let _: (usize, String) = (offset, message);
        }
    }
}
