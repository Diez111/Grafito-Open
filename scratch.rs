fn main() {
    let s = "Lorenz[10, 28, 2.66]";
    let args: Vec<&str> = s.split(|c| c == '[' || c == ']' || c == '(' || c == ')').collect();
    println!("{:?}", args);
}
