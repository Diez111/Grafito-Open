fn insert_implicit_multiplication(text: &str) -> String {
    let mut res = String::new();
    let chars: Vec<char> = text.chars().collect();
    for i in 0..chars.len() {
        res.push(chars[i]);
        if i + 1 < chars.len() {
            let c1 = chars[i];
            let c2 = chars[i + 1];
            if c1.is_ascii_digit() && c2.is_ascii_alphabetic() {
                res.push('*');
            }
            if c1 == ')' && c2.is_ascii_alphabetic() {
                res.push('*');
            }
            if c1 == ')' && c2.is_ascii_digit() {
                res.push('*');
            }
            if c1.is_ascii_digit() && c2 == '(' {
                res.push('*');
            }
            if c1 == ')' && c2 == '(' {
                res.push('*');
            }
        }
    }
    res
}

#[test]
fn test_implicit() {
    println!("{}", insert_implicit_multiplication("2x"));
    println!("{}", insert_implicit_multiplication("2(x+1)"));
    println!("{}", insert_implicit_multiplication("(x+1)(x-1)"));
    println!("{}", insert_implicit_multiplication("sin(x)"));
    println!("{}", insert_implicit_multiplication("2sin(x)"));
}
