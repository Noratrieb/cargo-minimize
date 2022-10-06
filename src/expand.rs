use anyhow::{bail, Context, Result};
use cargo::{
    core::{
        compiler::{BuildContext, Unit, UnitInterner},
        manifest::TargetSourcePath,
        Workspace,
    },
    ops::{self, CompileOptions},
    util::{command_prelude::CompileMode, Config},
};
use std::{ops::Not, path::Path, process::Command};

fn cargo_expand(cargo_dir: &TargetSourcePath) -> Result<syn::File> {
    let cargo_dir = cargo_dir
        .path()
        .context("target path is not a path")?
        .parent()
        .context("target path has no parent")?;

    let mut cmd = Command::new("cargo");

    cmd.current_dir(cargo_dir).arg("expand");

    let output = cmd.output().context(format!(
        "spawning cargo with target path {}",
        cargo_dir.display()
    ))?;

    if output.status.success().not() {
        bail!(String::from_utf8(output.stderr).context("stderr utf8")?);
    }

    let src = String::from_utf8(output.stdout).context("stdout utf8")?;

    let root = syn::parse_str(&src).context("parsing crate")?;

    Ok(root)
}

struct DepExpander<'ws, 'cfg> {
    bcx: BuildContext<'ws, 'cfg>,
}

impl<'ws, 'cfg> DepExpander<'ws, 'cfg> {
    fn source(unit: &Unit) -> Result<&Path> {
        unit.target
            .src_path()
            .path()
            .context("unit source path not found")
    }

    fn expand(&self) -> Result<syn::File> {
        let unit = self.bcx.roots.get(0).context("root unit not found")?;
        self.expand_recursively(unit)
            .context(format!("expanding {} crate", unit.target.crate_name()))
    }

    fn expand_recursively(&self, unit: &Unit) -> Result<syn::File> {
        let mut ast = cargo_expand(unit.target.src_path()).context("expanding unit")?;

        let deps = self
            .bcx
            .unit_graph
            .get(unit)
            .context("dependencies not found for crate")?;

        for dep in deps {
            let crate_name = dep.unit.target.crate_name();

            let file = self
                .expand_recursively(&dep.unit)
                .context(format!("expanding {crate_name} crate"))?;

            let name = proc_macro2::Ident::new(&crate_name, proc_macro2::Span::call_site());

            let module = syn::ItemMod {
                attrs: file.attrs,
                vis: syn::Visibility::Inherited,
                mod_token: Default::default(),
                ident: name,
                content: Some((Default::default(), file.items)),
                semi: None,
            };

            ast.items.push(syn::Item::Mod(module));
        }

        Ok(ast)
    }
}

/// Expands the crate in `cargo_dir` into a single file without dependencies
pub fn expand(cargo_dir: &Path) -> Result<syn::File> {
    let cargo_dir = cargo_dir.canonicalize().context("could not find path")?;
    let manifest_path = cargo_dir.join("Cargo.toml");

    let cfg = Config::default().context("create cargo config")?;
    let ws = Workspace::new(&manifest_path, &cfg).context("getting workspace")?;
    let interner = UnitInterner::new();
    let options = CompileOptions::new(&cfg, CompileMode::Build).context("create options")?;
    let bcx = ops::create_bcx(&ws, &options, &interner).context("resolve dep graph")?;

    let expander = DepExpander { bcx };

    let root = expander.expand()?;

    Ok(root)
}
