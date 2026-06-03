use ts_quote::ts_string;

fn main() {
    // Simple substitution
    let val = 23;
    let s = ts_string! { let foo: number = #val; };
    println!("{s}");

    // Repetition — no separator
    let variants = vec!["Red", "Green", "Blue"];
    let s = ts_string! { type Color = #(#variants)* };
    println!("{s}");

    // Repetition — single punct separator
    let variants = vec!["Red", "Green", "Blue"];
    let s = ts_string! { type Color = #(#variants) | * };
    println!("{s}");

    // Repetition — string literal separator (newline)
    let fields = vec!["x: number", "y: number", "z: number"];
    let s = ts_string! { type Point = { #(#fields)",\n  "* }; };
    println!("{s}");

    // Literal # escape for TypeScript private fields
    let s = ts_string! { class User { ##name: string; } };
    println!("{s}");

    // Doc comments are preserved (use `///` / `//!` / `/** */`, not `//` —
    // plain `//` and `/* */` comments are stripped by the compiler before the
    // macro ever sees them).
    // Default -> TS line comment. Prefix the text with `doc` -> TS JSDoc block.
    let s = ts_string! {
        //should not see this one
        ///doc A 2D point
        type Point = {
            /// the horizontal coordinate
            x: number;
            /**doc vertical
             * coordinate */
            y: number;
        };
    };
    println!("{s}");
}
