pub use ts_quote_macros::ts_quote;
pub use ts_quote_macros::ts_string;

#[cfg(feature = "deno_ast")]
use deno_ast::{parse_module, Diagnostic, SourceTextInfo};
#[cfg(feature = "deno_ast")]
use dprint_plugin_typescript::{
    configuration::{Configuration, ConfigurationBuilder, NextControlFlowPosition, QuoteStyle},
    format_parsed_source,
};

#[cfg(feature = "deno_ast")]
pub use deno_ast::ParsedSource as TS;

/**
The TSSource trait is used to add a few convenience methods to the  deno_ast::ParsedSource type.
**/
#[cfg(feature = "deno_ast")]
pub trait TSSource: Sized {
    /**
    Creates a ParsedSource instance from a string.

    # Arguments:

    * `source` - A TypeScript source string

    # Returns

    Returns a ParsedSource, or an error diagnostic if source is not valid TypeScript
    **/
    fn from_source(source: String) -> Result<Self, Diagnostic>;

    /**
    Returns a formatted TypeScript string.

    # Arguments:

    * `config` - Optional: a `dprint_plugin_typescript` config used for formatting the output.

    If no config is provided, the function will output using the default config:
    - `line_width`: `80`
    - `indent_width`: `2`
    - `prefer_hanging`: `true`
    - `prefer_single_line`: `false`
    - `quote_style`: `QuoteStyle::PreferSingle`
    - `next_control_flow_position`: `NextControlFlowPosition::SameLine`

    # Returns

    Returns a ParsedSource, or an error diagnostic if source is not valid TypeScript
    **/
    fn formatted(&self, config: Option<&Configuration>) -> anyhow::Result<String>;
}

#[cfg(feature = "deno_ast")]
impl TSSource for TS {
    fn from_source(source: String) -> Result<Self, Diagnostic> {
        parse_module(deno_ast::ParseParams {
            specifier: "".to_string(),
            text_info: SourceTextInfo::from_string(source),
            media_type: deno_ast::MediaType::TypeScript,
            capture_tokens: true,
            scope_analysis: false,
            maybe_syntax: None,
        })
    }

    fn formatted(&self, config: Option<&Configuration>) -> anyhow::Result<String> {
        match config {
            Some(config) => Ok(format_parsed_source(self, config)?.unwrap_or(String::new())),
            None => {
                let config = ConfigurationBuilder::new()
                    .indent_width(2)
                    .line_width(80)
                    .prefer_hanging(true)
                    .prefer_single_line(false)
                    .quote_style(QuoteStyle::PreferSingle)
                    .next_control_flow_position(NextControlFlowPosition::SameLine)
                    .build();

                Ok(format_parsed_source(self, &config)?.unwrap_or(String::new()))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(feature = "deno_ast")]
    #[test]
    fn test_format_source_from_string() -> anyhow::Result<()> {
        let ts: TS = TS::from_source("let a = 1; let b = 2;".to_string())?;

        let output = ts.formatted(None)?;

        println!("output:");
        println!("{}", output);

        assert_eq!(output.as_str(), "let a = 1;\nlet b = 2;\n");

        Ok(())
    }

    #[test]
    fn test_repetition_no_separator() {
        let items = vec!["A", "B", "C"];
        let s = ts_string! { type T = #(#items)* };
        assert_eq!(s.trim(), "type T = ABC");
    }

    #[test]
    fn test_repetition_punct_separator() {
        let items = vec!["A", "B", "C"];
        let s = ts_string! { type T = #(#items) | * };
        assert_eq!(s.trim(), "type T = A|B|C");
    }

    #[test]
    fn test_repetition_string_literal_separator() {
        let items = vec!["x: number", "y: number"];
        let s = ts_string! { type P = { #(#items)"\n"* } };
        assert!(s.contains("x: number\ny: number"));
    }

    #[test]
    fn test_repetition_empty_iterator() {
        let items: Vec<&str> = vec![];
        let s = ts_string! { type T = #(#items),* };
        assert!(!s.contains(','));
    }

    #[test]
    fn test_repetition_comma_separator() {
        let items = vec!["a", "b", "c"];
        let s = ts_string! { const arr = [#(#items),*] };
        assert!(s.contains("a,b,c"));
    }

    #[test]
    fn test_literal_hash_escape() {
        let s = ts_string! { class User { ##name: string; } };
        assert!(s.contains("#name"));
        // Ensure there is no double-# in the output
        assert!(!s.contains("##"));
    }

    #[test]
    fn test_literal_hash_escape_standalone() {
        let s = ts_string! { ##field };
        assert!(s.contains("#field"));
    }
}
