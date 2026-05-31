fn insert_implicit_multiplication(input: &str) -> String {
    let mut result = String::new();
    let chars: Vec<char> = input.chars().collect();
    for i in 0..chars.len() {
        result.push(chars[i]);
        if i < chars.len() - 1 {
            let c1 = chars[i];
            let c2 = chars[i + 1];
            if (c1.is_ascii_digit() && (c2.is_alphabetic() || c2 == '(')) ||
               (c1.is_alphabetic() && c2 == '(') ||
               (c1 == ')' && (c2.is_ascii_digit() || c2.is_alphabetic() || c2 == '(')) {
                result.push('*');
            }
        }
    }
    result
}

fn main() {
    let s = "Lorenz[10, 28, 2.66]";
    let text = &insert_implicit_multiplication(s);
    println!("after implicit: {}", text);
    
    let text = text.trim();
    if let Some(open) = text.find('[') {
        if let Some(close) = text.rfind(']') {
            if close > open {
                let command = text[..open].trim().to_string();
                let inside = &text[open+1..close];
                println!("cmd: '{}', inside: '{}'", command, inside);
            }
        }
    }
}
