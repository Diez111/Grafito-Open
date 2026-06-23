use grafito_geometry::ast::parse_ast;

fn main() {
    let expr = "x^3";
    let ast = parse_ast(expr).unwrap();
    let diff = ast.diff("x");
    println!("Diff raw: {:?}", diff);
    let simplified = diff.simplify();
    println!("Diff simplified: {}", simplified);

    let expr2 = "sin(x^2)";
    let ast2 = parse_ast(expr2).unwrap();
    println!("Diff sin(x^2): {}", ast2.diff("x").simplify());
}
