#[derive(Debug)]
pub struct CasCmd {
    pub command: String,
    pub args: Vec<String>,
}
pub fn split_args(s: &str) -> Vec<String> {
    let mut args = Vec::new();
    let mut depth = 0;
    let mut start = 0;
    for (i, ch) in s.char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => depth -= 1,
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
pub fn parse_cas_command(text: &str) -> Option<CasCmd> {
    let text = text.trim();
    if let Some(open) = text.find('[') {
        let close = text.rfind(']')?;
        let command = text[..open].trim().to_string();
        let inside = &text[open+1..close];
        let args: Vec<String> = split_args(inside).into_iter().map(|s| s.trim().to_string()).collect();
        if command.is_empty() { return None; }
        let normalized = match command.to_lowercase().as_str() {
            "lorenz" => "Lorenz",
            _ => command.as_str(),
        };
        return Some(CasCmd {
            command: normalized.to_string(),
            args,
        });
    }
    None
}
fn main() {
    let cmd = parse_cas_command("Lorenz[10, 28, 2.66]");
    println!("{:?}", cmd);
    
    if let Some(cmd) = cmd {
        let params: Vec<f64> = cmd.args.iter().filter_map(|s| s.trim().parse().ok()).collect();
        println!("params: {:?}", params);
    }
}
