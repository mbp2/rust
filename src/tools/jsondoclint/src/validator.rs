use std::collections::HashSet;
use std::hash::Hash;

use rustdoc_json_types::{
    Constant, Crate, DynTrait, Enum, FnDecl, Function, FunctionPointer, GenericArg, GenericArgs,
    GenericBound, GenericParamDef, Generics, Id, Impl, Import, ItemEnum, Method, Module, OpaqueTy,
    Path, ProcMacro, Static, Struct, StructKind, Term, Trait, TraitAlias, Type, TypeBinding,
    TypeBindingKind, Typedef, Union, Variant, WherePredicate,
};

use crate::{item_kind::can_appear_in_mod, Error};

#[derive(Debug)]
pub struct Validator<'a> {
    pub(crate) errs: Vec<Error>,
    krate: &'a Crate,
    seen_ids: HashSet<&'a Id>,
    todo: HashSet<&'a Id>,
}

fn set_remove<T: Hash + Eq + Clone>(set: &mut HashSet<T>) -> Option<T> {
    if let Some(id) = set.iter().next() {
        let id = id.clone();
        set.take(&id)
    } else {
        None
    }
}

impl<'a> Validator<'a> {
    pub fn new(krate: &'a Crate) -> Self {
        Self { krate, errs: Vec::new(), seen_ids: HashSet::new(), todo: HashSet::new() }
    }

    pub fn check_crate(&mut self) {
        let root = &self.krate.root;
        self.add_mod_id(root);
        while let Some(id) = set_remove(&mut self.todo) {
            self.seen_ids.insert(id);
            self.check_item(id);
        }
    }

    fn check_item(&mut self, id: &'a Id) {
        let item = &self.krate.index[id];
        match &item.inner {
            ItemEnum::Import(x) => self.check_import(x),
            ItemEnum::Union(x) => self.check_union(x),
            ItemEnum::Struct(x) => self.check_struct(x),
            ItemEnum::StructField(x) => self.check_struct_field(x),
            ItemEnum::Enum(x) => self.check_enum(x),
            ItemEnum::Variant(x) => self.check_variant(x),
            ItemEnum::Function(x) => self.check_function(x),
            ItemEnum::Trait(x) => self.check_trait(x),
            ItemEnum::TraitAlias(x) => self.check_trait_alias(x),
            ItemEnum::Method(x) => self.check_method(x),
            ItemEnum::Impl(x) => self.check_impl(x),
            ItemEnum::Typedef(x) => self.check_typedef(x),
            ItemEnum::OpaqueTy(x) => self.check_opaque_ty(x),
            ItemEnum::Constant(x) => self.check_constant(x),
            ItemEnum::Static(x) => self.check_static(x),
            ItemEnum::ForeignType => todo!(),
            ItemEnum::Macro(x) => self.check_macro(x),
            ItemEnum::ProcMacro(x) => self.check_proc_macro(x),
            ItemEnum::PrimitiveType(x) => self.check_primitive_type(x),
            ItemEnum::Module(x) => self.check_module(x),

            ItemEnum::ExternCrate { .. } => todo!(),
            ItemEnum::AssocConst { .. } => todo!(),
            ItemEnum::AssocType { .. } => todo!(),
        }
    }

    // Core checkers
    fn check_module(&mut self, module: &'a Module) {
        module.items.iter().for_each(|i| self.add_mod_item_id(i));
    }

    fn check_import(&mut self, x: &'a Import) {
        if x.glob {
            self.add_mod_id(x.id.as_ref().unwrap());
        } else if let Some(id) = &x.id {
            self.add_mod_item_id(id);
        }
    }

    fn check_union(&mut self, x: &'a Union) {
        self.check_generics(&x.generics);
        x.fields.iter().for_each(|i| self.add_field_id(i));
        x.impls.iter().for_each(|i| self.add_impl_id(i));
    }

    fn check_struct(&mut self, x: &'a Struct) {
        self.check_generics(&x.generics);
        match &x.kind {
            StructKind::Unit => {}
            StructKind::Tuple(fields) => fields.iter().flatten().for_each(|f| self.add_field_id(f)),
            StructKind::Plain { fields, fields_stripped: _ } => {
                fields.iter().for_each(|f| self.add_field_id(f))
            }
        }
        x.impls.iter().for_each(|i| self.add_impl_id(i));
    }

    fn check_struct_field(&mut self, x: &'a Type) {
        self.check_type(x);
    }

    fn check_enum(&mut self, x: &'a Enum) {
        self.check_generics(&x.generics);
        x.variants.iter().for_each(|i| self.add_variant_id(i));
        x.impls.iter().for_each(|i| self.add_impl_id(i));
    }

    fn check_variant(&mut self, x: &'a Variant) {
        match x {
            Variant::Plain(_discriminant) => {} // TODO: Check discriminant value parses
            Variant::Tuple(tys) => tys.iter().flatten().for_each(|t| self.add_field_id(t)),
            Variant::Struct { fields, fields_stripped: _ } => {
                fields.iter().for_each(|f| self.add_field_id(f))
            }
        }
    }

    fn check_function(&mut self, x: &'a Function) {
        self.check_generics(&x.generics);
        self.check_fn_decl(&x.decl);
    }

    fn check_trait(&mut self, x: &'a Trait) {
        self.check_generics(&x.generics);
        x.items.iter().for_each(|i| self.add_trait_item_id(i));
        x.bounds.iter().for_each(|i| self.check_generic_bound(i));
        x.implementations.iter().for_each(|i| self.add_impl_id(i));
    }

    fn check_trait_alias(&mut self, x: &'a TraitAlias) {
        self.check_generics(&x.generics);
        x.params.iter().for_each(|i| self.check_generic_bound(i));
    }

    fn check_method(&mut self, x: &'a Method) {
        self.check_fn_decl(&x.decl);
        self.check_generics(&x.generics);
    }

    fn check_impl(&mut self, x: &'a Impl) {
        self.check_generics(&x.generics);
        if let Some(path) = &x.trait_ {
            self.check_path(path); // TODO: Check is trait.
        }
        self.check_type(&x.for_);
        x.items.iter().for_each(|i| self.add_trait_item_id(i));
        if let Some(blanket_impl) = &x.blanket_impl {
            self.check_type(blanket_impl)
        }
    }

    fn check_typedef(&mut self, x: &'a Typedef) {
        self.check_generics(&x.generics);
        self.check_type(&x.type_);
    }

    fn check_opaque_ty(&mut self, x: &'a OpaqueTy) {
        x.bounds.iter().for_each(|b| self.check_generic_bound(b));
        self.check_generics(&x.generics);
    }

    fn check_constant(&mut self, x: &'a Constant) {
        self.check_type(&x.type_);
    }

    fn check_static(&mut self, x: &'a Static) {
        self.check_type(&x.type_);
    }

    fn check_macro(&mut self, _: &'a str) {
        // TODO
    }

    fn check_proc_macro(&mut self, _: &'a ProcMacro) {
        // TODO
    }

    fn check_primitive_type(&mut self, _: &'a str) {
        // TODO
    }

    fn check_generics(&mut self, x: &'a Generics) {
        x.params.iter().for_each(|p| self.check_generic_param_def(p));
        x.where_predicates.iter().for_each(|w| self.check_where_predicate(w));
    }

    fn check_type(&mut self, x: &'a Type) {
        match x {
            Type::ResolvedPath(path) => self.check_path(path),
            Type::DynTrait(dyn_trait) => self.check_dyn_trait(dyn_trait),
            Type::Generic(_) => {}
            Type::Primitive(_) => {}
            Type::FunctionPointer(fp) => self.check_function_pointer(&**fp),
            Type::Tuple(tys) => tys.iter().for_each(|ty| self.check_type(ty)),
            Type::Slice(inner) => self.check_type(&**inner),
            Type::Array { type_, len: _ } => self.check_type(&**type_),
            Type::ImplTrait(bounds) => bounds.iter().for_each(|b| self.check_generic_bound(b)),
            Type::Infer => {}
            Type::RawPointer { mutable: _, type_ } => self.check_type(&**type_),
            Type::BorrowedRef { lifetime: _, mutable: _, type_ } => self.check_type(&**type_),
            Type::QualifiedPath { name: _, args, self_type, trait_ } => {
                self.check_generic_args(&**args);
                self.check_type(&**self_type);
                self.check_path(trait_);
            }
        }
    }

    fn check_fn_decl(&mut self, x: &'a FnDecl) {
        x.inputs.iter().for_each(|(_name, ty)| self.check_type(ty));
        if let Some(output) = &x.output {
            self.check_type(output);
        }
    }

    fn check_generic_bound(&mut self, x: &'a GenericBound) {
        match x {
            GenericBound::TraitBound { trait_, generic_params, modifier: _ } => {
                self.check_path(trait_);
                generic_params.iter().for_each(|gpd| self.check_generic_param_def(gpd));
            }
            GenericBound::Outlives(_) => todo!(),
        }
    }

    fn check_path(&mut self, x: &'a Path) {
        self.add_id(&x.id); // TODO: What kinds are allowed here.
        if let Some(args) = &x.args {
            self.check_generic_args(&**args);
        }
    }

    fn check_generic_args(&mut self, x: &'a GenericArgs) {
        match x {
            GenericArgs::AngleBracketed { args, bindings } => {
                args.iter().for_each(|arg| self.check_generic_arg(arg));
                bindings.iter().for_each(|bind| self.check_type_binding(bind));
            }
            GenericArgs::Parenthesized { inputs, output } => {
                inputs.iter().for_each(|ty| self.check_type(ty));
                if let Some(o) = output {
                    self.check_type(o);
                }
            }
        }
    }

    fn check_generic_param_def(&mut self, gpd: &'a GenericParamDef) {
        match &gpd.kind {
            rustdoc_json_types::GenericParamDefKind::Lifetime { outlives: _ } => {}
            rustdoc_json_types::GenericParamDefKind::Type { bounds, default, synthetic: _ } => {
                bounds.iter().for_each(|b| self.check_generic_bound(b));
                if let Some(ty) = default {
                    self.check_type(ty);
                }
            }
            rustdoc_json_types::GenericParamDefKind::Const { type_, default: _ } => {
                self.check_type(type_)
            }
        }
    }

    fn check_generic_arg(&mut self, arg: &'a GenericArg) {
        match arg {
            GenericArg::Lifetime(_) => {}
            GenericArg::Type(ty) => self.check_type(ty),
            GenericArg::Const(c) => self.check_constant(c),
            GenericArg::Infer => {}
        }
    }

    fn check_type_binding(&mut self, bind: &'a TypeBinding) {
        self.check_generic_args(&bind.args);
        match &bind.binding {
            TypeBindingKind::Equality(term) => self.check_term(term),
            TypeBindingKind::Constraint(bounds) => {
                bounds.iter().for_each(|b| self.check_generic_bound(b))
            }
        }
    }

    fn check_term(&mut self, term: &'a Term) {
        match term {
            Term::Type(ty) => self.check_type(ty),
            Term::Constant(con) => self.check_constant(con),
        }
    }

    fn check_where_predicate(&mut self, w: &'a WherePredicate) {
        match w {
            WherePredicate::BoundPredicate { type_, bounds, generic_params } => {
                self.check_type(type_);
                bounds.iter().for_each(|b| self.check_generic_bound(b));
                generic_params.iter().for_each(|gpd| self.check_generic_param_def(gpd));
            }
            WherePredicate::RegionPredicate { lifetime: _, bounds } => {
                bounds.iter().for_each(|b| self.check_generic_bound(b));
            }
            WherePredicate::EqPredicate { lhs, rhs } => {
                self.check_type(lhs);
                self.check_term(rhs);
            }
        }
    }

    fn check_dyn_trait(&mut self, dyn_trait: &'a DynTrait) {
        for pt in &dyn_trait.traits {
            self.check_path(&pt.trait_);
            pt.generic_params.iter().for_each(|gpd| self.check_generic_param_def(gpd));
        }
    }

    fn check_function_pointer(&mut self, fp: &'a FunctionPointer) {
        self.check_fn_decl(&fp.decl);
        fp.generic_params.iter().for_each(|gpd| self.check_generic_param_def(gpd));
    }

    // Aux functions
    fn add_id(&mut self, id: &'a Id) {
        if !self.seen_ids.contains(id) {
            self.todo.insert(id);
        }
    }

    fn add_field_id(&mut self, id: &'a Id) {
        let item = &self.krate.index[id];
        if let ItemEnum::StructField(_) = item.inner {
            self.add_id(id);
        } else {
            self.fail(id, "Expecting field");
        }
    }

    fn add_mod_id(&mut self, id: &'a Id) {
        let item = &self.krate.index[id];
        if let ItemEnum::Module(_) = item.inner {
            self.add_id(id);
        } else {
            self.fail(id, "Expecting module");
        }
    }

    fn add_impl_id(&mut self, id: &'a Id) {
        let item = &self.krate.index[id];
        if let ItemEnum::StructField(_) = item.inner {
            self.add_id(id);
        } else {
            self.fail(id, "Expecting impl");
        }
    }

    fn add_variant_id(&mut self, id: &'a Id) {
        let item = &self.krate.index[id];
        if let ItemEnum::StructField(_) = item.inner {
            self.add_id(id);
        } else {
            self.fail(id, "Expecting variant");
        }
    }

    /// Add an Id that appeared in a trait
    fn add_trait_item_id(&mut self, id: &'a Id) {
        let item = &self.krate.index[id];
        if !can_appear_in_mod(&item.inner) {
            self.fail(id, "Expecting item that can appear in trait");
        } else {
            self.add_id(id);
        }
    }

    /// Add an Id that appeared in a mod
    fn add_mod_item_id(&mut self, id: &'a Id) {
        let item = &self.krate.index[id];
        if can_appear_in_mod(&item.inner) {
            self.add_id(id);
        } else {
            self.fail(id, "Expecting item that can appear in trait");
        }
    }

    fn fail(&mut self, id: &Id, msg: &str) {
        self.errs.push(Error { id: id.clone(), message: msg.to_string() });
    }
}
