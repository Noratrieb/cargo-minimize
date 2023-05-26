use std::collections::HashMap;

use genemichaels::FormatConfig;

pub fn format(file: syn::File) -> anyhow::Result<String> {
    Ok(genemichaels::format_ast(file, &FormatConfig::default(), HashMap::new())?.rendered)
}
