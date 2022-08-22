//! Validity checking for weak lang items

use rustc_data_structures::fx::FxHashSet;
use rustc_errors::struct_span_err;
use rustc_hir::lang_items::{self, LangItem};
use rustc_hir::weak_lang_items::WEAK_ITEMS_REFS;
use rustc_middle::middle::lang_items::required;
use rustc_middle::ty::TyCtxt;
use rustc_session::config::CrateType;

use crate::errors::{MissingAllocErrorHandler, MissingLangItem, MissingPanicHandler};

/// Checks the crate for usage of weak lang items, returning a vector of all the
/// language items required by this crate, but not defined yet.
pub fn check_crate<'tcx>(tcx: TyCtxt<'tcx>, items: &mut lang_items::LanguageItems) {
    // These are never called by user code, they're generated by the compiler.
    // They will never implicitly be added to the `missing` array unless we do
    // so here.
    if items.eh_personality().is_none() {
        items.missing.push(LangItem::EhPersonality);
    }
    if tcx.sess.target.os == "emscripten" && items.eh_catch_typeinfo().is_none() {
        items.missing.push(LangItem::EhCatchTypeinfo);
    }

    let crate_items = tcx.hir_crate_items(());
    for id in crate_items.foreign_items() {
        let attrs = tcx.hir().attrs(id.hir_id());
        if let Some((lang_item, _)) = lang_items::extract(attrs) {
            if let Some(&item) = WEAK_ITEMS_REFS.get(&lang_item) {
                if items.require(item).is_err() {
                    items.missing.push(item);
                }
            } else {
                let span = tcx.def_span(id.def_id);
                struct_span_err!(
                    tcx.sess,
                    span,
                    E0264,
                    "unknown external lang item: `{}`",
                    lang_item
                )
                .emit();
            }
        }
    }

    verify(tcx, items);
}

fn verify<'tcx>(tcx: TyCtxt<'tcx>, items: &lang_items::LanguageItems) {
    // We only need to check for the presence of weak lang items if we're
    // emitting something that's not an rlib.
    let needs_check = tcx.sess.crate_types().iter().any(|kind| match *kind {
        CrateType::Dylib
        | CrateType::ProcMacro
        | CrateType::Cdylib
        | CrateType::Executable
        | CrateType::Staticlib => true,
        CrateType::Rlib => false,
    });
    if !needs_check {
        return;
    }

    let mut missing = FxHashSet::default();
    for &cnum in tcx.crates(()).iter() {
        for &item in tcx.missing_lang_items(cnum).iter() {
            missing.insert(item);
        }
    }

    for (name, &item) in WEAK_ITEMS_REFS.iter() {
        if missing.contains(&item) && required(tcx, item) && items.require(item).is_err() {
            if item == LangItem::PanicImpl {
                tcx.sess.emit_err(MissingPanicHandler);
            } else if item == LangItem::Oom {
                if !tcx.features().default_alloc_error_handler {
                    tcx.sess.emit_err(MissingAllocErrorHandler);
                }
            } else {
                tcx.sess.emit_err(MissingLangItem { name: *name });
            }
        }
    }
}
