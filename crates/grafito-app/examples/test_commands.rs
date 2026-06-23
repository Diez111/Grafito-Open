fn main() {
    let text = "y > abs(x)";
    let parts = text.split_once('>');
    println!("{:?}", parts);

    let text2 = "x^2+y^2=9";
    let parts2 = text2.split_once('=');
    println!("{:?}", parts2);
}
