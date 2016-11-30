use std::collections::{HashMap, HashSet};
use std::iter;
use std::ops::Deref;

use itertools::Itertools;

use syntax::ast::{self, NodeId};
use rustc::hir;
use rustc::hir::def::CtorKind;
use rustc::hir::def_id::DefId;
use rustc::ty::subst::{Subst, Substs};
use rustc::traits::*;
use rustc::ty::{self, Lift, Ty};

use util::*;
use trans::TransResult;
use trans::krate::{self, CrateTranspiler};

pub fn mk_tuple_ty<It: IntoIterator<Item=String>>(it: It) -> String {
    match it.into_iter().collect_vec()[..] {
        [] => "unit".to_string(),
        [ref x] => x.clone(),
        ref xs => format!("({})", xs.into_iter().join(" × "))
    }
}

pub enum TraitImplLookup<'tcx> {
    Static { impl_def_id: DefId, params: Vec<String>, substs: &'tcx Substs<'tcx> },
    Dynamic { param: String },
}

impl<'tcx> TraitImplLookup<'tcx> {
    pub fn to_string<'a>(self, trans: &ItemTranspiler<'a, 'tcx>) -> TransResult {
        Ok(match self {
            TraitImplLookup::Static { impl_def_id, params, substs } =>
                format!("(@{} {})", trans.name_def_id(impl_def_id), try_iter!(substs.types().map(|ty| {
                    trans.transpile_ty(ty)
                })).chain(params).join(" ")),
            TraitImplLookup::Dynamic { param } => param,
        })
    }
}

pub enum LeanTyParam<'tcx> {
    RustTyParam(String),
    AssocTy(String),
    TraitRef(String, String, ty::TraitRef<'tcx>),
}

impl<'tcx> LeanTyParam<'tcx> {
    pub fn to_string(&self) -> String {
        match *self {
            LeanTyParam::RustTyParam(ref name) => format!("{{{} : Type₁}}", name),
            LeanTyParam::AssocTy(ref name) => format!("({} : Type₁)", name),
            LeanTyParam::TraitRef(ref name, ref ty, _) => format!("[{} : {}]", name, ty),
        }
    }

    pub fn name(&self) -> &str {
        match *self {
            LeanTyParam::RustTyParam(ref name) => name,
            LeanTyParam::AssocTy(ref name) => name,
            LeanTyParam::TraitRef(ref name, _, _) => name,
        }
    }
}

pub struct ItemTranspiler<'a, 'tcx: 'a> {
    pub sup: &'a CrateTranspiler<'a, 'tcx>,
    pub def_id: DefId,
}

impl<'a, 'tcx> Deref for ItemTranspiler<'a, 'tcx> {
    type Target = krate::CrateTranspiler<'a, 'tcx>;

    fn deref(&self) -> &krate::CrateTranspiler<'a, 'tcx> {
        self.sup
    }
}

impl<'a, 'tcx> ItemTranspiler<'a, 'tcx> {
    pub fn name(&self) -> String { krate::name_def_id(self.tcx, self.def_id) }
    pub fn node_id(&self) -> NodeId {
        self.tcx.map.as_local_node_id(self.def_id).unwrap()
    }

    pub fn add_dep(&self, def_id: DefId) {
        self.sup.add_dep(def_id, self.def_id)
    }

    pub fn name_def_id(&self, did: DefId) -> String {
        // the primary way of adding dependencies: if some definition textually relies on another one,
        // it will call `name_def_id` at some point
        self.add_dep(did);
        krate::name_def_id(self.tcx, did)
    }

    fn transpile_trait_ref_args(&self, trait_ref: ty::TraitRef<'tcx>) -> TransResult<Vec<String>> {
        trait_ref.substs.types().map(|ty| {
            self.transpile_ty(ty)
        }).collect()
    }

    fn transpile_associated_type(&self, trait_ref: ty::TraitRef, name: &ast::Name) -> String {
        krate::mk_lean_name_from_parts(iter::once(&format!("{:?}.{}", trait_ref, name)))
    }

    /// `Item = u32` ~> `{'Item': 'u32'}`
    pub fn get_assoc_ty_substs(&self, def_id: DefId, substs: &Substs<'tcx>) -> TransResult<HashMap<String, String>> {
        Ok(self.tcx.item_predicates(def_id).predicates.into_iter().try_filter_map(|trait_pred| -> TransResult<_> { Ok(match trait_pred {
            ty::Predicate::Projection(ty::Binder(proj_pred)) => {
                let assoc_ty = self.transpile_associated_type(proj_pred.projection_ty.trait_ref.subst(self.tcx, substs), &proj_pred.projection_ty.item_name);
                Some((assoc_ty, self.transpile_ty(&proj_pred.ty)?))
            }
            _ => None,
        })})?.collect())
    }

    fn trait_ancestors(&self, trait_ref: ty::TraitRef<'tcx>) -> Vec<ty::TraitRef<'tcx>> {
        iter::once(trait_ref).chain(self.trait_predicates_without_markers(trait_ref.def_id).flat_map(|trait_pred| {
            if trait_pred.def_id() != trait_ref.def_id {
                self.trait_ancestors(trait_pred.trait_ref.subst(self.tcx, trait_ref.substs))
            } else { vec![] }
        })).collect_vec()
    }

    // includes associates types from trait ancestors
    fn all_assoc_tys(&self, trait_ref: ty::TraitRef<'tcx>) -> TransResult<Vec<String>> {
        let assoc_substs = self.get_assoc_ty_substs(trait_ref.def_id, trait_ref.substs)?;
        Ok(self.trait_ancestors(trait_ref).into_iter().flat_map(|trait_ref| {
            self.tcx.associated_items(trait_ref.def_id).filter_map(|item| match item.kind {
                ty::AssociatedKind::Type
                    if !assoc_substs.contains_key(&self.transpile_associated_type(trait_ref, &item.name)) =>
                    Some(self.transpile_associated_type(trait_ref, &item.name)),
                _ => None,
            }).collect_vec()
        }).collect_vec())
    }

    /// `Iterator, {}` ~> `(['Item'], ['Item'])`
    /// `Iterator, {'Item': 'u32'}` ~> `(['u32'], [])`
    pub fn transpile_trait_ref_assoc_tys(&self, trait_ref: ty::TraitRef<'tcx>, assoc_ty_substs: &HashMap<String, String>) -> TransResult<(Vec<String>, Vec<String>)> {
        let mut free_assoc_tys = vec![];
        let assoc_tys = self.all_assoc_tys(trait_ref)?.into_iter().flat_map(|assoc_ty| {
            Some(match assoc_ty_substs.get(&assoc_ty) {
                Some(assoc_ty) => assoc_ty.to_owned(),
                _ => {
                    free_assoc_tys.push(assoc_ty.clone());
                    assoc_ty
                }
            })
        }).collect();

        Ok((assoc_tys, free_assoc_tys))
    }

    /// `Add<T, RHS=S>` ~> `'Add T'`
    pub fn transpile_trait_ref_no_assoc_tys(&self, trait_ref: ty::TraitRef<'tcx>) -> TransResult {
        Ok((&self.name_def_id(trait_ref.def_id), self.transpile_trait_ref_args(trait_ref)?).join(" "))
    }

    /// `Add<T>` ~> `'Add T RHS'`
    /// `Add<T, RHS=S>` ~> `'Add T S'`
    pub fn transpile_trait_ref(&self, trait_ref: ty::TraitRef<'tcx>, assoc_ty_substs: &HashMap<String, String>) -> TransResult {
        let associated_types = self.transpile_trait_ref_assoc_tys(trait_ref, assoc_ty_substs)?.0;
        Ok((self.transpile_trait_ref_no_assoc_tys(trait_ref)?, associated_types).join(" "))
    }

    pub fn free_substs_for_item(&self, def_id: DefId) -> &'tcx Substs<'tcx> {
        ty::ParameterEnvironment::for_item(self.tcx, self.tcx.map.as_local_node_id(def_id).unwrap()).free_substs
    }

    /// `T : Iterator` ~> `[(T : Type), (Item : Type), [Iterator : Iterator T Item]]`
    pub fn transpile_ty_params_with_substs(&self, def_id: DefId, substs: &Substs<'tcx>, is_trait: bool) -> TransResult<Vec<LeanTyParam<'tcx>>> {
        if self.tcx.def_key(def_id).disambiguated_data.data == hir::map::definitions::DefPathData::ClosureExpr {
            // closure shares params with outer scope
            return self.transpile_ty_params(self.tcx.parent_def_id(def_id).unwrap())
        }
        let mut parent_params = vec![];
        if let Some(trait_def_id) = self.tcx.trait_of_item(def_id) {
            // typeclass fields already derive the parent params
            if trait_def_id != self.def_id {
                parent_params = self.transpile_ty_params_with_substs(trait_def_id, substs, true)?
            }
        } else if let Some(impl_def_id) = self.tcx.impl_of_method(def_id) {
            parent_params = self.transpile_ty_params_with_substs(impl_def_id, substs, false)?
        };

        let ty_params = self.tcx.item_generics(def_id).types.iter().map(|p| LeanTyParam::RustTyParam(p.name.as_str().to_string()));
        let assoc_ty_substs = self.get_assoc_ty_substs(def_id, substs)?;

        let predicates = if is_trait {
            // for trait items, ignore predicates on trait except for the `Self: Trait` predicate
            self.tcx.item_predicates(def_id).predicates.into_iter().filter(|pred| match *pred {
                ty::Predicate::Trait(ref trait_pred) => trait_pred.def_id() == def_id,
                _ => false,
            }).collect_vec()
        } else {
                self.tcx.item_predicates(def_id).predicates
        };
        let trait_params = predicates.into_iter()
            .filter_map(|trait_pred| match trait_pred {
                ty::Predicate::Trait(ref trait_pred) if !self.is_marker_trait(trait_pred.def_id()) => Some(trait_pred.clone().0),
                _ => None,
            })
            .try_flat_map(|trait_pred| {
                let trait_ref = trait_pred.trait_ref.subst(self.tcx, substs);
                self.add_dep(trait_ref.def_id);
                let free_assoc_tys = self.transpile_trait_ref_assoc_tys(trait_ref, &assoc_ty_substs)?.1;
                let free_assoc_tys = free_assoc_tys.into_iter().map(|ty| LeanTyParam::AssocTy(ty));
                let trait_param = LeanTyParam::TraitRef(
                    self.mk_lean_name(self.transpile_trait_ref_no_assoc_tys(trait_ref)?),
                    self.transpile_trait_ref(trait_ref, &assoc_ty_substs)?,
                    trait_ref);
                Ok(free_assoc_tys.chain(iter::once(trait_param)))
            })?;
        Ok(parent_params.into_iter().chain(ty_params).chain(trait_params).collect_vec())
    }

    pub fn transpile_ty_params(&self, def_id: DefId) -> TransResult<Vec<LeanTyParam<'tcx>>> {
        self.transpile_ty_params_with_substs(def_id, self.free_substs_for_item(def_id), false)
    }

    /// `Fn(&mut T) -> R` ~> `(R × T)`
    /// `Fn(&mut T) -> &mut S` ~> `lens T S`
    pub fn ret_ty(&self, in_tys: &[ty::Ty<'tcx>], out_ty: ty::Ty<'tcx>) -> TransResult {
        let muts = in_tys.iter().filter_map(|i| krate::try_unwrap_mut_ref(i));
        let out_ty = match krate::try_unwrap_mut_ref(out_ty) {
            Some(inner) => match in_tys.first().cloned().and_then(krate::try_unwrap_mut_ref) {
                Some(outer) =>
                    format!("(lens {} {})", self.transpile_ty(outer)?, self.transpile_ty(inner)?),
                None => throw!("unimplemented: returning mutable reference to argument other than the first"),
            },
            None => self.transpile_ty(out_ty)?,
        };
        Ok(format!("({})", (out_ty, try_iter!(muts.map(|ty| self.transpile_ty(ty)))).join(" × ")))
    }

    pub fn normalize_ty(&self, value: Ty<'tcx>) -> Ty<'tcx> {
        self.tcx.infer_ctxt(None, Some(ty::ParameterEnvironment::for_item(self.tcx, self.node_id())), Reveal::All).enter(|infcx| {
            let mut selcx = SelectionContext::new(&infcx);
            let normalized = normalize(&mut selcx, ObligationCause::dummy(), &value);
            let mut fulfill_cx = FulfillmentContext::new();
            for obl in normalized.obligations {
                fulfill_cx.register_predicate_obligation(&infcx, obl);
            }
            let span = ::syntax::codemap::DUMMY_SP;
            infcx.drain_fulfillment_cx_or_panic(span, &mut fulfill_cx, &normalized.value)
                .lift_to_tcx(self.tcx)
                .unwrap()
        })
    }

    pub fn normalize_trait_ref(&self, value: ty::TraitRef<'tcx>) -> ty::TraitRef<'tcx> {
        self.tcx.infer_ctxt(None, Some(ty::ParameterEnvironment::for_item(self.tcx, self.node_id())), Reveal::All).enter(|infcx| {
            let mut selcx = SelectionContext::new(&infcx);
            let normalized = normalize(&mut selcx, ObligationCause::dummy(), &value);
            let mut fulfill_cx = FulfillmentContext::new();
            for obl in normalized.obligations {
                fulfill_cx.register_predicate_obligation(&infcx, obl);
            }
            let span = ::syntax::codemap::DUMMY_SP;
            infcx.drain_fulfillment_cx_or_panic(span, &mut fulfill_cx, &normalized.value)
                .lift_to_tcx(self.tcx)
                .unwrap()
        })
    }

    pub fn transpile_ty(&self, ty: Ty<'tcx>) -> TransResult {
        Ok(match ty.sty {
            ty::TypeVariants::TyBool => "bool".to_string(),
            ty::TypeVariants::TyUint(ref ty) => ty.to_string(),
            ty::TypeVariants::TyInt(ref ty) => ty.to_string(),
            //ty::TypeVariants::TyFloat(ref ty) => ty.to_string(),
            ty::TypeVariants::TyTuple(ref tys) => mk_tuple_ty(
                tys.iter().map(|ty| self.transpile_ty(ty)).try()?),
            // `Fn(&mut T) -> R` ~> `'T -> sem (R × T)'`
            ty::TypeVariants::TyFnDef(_, _, ref data) => {
                let sig = data.sig.skip_binder();
                let inputs = try_iter!(sig.inputs.iter().map(|ty| self.transpile_ty(krate::unwrap_mut_ref(ty))));
                inputs.chain(iter::once(format!("sem {}", self.ret_ty(&sig.inputs, sig.output)?))).join(" → ")
            },
            ty::TypeVariants::TyAdt(ref adt_def, ref substs) => format!(
                "({})",
                (&self.name_def_id(adt_def.did), try_iter!(substs.types().map(|ty| self.transpile_ty(ty)))).join(" ")
            ),
            ty::TypeVariants::TyRef(_, ty::TypeAndMut {
                mutbl: hir::Mutability::MutImmutable, ref ty
            }) => self.transpile_ty(ty)?,
            ty::TypeVariants::TyParam(ref param) => param.name.to_string(),
            ty::TypeVariants::TyProjection(ref proj) => self.transpile_associated_type(proj.trait_ref, &proj.item_name),
            ty::TypeVariants::TySlice(ref ty) => format!("(slice {})", self.transpile_ty(ty)?),
            ty::TypeVariants::TyStr => "string".to_string(),
            ty::TypeVariants::TyTrait(_) => throw!("unimplemented: trait objects"),
            ty::TypeVariants::TyArray(ref ty, size) =>
                format!("(array {} {})", self.transpile_ty(ty)?, size),
            ty::TypeVariants::TyBox(ref ty) => {
                self.deps.borrow_mut().crate_deps.insert("alloc".to_string());
                format!("(alloc.boxed.Box {})", self.transpile_ty(ty)?)
            }
            ty::TypeVariants::TyClosure(def_id, ref substs) => {
                let upvar_tys = substs.upvar_tys(def_id, self.tcx).map(|ty| self.transpile_ty(ty)).try()?;
                format!("({})", (&self.name_def_id(def_id), upvar_tys).join(" "))
            }
            ty::TypeVariants::TyNever => "empty".to_string(),
            _ => match ty.ty_to_def_id() {
                Some(did) => self.name_def_id(did),
                None => throw!("unimplemented: ty {:?}", ty),
            }
        })
    }

    fn trait_predicates(&'a self, def_id: DefId) -> impl Iterator<Item=ty::TraitPredicate<'tcx>> {
        let predicates = if let Some(trait_def_id) = self.tcx.trait_of_item(def_id) {
            // for trait items, ignore predicates on trait except for the `Self: Trait` predicate
            let g = self.tcx.item_predicates(def_id);
            g.predicates.into_iter().chain(self.tcx.item_predicates(trait_def_id).predicates.into_iter().filter(|pred| match *pred {
                ty::Predicate::Trait(ref trait_pred) => trait_pred.def_id() == trait_def_id,
                _ => false,
            })).collect_vec()
        } else {
            ::itertools::Unfold::new(Some(def_id), |opt_def_id| {
                opt_def_id.map(|def_id| {
                    let g = self.tcx.item_predicates(def_id);
                    *opt_def_id = g.parent;
                    self.tcx.item_predicates(def_id).predicates
                })
            }).flat_map(|ps| ps).collect()
        };
        predicates.into_iter().filter_map(|trait_pred| match trait_pred {
            ty::Predicate::Trait(trait_pred) => Some(trait_pred.0),
            _ => None,
        })
    }

    fn is_marker_trait(&self, trait_def_id: DefId) -> bool {
        self.tcx.associated_items(trait_def_id).next().is_none() &&
        self.trait_predicates(trait_def_id).all(|trait_pred| {
            trait_pred.def_id() == trait_def_id || self.is_marker_trait(trait_pred.def_id())
        }) && {
            let name = self.name_def_id(trait_def_id);
            // marker traits that influence static semantics
            name != "core.marker.Unsize" && name != "core.ops.CoerceUnsized"
        }
    }

    pub fn trait_predicates_without_markers(&self, def_id: DefId) -> ::std::vec::IntoIter<ty::TraitPredicate<'tcx>> {
        self.trait_predicates(def_id).filter(|trait_pred| !self.is_marker_trait(trait_pred.def_id())).collect_vec().into_iter()
    }

    // ugh
    //
    // very incomplete implementation gleaned from the rustc sources (though those never have to
    // construct a full tree of impls)
    pub fn infer_trait_impl<'b, 'c>(&self, trait_ref: ty::TraitRef<'tcx>, infcx: &'b ::rustc::infer::InferCtxt<'b, 'tcx, 'c>) -> TransResult<TraitImplLookup<'tcx>> {
        let span = ::syntax::codemap::DUMMY_SP;
        let trait_ref = self.normalize_trait_ref(trait_ref);
        let pred: ty::PolyTraitPredicate<'tcx> = ty::Binder(trait_ref).to_poly_trait_predicate();

        let mut selcx = SelectionContext::new(infcx);
        let obligation = Obligation::new(ObligationCause::misc(span, ast::DUMMY_NODE_ID), pred);
        let selection = selcx.select(&obligation)
            .map_err(|err| {
                format!("obligation select: {:?} {:?}", obligation, err)
            })?
            .ok_or(format!("empty selection result: {:?}", obligation))?;

        Ok(match selection {
            Vtable::VtableImpl(data) => {
                let nested_traits = data.nested.iter().try_filter_map(|obl| -> TransResult<_> { Ok(match obl.predicate {
                    ty::Predicate::Trait(ref trait_pred) if !self.is_marker_trait(trait_pred.skip_binder().def_id()) => {
                        let trait_ref = &trait_pred.skip_binder().trait_ref;
                        let trait_ref = self.tcx.lift(trait_ref).ok_or(format!("failed to lift {:?}", trait_ref))?;
                        Some(self.infer_trait_impl(trait_ref, infcx)?.to_string(self)?)
                    }
                    _ => None,
                })})?;

                let mut fulfill_cx = FulfillmentContext::new();
                for obl in data.nested {
                    fulfill_cx.register_predicate_obligation(&infcx, obl);
                }
                let substs = infcx.drain_fulfillment_cx_or_panic(span, &mut fulfill_cx, &data.substs);

                TraitImplLookup::Static {
                    impl_def_id: data.impl_def_id,
                    params: nested_traits.collect(),
                    substs: substs.clone()
                }
            },
            Vtable::VtableParam(_) => TraitImplLookup::Dynamic { param: {
                let t = self.transpile_trait_ref_no_assoc_tys(trait_ref)?;
                if self.all_assoc_tys(trait_ref)?.is_empty() {
                    format!("(_ : {})", t)
                } else {
                    let param = self.mk_lean_name(t);
                    if !self.transpile_ty_params(self.def_id)?.iter().any(|p| p.name() == param) {
                        throw!("unimplemented: dynamic supertrait call with associated types")
                    }
                    param
                }
            }},
            Vtable::VtableClosure(_) => {
                TraitImplLookup::Dynamic {
                    // global instances in core/pre.lean
                    param: "_".to_string()
                }
            },
            vtable => throw!("unimplemented: vtable {:?}", vtable),
        })
    }

    // `self.def_id=Iterator, name_suffix=' [class]'` ~> `'Iterator [class] (T : Type₁)'`
    fn as_generic_ty_def(&self, name_suffix: &str) -> String {
        let generics = self.tcx.item_generics(self.def_id);
        (self.name() + name_suffix, generics.types.iter().map(|p| format!("({} : Type₁)", p.name))).join(" ")
    }

    pub fn mk_applied_ty(&self, name: &str, generics: &ty::Generics) -> String {
        (name, generics.types.iter().map(|p| p.name.as_str().to_string())).join(" ")
    }

    fn transpile_struct(&self, suffix: &str, variant: ty::VariantDef<'tcx>) -> TransResult {
        if self.transpile_ty_params(self.def_id)?.iter().any(|p| match *p {
            LeanTyParam::AssocTy(_) => true,
            _ => false,
        }) {
            throw!("unimplemented: struct with associated type dependency")
        }
        Ok(match variant.ctor_kind {
            CtorKind::Fictive => { // actual (non-fictive) struct
                let mut fields = variant.fields.iter().map(|f| -> TransResult {
                    Ok(format!("({} : {})", self.mk_lean_name(&*f.name.as_str()), self.transpile_ty(f.unsubst_ty())?))
                }).try()?;
                format!("structure {} := mk {{}} ::\n{}",
                        self.as_generic_ty_def(suffix),
                        fields.join("\n"))
            }
            CtorKind::Fn => { // tuple struct
                let mut fields = try_iter!(variant.fields.iter().map(|f| {
                    self.transpile_ty(f.unsubst_ty())
                }));
                let applied_ty = (self.name(), self.tcx.item_generics(self.def_id).types.iter().map(|p| p.name.as_str().to_string())).join(" ");
                format!("inductive {} :=\nmk {{}} : {} → {}",
                        self.as_generic_ty_def(suffix),
                        fields.join(" → "),
                        applied_ty)
            }
            CtorKind::Const => // unit struct
                format!("structure {} := mk {{}} ::",
                        self.as_generic_ty_def(suffix)),
        })
    }

    fn transpile_enum(&self, name: &str, adt_def: ty::AdtDef<'tcx>) -> TransResult {
        let generics = self.tcx.item_generics(self.def_id);
        let applied_ty = self.mk_applied_ty(name, generics);
        let mut prelude = adt_def.variants.iter().try_filter_map(|variant| -> TransResult<_> {Ok(match variant.ctor_kind {
            CtorKind::Fictive => { // struct variant
                Some(self.transpile_struct(&format!(".{}.struct", variant.name), variant)? + "\n\n")
            }
            _ => None,
        })})?;
        let mut variants = adt_def.variants.iter().map(|variant| -> TransResult<_> {Ok(match variant.ctor_kind {
            CtorKind::Const => // unit variant
                format!("| {} {{}} : {}", variant.name, applied_ty),
            CtorKind::Fn => { // tuple variant
                let fields = variant.fields.iter().map(|f| {
                    self.transpile_ty(f.unsubst_ty())
                }).try()?;
                let ty = fields.chain(iter::once(applied_ty.clone())).join(" → ");
                format!("| {} {{}} : {}", variant.name, ty)
            }
            CtorKind::Fictive => { // struct variant
                format!("| {} {{}} : {} → {}",
                        variant.name,
                        self.mk_applied_ty(&format!("{}.{}.struct", name, variant.name), generics),
                        applied_ty)
            }
        })}).try()?;
        // if no variants have data attached, add a function to extract the discriminant
        let discr = if !adt_def.variants.is_empty() && adt_def.variants.iter().all(|variant| variant.ctor_kind == CtorKind::Const) {
            let discrs = adt_def.variants.iter().map(|variant| {
                format!("| {}.{} := {}", name, variant.name,
                        variant.disr_val.to_u64_unchecked() as i64)
            }).join("\n");
            format!("\n\ndefinition {}.discr {} : isize := match self with\n{}\nend",
                    name,
                    (generics.types.iter().map(|p| format!("{{{} : Type₁}}", p.name)),
                     &format!("(self : {})", applied_ty)).join(" "),
                    discrs)
        } else { "".to_string() };
        Ok(format!("{}inductive {} :=\n{}{}",
                   prelude.join("\n\n"), self.as_generic_ty_def(""), variants.join("\n"), discr))
    }

    fn transpile_static(&self) -> TransResult {
        Ok(format!("definition {} : sem {} :=\n{}",
                   krate::name_def_id(self.tcx, self.def_id),
                   self.transpile_ty(self.tcx.item_type(self.def_id))?,
                   ::trans::fun::FnTranspiler::new(self, &*self.tcx.item_mir(self.def_id)).transpile_mir()?))
    }

    fn transpile_trait(&self, name: &str) -> TransResult {
        let substs = self.free_substs_for_item(self.def_id);
        let assoc_ty_substs = self.get_assoc_ty_substs(self.def_id, substs)?;
        let supertraits = try_iter!(self.trait_predicates_without_markers(self.def_id)
            .filter(|trait_pred| trait_pred.def_id() != self.def_id)
            .map(|trait_pred| self.transpile_trait_ref(trait_pred.trait_ref, &assoc_ty_substs)))
            .collect_vec();
        let extends = if supertraits.is_empty() { "".to_owned() } else {
            format!(" extends {}", supertraits.into_iter().join(", "))
        };

        let only_path = format!("traits.\"{}\".only", name);
        let only: Option<HashSet<_>> = self.config.config.lookup(&only_path).map(|only| ::toml_value_as_str_array(only).into_iter().collect());
        let items = self.tcx.associated_items(self.def_id).try_filter_map(|item| Ok(match item.kind {
            ty::AssociatedKind::Type => None,
            ty::AssociatedKind::Method => {
                // TODO: allow overriding default methods
                if self.tcx.provided_trait_methods(self.def_id).iter().any(|m| m.name == item.name) || only.iter().any(|only| !only.contains(&*item.name.as_str())) {
                    None
                } else {
                    let ty_params = self.transpile_ty_params(item.def_id)?;
                    let pi = if ty_params.is_empty() { "".to_string() } else {
                        format!("Π {}, ", ty_params.iter().map(LeanTyParam::to_string).join(" "))
                    };
                    let ty = self.transpile_ty(self.tcx.item_type(item.def_id))?;
                    Some(format!("({} : {}{})", self.mk_lean_name(item.name), pi, ty))
                }
            }
            ty::AssociatedKind::Const =>
                throw!("unimplemented: const trait items"),
        }))?.collect_vec();
        let trait_ref = self.tcx.lookup_trait_def(self.def_id).trait_ref;
        Ok(format!("structure {} {}{} {}",
                   self.as_generic_ty_def(" [class]"),
                   self.all_assoc_tys(trait_ref)?.into_iter().map(|assoc_ty| {
                       format!("({} : Type₁)", assoc_ty)
                   }).join(" "),
                   extends,
                   if items.is_empty() { "".to_string() }
                   else { format!(":=\n{}", items.join("\n")) }))
    }

    fn trait_impl_substs(&self, trait_ref: ty::TraitRef) -> TransResult<HashMap<String, String>> {
        let mut substs = self.get_assoc_ty_substs(self.def_id, self.free_substs_for_item(self.def_id))?;

        // For `type S = T`, extend `assoc_ty_substs` by `{'S': 'T'}`
        for item in self.tcx.associated_items(self.def_id) {
            if item.kind == ty::AssociatedKind::Type {
                let ty = self.normalize_ty(self.tcx.item_type(item.def_id));
                substs.insert(self.transpile_associated_type(trait_ref, &item.name), self.transpile_ty(ty)?);
            }
        }
        Ok(substs)
    }

    fn transpile_trait_impl(&self) -> TransResult {
        let mut trait_ref = self.normalize_trait_ref(self.tcx.impl_trait_ref(self.def_id).unwrap());
        trait_ref.substs = self.tcx.erase_regions(&trait_ref.substs);
        let assoc_ty_substs = self.trait_impl_substs(trait_ref)?;

        let ty_params = self.transpile_ty_params(self.def_id)?;
        let supertrait_impls = self.tcx.infer_ctxt(None, Some(ty::ParameterEnvironment::for_item(self.tcx, self.node_id())), Reveal::All).enter(|infcx| {
            self.trait_predicates_without_markers(trait_ref.def_id).map(|p| p.subst(self.tcx, trait_ref.substs))
                .filter(|trait_pred| trait_pred.def_id() != trait_ref.def_id)
                .map(|trait_pred| self.infer_trait_impl(trait_pred.trait_ref, &infcx)?.to_string(self))
                .try()
        })?;

        let only_path = format!("traits.\"{}\".only", &self.name_def_id(trait_ref.def_id));
        let only: Option<HashSet<_>> = self.config.config.lookup(&only_path).map(|only| ::toml_value_as_str_array(only).into_iter().collect());
        let items = self.tcx.associated_items(self.def_id).try_filter_map(|item| Ok(match item.kind {
            ty::AssociatedKind::Type =>
                None,
            ty::AssociatedKind::Method => {
                if only.iter().any(|only| !only.contains(&*item.name.as_str())) {
                    None // method ignored in config
                } else if self.tcx.provided_trait_methods(trait_ref.def_id).iter().any(|m| m.name == item.name) {
                    throw!("unimplemented: overriding default method {:?}", self.name_def_id(item.def_id))
                } else {
                    Some(format!("{} := @{}", self.mk_lean_name(item.name), (self.name_def_id(item.def_id), ty_params.iter().map(|p| p.name())).join(" ")))
                }
            }
            ty::AssociatedKind::Const =>
                throw!("unimplemented: const trait items"),
        }))?.collect_vec();

        Ok(format!("definition {} := ⦃\n  {}\n⦄",
                   (self.name() + " [instance]", ty_params.iter().map(LeanTyParam::to_string)).join(" "),
                   (self.transpile_trait_ref(trait_ref, &assoc_ty_substs)?, supertrait_impls.into_iter().chain(items)).join(",\n  ")))
    }

    fn transpile_fn(&self, name: String) -> TransResult {
        ::trans::fun::FnTranspiler::new(self, &*self.tcx.item_mir(self.def_id)).transpile_fn(name)
    }

    pub fn transpile_def_id(&self) -> TransResult<Option<String>> {
        use rustc::hir::map::Node;
        use rustc::hir::Item_;
        let name = self.name();

        let node = self.tcx.map.get(self.node_id());
        Ok(Some(match node {
            Node::NodeItem(item) => match item.node {
                Item_::ItemExternCrate(_) | Item_::ItemUse(_) | Item_::ItemMod(_)
                | Item_::ItemForeignMod(_) => return Ok(None),
                Item_::ItemStatic(_, hir::Mutability::MutMutable, _) =>
                    throw!("unimplemented: mutable static {:?}", name),
                Item_::ItemStatic(_, hir::Mutability::MutImmutable, _) | Item_::ItemConst(..) =>
                    self.transpile_static()?,
                Item_::ItemEnum(..) =>
                    match self.tcx.item_type(self.def_id).sty {
                        ty::TypeVariants::TyAdt(ref adt_def, _) =>
                            self.transpile_enum(&name, adt_def)?,
                        _ => unreachable!(),
                    },
                Item_::ItemStruct(..) =>
                    match self.tcx.item_type(self.def_id).sty {
                        ty::TypeVariants::TyAdt(ref adt_def, _) =>
                            self.transpile_struct("", adt_def.struct_variant())?,
                        _ => unreachable!(),
                    },
                Item_::ItemTrait(..) => {
                    if self.is_marker_trait(self.def_id) {
                        return Ok(None)
                    }
                    self.transpile_trait(&name)?
                }
                Item_::ItemDefaultImpl(..) => return Ok(None),
                Item_::ItemImpl(..) => {
                    if let Some(trait_ref) = self.tcx.impl_trait_ref(self.def_id) {
                        if !self.is_marker_trait(trait_ref.def_id) {
                            return Ok(Some(self.transpile_trait_impl()?))
                        }
                    }
                    return Ok(None)
                }
                Item_::ItemTy(..) =>
                    format!("definition {} := {}",
                            self.as_generic_ty_def(""),
                            self.transpile_ty(self.tcx.item_type(self.def_id))?),
                Item_::ItemFn(..) =>
                    self.transpile_fn(name)?,
                Item_::ItemUnion(..) => throw!("unimplemented: {:?}", node),
            },
            Node::NodeExpr(_) => // top-level expr? closure!
                self.transpile_fn(name)?,
            Node::NodeTraitItem(&hir::TraitItem { node: hir::TraitItem_::MethodTraitItem(_, Some(_)), .. })
            | Node::NodeImplItem(&hir::ImplItem { node: hir::ImplItemKind::Method(..), .. }) =>
                self.transpile_fn(name)?,
            Node::NodeTraitItem(_) | Node::NodeVariant(_) | Node::NodeStructCtor(_)
            | Node::NodeImplItem(&hir::ImplItem { node: hir::ImplItemKind::Type(..), .. }) =>
                return Ok(None),
            _ => throw!("unimplemented: {:?}", node),
        }))
    }
}
