// we require access to many rustc internals
#![feature(rustc_private)]
#![feature(box_patterns, slice_patterns, advanced_slice_patterns, dotdot_in_tuple_patterns)]
#![feature(question_mark)]
#![feature(conservative_impl_trait)]

extern crate itertools;
#[macro_use]
extern crate lazy_static;
extern crate petgraph;
extern crate regex;
extern crate toml;

#[macro_use]
extern crate rustc;
extern crate rustc_const_eval;
extern crate rustc_const_math;
extern crate rustc_data_structures;
extern crate rustc_driver;
extern crate rustc_errors;
extern crate rustc_metadata;
extern crate rustc_mir;
extern crate rustc_typeck;
extern crate syntax;

macro_rules! throw { ($($arg: tt)*) => { return Err(format!($($arg)*)) } }
macro_rules! try_iter { ($arg: expr) => { $arg.collect::<Result<Vec<_>, _>>()?.into_iter() } }

mod item_path;
mod joins;
mod mir_graph;
mod trans;

use std::collections::HashSet;
use std::io;
use std::io::prelude::*;
use std::iter;
use std::fs::File;
use std::path;

use itertools::Itertools;
use petgraph::algo::*;
use regex::Regex;

use syntax::ast::{self, NodeId};
use rustc::hir::{self, intravisit};
use rustc::ty;

use rustc_driver::driver;
use rustc::session;

use trans::krate::{CrateTranspiler, name_def_id};

fn main() {
    // get rust path from rustc itself
    let sysroot = std::process::Command::new("rustc")
        .arg("--print")
        .arg("sysroot")
        .output()
        .unwrap()
        .stdout;
    let sysroot = path::PathBuf::from(String::from_utf8(sysroot).unwrap().trim());

    let input = match std::env::args().collect_vec()[..] {
        [_, ref input] => input.clone(),
        _ => panic!("Expected .rs/.toml/crate name as single cmdline argument"),
    };

    let (crate_name, base, rustc_args, config) = if input.ends_with(".rs") {
        ("test".to_string(), path::PathBuf::from(&input).parent().unwrap().to_owned(), input, toml::Value::Table(toml::Table::new()))
    } else {
        let (crate_name, config_path) = if input.ends_with(".toml") {
            ("test".to_string(), path::PathBuf::from(input))
        } else {
            let config_path = path::Path::new("thys").join(&input).join("config.toml");
            (input, config_path)
        };
        // load TOML config from 'thys/$crate/config.toml'
        let mut config = String::new();
        let mut config_file = File::open(&config_path).expect("error opening crate config");
        config_file.read_to_string(&mut config).unwrap();
        let config: toml::Value = config.parse().unwrap();
        let mut rustc_args = config.lookup("rustc_args").expect("missing config item 'rustc_args'").as_str().unwrap().to_string();
        if rustc_args.starts_with("$RUST_SRC_PATH") {
            rustc_args = rustc_args.replace("$RUST_SRC_PATH", get_rust_src_path().expect("Please run 'rustup component add rust-src' in the electrolysis directory").to_str().unwrap())
        }
        (crate_name, config_path.parent().unwrap().to_owned(), rustc_args, config)
    };

    // parse rustc options
    let rustc_args = iter::once("rustc").chain(rustc_args.split(" ")).map(|s| s.to_string());
    let rustc_matches = rustc_driver::handle_options(&rustc_args.collect_vec()).expect("error parsing rustc args");
    let (mut options, rustc_cfg) = session::config::build_session_options_and_crate_config(&rustc_matches);
    options.crate_name = Some(crate_name);
    options.maybe_sysroot = Some(sysroot);
    options.crate_types = vec![rustc::session::config::CrateType::CrateTypeRlib];
    let input = path::PathBuf::from(rustc_matches.free.iter().join(" "));

    // some more rustc orchestration
    let dep_graph = rustc::dep_graph::DepGraph::new(false);
    let cstore = std::rc::Rc::new(rustc_metadata::cstore::CStore::new(&dep_graph));
    let sess = session::build_session(options, &dep_graph, Some(input.clone()),
        rustc_errors::registry::Registry::new(&rustc::DIAGNOSTICS),
        cstore.clone()
    );
    let rustc_cfg = session::config::build_configuration(&sess, rustc_cfg);

    println!("Compiling up to MIR...");
    let _ = driver::compile_input(&sess, &cstore, rustc_cfg,
        &session::config::Input::File(input),
        &None, &None, None, &driver::CompileController {
            after_analysis: driver::PhaseController {
                stop: rustc_driver::Compilation::Stop,
                callback: Box::new(|state| {
                    transpile_crate(&state, &config, &base).unwrap();
                }),
                run_callback_on_error: false,
            },
            .. driver::CompileController::basic()
        }
    );
}

// from https://github.com/phildawes/racer/blob/c680530f9e9dcd8de5ef0d45954a6112a01a6fe5/src/bin/main.rs#L169-L183
fn get_rust_src_path() -> Option<path::PathBuf> {
    let mut cmd = std::process::Command::new("rustc");
    cmd.arg("--print").arg("sysroot");

    if let Ok(output) = cmd.output() {
        if let Ok(s) = String::from_utf8(output.stdout) {
            let sysroot = path::Path::new(s.trim());
            let srcpath = sysroot.join("lib/rustlib/src/rust/src");
            if srcpath.exists() {
                return Some(srcpath);
            }
        }
    }
    None
}

/// Collects all node IDs of a crate
struct IdCollector<'a, 'tcx : 'a> {
    tcx: ty::TyCtxt<'a, 'tcx, 'tcx>,
    ids: Vec<NodeId>
}

impl<'a, 'tcx> IdCollector<'a, 'tcx> {
    fn skip(&self, attrs: &[ast::Attribute]) -> bool {
        // skip config-dependent definitions except for `#[cfg(not(test))]`
        attrs.iter().any(|attr| attr.check_name("cfg") && !match *attr.meta().meta_item_list().unwrap() {
            [ref arg] => arg.check_name("not") && match *arg.meta_item_list().unwrap() {
                [ref argarg] => argarg.word().map_or(false, |word| word.check_name("test")),
                _ => false,
            },
            _ => false,
        })
    }
}

impl<'a, 'tcx> intravisit::Visitor<'a> for IdCollector<'a, 'tcx> {
    fn visit_item(&mut self, item: &'a hir::Item) {
        if self.skip(&item.attrs) {
            return
        }

        if let hir::Item_::ItemDefaultImpl(_, _) = item.node {
            return // default impls don't seem to be part of the HIR map
        }

        self.ids.push(item.id);
        intravisit::walk_item(self, item);
    }

    fn visit_nested_item(&mut self, id: hir::ItemId) {
        let tcx = self.tcx;
        self.visit_item(tcx.map.expect_item(id.id))
    }

    fn visit_impl_item(&mut self, ii: &'a hir::ImplItem) {
        if self.skip(&ii.attrs) {
            return
        }
        self.ids.push(ii.id);
    }

    fn visit_trait_item(&mut self, trait_item: &'a hir::TraitItem) {
        if self.skip(&trait_item.attrs) {
            return
        }
        self.ids.push(trait_item.id);
    }
}

fn toml_value_as_str_array(val: &::toml::Value) -> Vec<&str> {
    val.as_slice().unwrap().iter().map(|t| t.as_str().unwrap()).collect()
}

fn transpile_crate(state: &driver::CompileState, config: &toml::Value, base: &path::Path) -> io::Result<()> {
    let tcx = state.tcx.unwrap();
    let crate_name = state.crate_name.unwrap();

    let mut trans = CrateTranspiler::new(tcx, &state.mir_map.unwrap(), config);
    println!("Transpiling...");

    let targets = config.lookup("targets").map(|targets| {
        Regex::new(&format!("^({})$", toml_value_as_str_array(targets).into_iter().join("|"))).unwrap()
    });

    // find targets' DefIds and transpile them
    let mut id_collector = IdCollector { tcx: tcx, ids: vec![] };
    intravisit::walk_crate(&mut id_collector, state.hir_crate.unwrap());
    for id in id_collector.ids {
        let def_id = tcx.map.local_def_id(id);
        let name = name_def_id(tcx, def_id);
        if targets.iter().all(|targets| targets.is_match(&*name)) {
            trans.transpile(def_id, targets.is_none());
        }
    }

    let (trans_results, trans::krate::Deps { mut crate_deps, graph, .. }) = trans.destruct();

    // write out theory header, importing dependencies and the pre file, if existent

    if crate_name != "core" {
        crate_deps.insert("core".to_string()); // always include prelude
    }
    let mut crate_deps = crate_deps.into_iter().map(|c| format!("{}.generated", c)).collect_vec();
    crate_deps.sort();
    let has_pre = base.join("pre.lean").exists();
    if has_pre {
        crate_deps.insert(0, format!("{}.pre", crate_name));
    }

    let mut f = try!(File::create(base.join("generated.lean")));
    for dep in crate_deps {
        try!(write!(f, "import {}\n", dep));
    }
    try!(write!(f, "
noncomputable theory

open bool
open [class] classical
open [notation] function
open [class] int
open [notation] list
open [class] nat
open [notation] prod.ops
open [notation] unit
"));
    if has_pre {
        try!(write!(f, "open {}\n", crate_name));
    }
    try!(write!(f, "\n"));

    // condensate sets of cyclic dependencies into graph nodes
    let condensed = condensation(graph, /* make_acyclic */ true);

    // write out each cyclic set, in dependencies-first order
    let mut failed = HashSet::new();
    for idx in toposort(&condensed) {
        match condensed[idx][..] {
            // a singleton set, meaning no cyclic dependencies!
            [def_id] => {
                let name = name_def_id(tcx, def_id);

                // don't even bother writing out code that will fail because of missing dependencies
                let failed_deps = condensed.neighbors_directed(idx, petgraph::EdgeDirection::Incoming).filter(|idx| failed.contains(idx)).collect_vec();
                if failed_deps.is_empty() {
                    match trans_results[&def_id] {
                        Ok(Some(ref trans)) => {
                            try!(write!(f, "{}\n\n", trans));
                        }
                        Ok(None) => {}
                        Err(ref err) => {
                            failed.insert(idx);
                            try!(write!(f, "/- {}: {} -/\n\n", name, err.replace("/-", "/ -")))
                        }
                    }
                } else {
                    failed.insert(idx);
                    try!(write!(f, "/- {}: failed dependencies {} -/\n\n", name, failed_deps.into_iter().flat_map(|idx| &condensed[idx]).map(|&def_id| {
                        name_def_id(tcx, def_id)
                    }).join(", ")));
                }
            }

            // cyclic dependencies, oh my
            ref component => {
                let succeeded = component.iter().filter_map(|def_id| trans_results.get(def_id).and_then(|trans| trans.as_ref().ok())).collect_vec();
                if succeeded.len() == component.len() {
                    if succeeded.iter().all(|trans| trans.as_ref().unwrap().starts_with("inductive")) {
                        // hackishly turn ["inductive A...", "inductive B..."] into "inductive A... with B..."
                        try!(write!(f, "inductive {}\n\n", succeeded.iter().map(|trans| trans.as_ref().unwrap().trim_left_matches("inductive")).join("\n\nwith")));
                        continue;
                    }
                }
                failed.insert(idx);
                try!(write!(f, "/- unimplemented: circular dependencies: {}\n\n", component.iter().map(|&def_id| {
                    name_def_id(tcx, def_id)
                }).join(", ")));
                for &def_id in component {
                    let name = name_def_id(tcx, def_id);
                    match trans_results[&def_id] {
                        Ok(Some(ref trans)) => {
                            try!(write!(f, "{}\n\n", trans));
                        }
                        Err(ref err) => {
                            try!(write!(f, "{}: {}\n\n", name, err.replace("/-", "/ -")))
                        }
                        _ => {}
                    }
                }
                try!(write!(f, "-/\n\n"))
            }
        }
    }

    Ok(())
}
