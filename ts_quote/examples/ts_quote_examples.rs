use ts_quote::{ts_quote, TSSource, TS};

fn main() -> anyhow::Result<()> {
    // Repetition produces a valid parsed TypeScript source
    let variants = vec!["Red", "Green", "Blue"];
    let ts: TS = ts_quote! { type Color = #(#variants) | * }?;
    println!("{}", ts.formatted(None)?);

    // Struct-like type from field pairs
    let fields = vec![("x", "number"), ("y", "number")];
    let field_strs: Vec<String> = fields.iter().map(|(k, v)| format!("{k}: {v}")).collect();
    let ts: TS = ts_quote! { type Point = { #(#field_strs)"\n  "* } }?;
    println!("{}", ts.formatted(None)?);

    // Literal # for private fields
    let ts: TS = ts_quote! { class User { ##name: string; } }?;
    println!("{}", ts.formatted(None)?);

    Ok(())
}
