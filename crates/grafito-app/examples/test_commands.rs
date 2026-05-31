use std::collections::HashMap;
fn main() {
    let text = "y > abs(x)";
    let mut parts = text.split_once('>');
    println!("{:?}", parts);
    
    let text2 = "x^2+y^2=9";
    let mut parts2 = text2.split_once('=');
    println!("{:?}", parts2);
}
