// this code is pretty neat i guess but i dont have a use for it right now
#![allow(dead_code)]

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
use syn::{visit_mut::VisitMut, File, Item, ItemExternCrate, ItemMod, ItemUse, Visibility};

fn cargo_expand(cargo_dir: &TargetSourcePath) -> Result<syn::File> {
    let cargo_dir = cargo_dir
        .path()
        .context("target path is not a path")?
        .parent()
        .context("target path has no parent")?;

    let mut cmd = Command::new("cargo");

    cmd.current_dir(cargo_dir).arg("expand");

    if let Some(lib) = std::env::args().nth(2) {
        if lib == "lib" {
            cmd.arg("--lib");
        }
    }

    let output = cmd.output().context(format!(
        "spawning cargo with target path {}",
        cargo_dir.display()
    ))?;

    if output.status.success().not() {
        bail!(String::from_utf8(output.stderr).context("stderr utf8")?);
    }

    let src = String::from_utf8(output.stdout).context("stdout utf8")?;

    let root = match syn::parse_str(&src) {
        Ok(root) => root,
        Err(err) => {
            let name = "invalid.rs";

            std::fs::write(name, src).context("write debug file")?;
            Err(err).context(format!(
                "failed to parse, debug file with the contents at `{name}`"
            ))?;
            unreachable!()
        }
    };

    Ok(root)
}

struct Crate {
    name: String,
    ast: syn::File,
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

    fn dep_crates(&self, unit: &Unit, set: &mut BTreeSet<Crate>) -> Result<()> {
        let krate = self.crates(unit, set)?;
        set.insert(krate);

        Ok(())
    }

    /// Adds all dependencies to `set` and returns itself
    fn crates(&self, unit: &Unit, set: &mut BTreeSet<Crate>) -> Result<Crate> {
        let name = unit.target.crate_name();

        let ast =
            cargo_expand(unit.target.src_path()).context(format!("expanding crate `{name}`"))?;

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
            ast,
            name,
            deps: dep_names,
        };

        for dep in deps {
            self.dep_crates(&dep.unit, set)?;
        }

        Ok(krate)
    }

    fn expand(&self) -> Result<File> {
        let unit = self.bcx.roots.get(0).context("root unit not found")?;

        let mut crates = BTreeSet::new();
        let mut root = self.crates(unit, &mut crates).context("get crate list")?;

        for krate in crates {
            self.expand_crate(krate, &mut root.ast);
        }

        Ok(root.ast)
    }

    fn expand_crate(&self, krate: Crate, root: &mut syn::File) {
        let crate_name = krate.name;
        let file = krate.ast;
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

        root.items.push(syn::Item::Mod(module));
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
    let items = &mut module.content.as_mut().unwrap().1;

    items.retain(|item| !matches!(item, Item::ExternCrate(_)));
    clean_items_general(items);

    module.attrs.retain(
        |attr| match attr.path.segments[0].ident.to_string().as_ref() {
            "no_std" | "feature" => false,
            _ => true,
        },
    );
}

fn clean_final_code(file: &mut File) {
    clean_items_general(&mut file.items);

    MakePubCrateVisitor.visit_file_mut(file);
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

fn clean_items_general(items: &mut Vec<Item>) {
    items.retain(|item| match item {
        Item::ExternCrate(ItemExternCrate { ident, .. }) if *ident == "std" => false,
        Item::Use(ItemUse { attrs, .. }) => attrs
            .get(0)
            .map(|attr| attr.path.segments[0].ident != "prelude_import")
            .unwrap_or(true),
        _ => true,
    })
}
