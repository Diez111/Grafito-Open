fn split_args(s: &str) -> Vec<String> {
    let mut args = Vec::new();
    let mut depth = 0;
    let mut start = 0;
    for (i, ch) in s.char_indices() {
        match ch {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth -= 1,
            ',' if depth == 0 => {
                args.push(s[start..i].to_string());
                start = i + 1;
            }
            _ => {}
        }
    }
    args.push(s[start..].to_string());
    args
}

fn main() {
    let inside = "10, 28, 2.66";
    let args: Vec<String> = split_args(inside).into_iter().map(|s| s.trim().to_string()).collect();
    let params: Vec<f64> = args.iter().filter_map(|s| s.trim().parse().ok()).collect();
    println!("args: {:?}", args);
    println!("params: {:?}", params);
}
