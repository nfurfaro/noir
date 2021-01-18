
use std::{collections::HashMap};

use crate::{Ident, Path, PathKind, hir::crate_graph::CrateId};

use crate::hir::{crate_def_map::{CrateDefMap, LocalModuleId, ModuleDefId, PerNs}};

#[derive(Debug)]
pub struct ImportDirective {
    pub module_id : LocalModuleId,
    pub path : Path,
    pub alias : Option<Ident>,
}

#[derive(Debug)]
pub enum PathResolution {
    Resolved(PerNs),
    Unresolved(String),
}
#[derive(Debug)]
pub struct ResolvedImport {
    // name of the namespace, either last path segment or an alias
    pub name : String,
    // The symbol which we have resolved to 
    pub resolved_namespace : PerNs,
    // The module which we must add the resolved namespace to 
    pub module_scope : LocalModuleId,
}


pub fn resolve_imports(crate_id : CrateId, imports_to_resolve : Vec<ImportDirective>, def_maps : &HashMap<CrateId, CrateDefMap>) -> (Vec<ImportDirective>, Vec<ResolvedImport>){
    let num_imports = imports_to_resolve.len();

    let def_map = &def_maps[&crate_id];

    let mut unresolved : Vec<ImportDirective> = Vec::new();
    let mut resolved : Vec<ResolvedImport> = Vec::new();
    for import_directive in imports_to_resolve {

        let defs = resolve_path_to_ns(&import_directive, def_map, def_maps);

        // Once we have the Option<defs>
        // resolve name and push into appropriate vector
        match defs {
            PathResolution::Unresolved(_) => {
                unresolved.push(import_directive);
            },
            PathResolution::Resolved(resolved_namespace) => {
                let name = resolve_path_name(&import_directive);
                let res = ResolvedImport {name, resolved_namespace,module_scope : import_directive.module_id};
                resolved.push(res);
            }
        };
    }

    assert!(unresolved.len() + resolved.len() == num_imports);

    (unresolved, resolved)

    }

pub fn resolve_path_to_ns(import_directive: &ImportDirective, def_map : &CrateDefMap, def_maps : &HashMap<CrateId, CrateDefMap> ) -> PathResolution {
    let import_path = &import_directive.path.segments;

    match import_directive.path.kind {
        crate::ast::PathKind::Crate => {
            // Resolve from the root of the crate
            let path = &import_path[1..]; // We do not need the `crate` segment. XXX: Maybe we can get rid of this in the Parser? 
            resolve_path_from_crate_root(def_map, path, def_maps)
        },
        crate::ast::PathKind::Dep => resolve_external_dep(def_map, &import_directive, def_maps),
        crate::ast::PathKind::Plain => {
            // Plain paths are only used to import children modules. It's possible to allow import of external deps, but maybe this distinction is better?
            // In Rust they can also point to external Dependencies, if no children can be found with the specified name
            resolve_name_in_module(def_map,import_path, import_directive.module_id, def_maps)
        },
    }

}

fn resolve_path_from_crate_root(def_map : &CrateDefMap, import_path : &[Ident], def_maps : &HashMap<CrateId, CrateDefMap>) -> PathResolution {
    resolve_name_in_module(def_map, import_path, def_map.root, def_maps)
}

fn resolve_name_in_module(def_map : &CrateDefMap, import_path : &[Ident], starting_mod: LocalModuleId, def_maps : &HashMap<CrateId, CrateDefMap>) -> PathResolution {
    let mut current_mod = &def_map.modules[starting_mod.0];

    let mut import_path = import_path.into_iter();
   
    let mut current_ns = match import_path.next() {
        Some(segment) => current_mod.scope.find_name(&segment.0.contents),
        None => return PathResolution::Unresolved(String::from("ice: could not fetch first segment"))
    };

    for segment in import_path {
        
        let typ = match current_ns.take_types() {
            None => return PathResolution::Unresolved(segment.0.contents.clone()),
            Some(typ) => typ,
        };
        
        // In the type namespace, only Mod can be used in a path. Moreover only Mod is in this namespace for now
        let new_module_id = match typ {
            ModuleDefId::ModuleId(id) => id,
            ModuleDefId::FunctionId(_) => unreachable!("functions cannot be in the type namespace"),
        };
        current_mod = &def_maps[&new_module_id.krate].modules[new_module_id.local_id.0];
               
        // Check if namespace
        let found_ns = current_mod.scope.find_name(&segment.0.contents);
        if found_ns.is_none() {
            return PathResolution::Unresolved(segment.0.contents.clone());
        }
        current_ns = found_ns 
    }

    PathResolution::Resolved(current_ns)
}

fn resolve_path_name(import_directive: &ImportDirective ) -> String {
    match &import_directive.alias {
        None => import_directive.path.segments.last().unwrap().0.contents.clone(),
        Some(ident) => ident.0.contents.clone(),
    }
}

fn resolve_external_dep(current_def_map : &CrateDefMap, directive : &ImportDirective, def_maps : &HashMap<CrateId, CrateDefMap>) -> PathResolution {
    
    // Use extern_prelude to get the dep
    //
    // Remove the first segment, it is the "dep" symbol
    //
    let path = &directive.path.segments;
    let path_without_dep_seg = &path[1..];
    
    // Fetch the root module from the prelude
    let crate_name = path_without_dep_seg.first().unwrap().0.contents.clone();
    let dep_module = current_def_map.extern_prelude.get(&crate_name).expect("error reporter: could not find crate");
    
    // Create an import directive for the dependency crate
    let path_without_crate_name = &path[2..]; // XXX: This will panic if the path is of the form `use dep::std` Ideal algorithm will not distinguish between crate and module
    let path = Path{ segments: path_without_crate_name.to_vec(), kind: PathKind::Plain};
    let dep_directive = ImportDirective { module_id: directive.module_id, path: path, alias: directive.alias.clone()};
    
    let dep_def_map = def_maps.get(&dep_module.krate).unwrap();

    resolve_path_to_ns(&dep_directive, dep_def_map, def_maps)
}