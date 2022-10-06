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
use std::{collections::BTreeSet, fmt::Debug, ops::Not, path::Path, process::Command};
use syn::{visit_mut::VisitMut, File, Item, ItemMod, Visibility};

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

struct Crate {
    name: String,
    file: syn::File,
    deps: Vec<String>,
}

impl Debug for Crate {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Crate")
            .field("name", &self.name)
            .field("deps", &self.deps)
            .finish()
    }
}

impl PartialEq for Crate {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name
    }
}

impl Eq for Crate {}

impl PartialOrd for Crate {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        self.name.partial_cmp(&other.name)
    }
}

impl Ord for Crate {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.name.cmp(&other.name)
    }
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

    fn crates(&self, unit: &Unit, set: &mut BTreeSet<Crate>) -> Result<()> {
        let ast = cargo_expand(unit.target.src_path()).context("expanding unit")?;

        let deps = self
            .bcx
            .unit_graph
            .get(unit)
            .context("dependencies not found for crate")?;

        let dep_names = deps
            .iter()
            .map(|dep| dep.unit.target.crate_name())
            .collect();

        let krate = Crate {
            file: ast,
            name: unit.target.crate_name(),
            deps: dep_names,
        };

        set.insert(krate);

        for dep in deps {
            self.crates(&dep.unit, set)?;
        }

        Ok(())
    }

    fn expand(&self) -> Result<File> {
        let unit = self.bcx.roots.get(0).context("root unit not found")?;

        let mut crates = BTreeSet::new();
        self.crates(unit, &mut crates).context("get crate list")?;
        println!("{crates:?}");

        self.expand_recursively(unit)
            .context(format!("expanding {} crate", unit.target.crate_name()))
    }

    fn expand_recursively(&self, unit: &Unit) -> Result<File> {
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

            let mut module = ItemMod {
                attrs: file.attrs,
                vis: syn::Visibility::Inherited,
                mod_token: Default::default(),
                ident: name,
                content: Some((Default::default(), file.items)),
                semi: None,
            };

            clean_dep_mod(&mut module);

            ast.items.push(syn::Item::Mod(module));
        }

        Ok(ast)
    }
}

/// Expands the crate in `cargo_dir` into a single file without dependencies
pub fn expand(cargo_dir: &Path) -> Result<File> {
    let cargo_dir = cargo_dir.canonicalize().context("could not find path")?;
    let manifest_path = cargo_dir.join("Cargo.toml");

    let cfg = Config::default().context("create cargo config")?;
    let ws = Workspace::new(&manifest_path, &cfg).context("getting workspace")?;
    let interner = UnitInterner::new();
    let options = CompileOptions::new(&cfg, CompileMode::Build).context("create options")?;
    let bcx = ops::create_bcx(&ws, &options, &interner).context("resolve dep graph")?;

    let expander = DepExpander { bcx };

    let mut root = expander.expand()?;

    clean_final_code(&mut root);

    Ok(root)
}

fn clean_dep_mod(module: &mut ItemMod) {
    module
        .content
        .as_mut()
        .unwrap()
        .1
        .retain(|item| !matches!(item, Item::ExternCrate(_)));

    module.attrs.retain(
        |attr| match attr.path.segments[0].ident.to_string().as_ref() {
            "no_std" | "feature" => false,
            _ => true,
        },
    )
}

fn clean_final_code(file: &mut File) {
    MakePubCrateVisitor.visit_file_mut(file)
}

struct MakePubCrateVisitor;

impl VisitMut for MakePubCrateVisitor {
    fn visit_visibility_mut(&mut self, vis: &mut Visibility) {
        if let Visibility::Public(_) = vis {
            let pub_crate = syn::parse2(quote::quote! { pub(crate) }).unwrap();

            *vis = pub_crate;
        }
    }
}
