use ts_quote::ts_string;
fn main() {
    let val = 23;
    let s: String = ts_string! {
        let foo: number = #val;
    };
    println!("got string \n{}", s);
}
