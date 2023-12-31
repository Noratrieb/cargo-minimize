use std::collections::HashMap;

use anyhow::Context;
use genemichaels::FormatConfig;

pub fn format(file: syn::File) -> anyhow::Result<String> {
    Ok(
        genemichaels::format_ast(file, &FormatConfig::default(), HashMap::new())
            .context("formatting source file")?
            .rendered,
    )
}
