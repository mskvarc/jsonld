//! Forking a vocabulary for concurrent work, and merging it back.
//!
//! An interning vocabulary hands out identifiers that are only meaningful
//! relative to the table that minted them. Giving each concurrent task its own
//! copy therefore cannot be the whole story: two tasks that intern different
//! IRIs both start from the same next index and both hand that index out, so
//! their outputs cannot simply be concatenated.
//!
//! [`ForkableVocabulary`] closes that gap. [`ForkableVocabulary::fork`] hands a
//! task an independent vocabulary; [`ForkableVocabulary::merge`] folds whatever
//! the task interned back into the parent and returns a
//! [`VocabularyRemap`] describing where those terms ended up. Applying the
//! remap to the task's output — via `map_ids` — makes the output valid in the
//! parent, and only then may it be mixed with the output of other tasks.

use crate::{HashMap, Id, ValidId};
use iri_rs::{Iri, IriBuf};
use rdfx::{
    BlankId,
    BlankIdBuf,
    Literal,
    LiteralRef,
    vocabulary::{
        BlankIdVocabulary,
        BlankIdVocabularyMut,
        IndexVocabulary,
        IndexedBlankId,
        IndexedIri,
        IndexedLiteral,
        IriVocabulary,
        IriVocabularyMut,
        LiteralVocabulary,
        LiteralVocabularyMut,
        NoVocabulary,
        VocabularyMut,
    },
};
use std::hash::Hash;

/// Translation from the identifiers a fork minted to the identifiers the same
/// terms received when that fork was merged into its parent.
///
/// Terms the fork inherited from its parent keep their identifier and are
/// absent from the remap, so a remap holding nothing means nothing needs
/// rewriting — see [`VocabularyRemap::is_identity`].
#[derive(Debug, Clone)]
pub struct VocabularyRemap<I, B> {
    iris: HashMap<I, I>,
    blank_ids: HashMap<B, B>,
}

impl<I, B> VocabularyRemap<I, B> {
    /// A remap that leaves every identifier unchanged.
    pub fn identity() -> VocabularyRemap<I, B> {
        VocabularyRemap {
            iris: HashMap::default(),
            blank_ids: HashMap::default(),
        }
    }

    /// Whether this remap leaves every identifier unchanged.
    ///
    /// Callers check this to skip rewriting a task's output altogether, which
    /// is the whole cost of the remap for vocabularies that never intern.
    pub fn is_identity(&self) -> bool {
        self.iris.is_empty() && self.blank_ids.is_empty()
    }
}

impl<I: Clone + Eq + Hash, B: Clone + Eq + Hash> VocabularyRemap<I, B> {
    /// Records that the IRI identifier `minted` by a fork became `merged` in
    /// the parent.
    pub fn insert_iri(&mut self, minted: I, merged: I) {
        self.iris.insert(minted, merged);
    }

    /// Records that the blank node identifier `minted` by a fork became
    /// `merged` in the parent.
    pub fn insert_blank_id(&mut self, minted: B, merged: B) {
        self.blank_ids.insert(minted, merged);
    }

    /// Translates an IRI identifier produced by the fork.
    ///
    /// Identifiers the fork inherited are returned unchanged.
    pub fn iri(&self, iri: I) -> I {
        match self.iris.get(&iri) {
            Some(merged) => merged.clone(),
            None => iri,
        }
    }

    /// Translates a blank node identifier produced by the fork.
    ///
    /// Identifiers the fork inherited are returned unchanged.
    pub fn blank_id(&self, blank_id: B) -> B {
        match self.blank_ids.get(&blank_id) {
            Some(merged) => merged.clone(),
            None => blank_id,
        }
    }

    /// Translates a node identifier produced by the fork.
    ///
    /// Invalid identifiers carry their original string rather than a
    /// vocabulary index, so they pass through untouched.
    pub fn id(&self, id: Id<I, B>) -> Id<I, B> {
        match id {
            Id::Valid(ValidId::Iri(iri)) => Id::Valid(ValidId::Iri(self.iri(iri))),
            Id::Valid(ValidId::Blank(blank_id)) => Id::Valid(ValidId::Blank(self.blank_id(blank_id))),
            Id::Invalid(reference) => Id::Invalid(reference),
        }
    }
}

/// A vocabulary that can be forked for a concurrent task and merged back
/// afterwards.
///
/// [`Self::Fork`] is an associated type rather than `Self` so that `&mut V`
/// can implement this trait: a `&mut V` cannot produce a `Self`, but it can
/// delegate to `V` and produce a `V::Fork`. That is what lets callers keep
/// passing `&mut vocabulary` to the by-value vocabulary parameters of the
/// processing APIs.
pub trait ForkableVocabulary: IriVocabulary + BlankIdVocabulary {
    /// The independent vocabulary handed to a task.
    ///
    /// A fork is itself forkable, and forking one yields the same type again:
    /// a task that recurses into a nested array forks a second time, and
    /// without `Fork = Self::Fork` that would demand an unbounded tower of
    /// distinct fork types.
    type Fork: VocabularyMut<Iri = Self::Iri, BlankId = Self::BlankId> + ForkableVocabulary<Iri = Self::Iri, BlankId = Self::BlankId, Fork = Self::Fork>;

    /// Creates a vocabulary a concurrent task can use on its own.
    fn fork(&self) -> Self::Fork;

    /// Folds everything `fork` interned back into this vocabulary.
    ///
    /// The returned remap must be applied to the output that `fork` produced
    /// before that output is combined with anything derived from this
    /// vocabulary. Skipping it leaves identifiers that either denote the wrong
    /// term or denote nothing at all.
    fn merge(&mut self, fork: Self::Fork) -> VocabularyRemap<Self::Iri, Self::BlankId>;
}

/// An [`IndexVocabulary`] forked for a concurrent task.
///
/// Along with the copied table this records the parent's index counts at the
/// moment of the fork. Those watermarks have to be captured here rather than
/// read back during [`ForkableVocabulary::merge`], because the parent grows
/// with every merge: by the time a second fork is merged, the parent's count
/// no longer marks where that fork's own entries begin.
pub struct IndexVocabularyFork<I, B, L> {
    vocabulary: IndexVocabulary<I, B, L>,
    iri_watermark: usize,
    blank_id_watermark: usize,
    literal_watermark: usize,
}

impl<I: IndexedIri, B, L> IriVocabulary for IndexVocabularyFork<I, B, L> {
    type Iri = I;

    fn iri<'i>(&'i self, id: &'i I) -> Option<Iri<&'i str>> {
        self.vocabulary.iri(id)
    }

    fn owned_iri(&self, id: I) -> Result<IriBuf, I> {
        self.vocabulary.owned_iri(id)
    }

    fn get(&self, iri: Iri<&str>) -> Option<I> {
        self.vocabulary.get(iri)
    }
}

impl<I: IndexedIri, B, L> IriVocabularyMut for IndexVocabularyFork<I, B, L> {
    fn insert(&mut self, iri: Iri<&str>) -> I {
        self.vocabulary.insert(iri)
    }

    fn insert_owned(&mut self, iri: IriBuf) -> I {
        self.vocabulary.insert_owned(iri)
    }
}

impl<I, B: IndexedBlankId, L> BlankIdVocabulary for IndexVocabularyFork<I, B, L> {
    type BlankId = B;

    fn blank_id<'b>(&'b self, id: &'b B) -> Option<&'b BlankId> {
        self.vocabulary.blank_id(id)
    }

    fn owned_blank_id(&self, id: B) -> Result<BlankIdBuf, B> {
        self.vocabulary.owned_blank_id(id)
    }

    fn get_blank_id(&self, blank_id: &BlankId) -> Option<B> {
        self.vocabulary.get_blank_id(blank_id)
    }
}

impl<I, B: IndexedBlankId, L> BlankIdVocabularyMut for IndexVocabularyFork<I, B, L> {
    fn insert_blank_id(&mut self, blank_id: &BlankId) -> B {
        self.vocabulary.insert_blank_id(blank_id)
    }

    fn insert_owned_blank_id(&mut self, blank_id: BlankIdBuf) -> B {
        self.vocabulary.insert_owned_blank_id(blank_id)
    }
}

impl<I, B, L: IndexedLiteral> LiteralVocabulary for IndexVocabularyFork<I, B, L> {
    type Literal = L;

    fn literal<'l>(&'l self, id: &'l L) -> Option<LiteralRef<'l>> {
        self.vocabulary.literal(id)
    }

    fn owned_literal(&self, id: L) -> Result<Literal, L> {
        self.vocabulary.owned_literal(id)
    }

    fn get_literal(&self, literal: LiteralRef<'_>) -> Option<L> {
        self.vocabulary.get_literal(literal)
    }
}

impl<I, B, L: IndexedLiteral> LiteralVocabularyMut for IndexVocabularyFork<I, B, L> {
    fn insert_literal(&mut self, literal: LiteralRef<'_>) -> L {
        self.vocabulary.insert_literal(literal)
    }

    fn insert_owned_literal(&mut self, literal: Literal) -> L {
        self.vocabulary.insert_owned_literal(literal)
    }
}

/// Snapshots `vocabulary` for a task, recording where its tables currently end.
fn fork_index_vocabulary<I, B, L>(vocabulary: &IndexVocabulary<I, B, L>) -> IndexVocabularyFork<I, B, L>
where
    I: IndexedIri,
    B: IndexedBlankId,
    L: IndexedLiteral,
{
    IndexVocabularyFork {
        // The task must be able to resolve identifiers the document already
        // carries, all of which index the parent's table, so the fork starts
        // as a copy rather than empty.
        vocabulary: vocabulary.clone(),
        iri_watermark: vocabulary.iri_count(),
        blank_id_watermark: vocabulary.blank_id_count(),
        literal_watermark: vocabulary.literal_count(),
    }
}

/// Folds everything `fork` interned past its watermarks into `parent`.
fn merge_index_vocabulary<I, B, L>(parent: &mut IndexVocabulary<I, B, L>, fork: IndexVocabularyFork<I, B, L>) -> VocabularyRemap<I, B>
where
    I: IndexedIri + Clone + Eq + Hash,
    B: IndexedBlankId + Clone + Eq + Hash,
    L: IndexedLiteral,
{
    let mut remap = VocabularyRemap::identity();

    for (offset, iri) in fork.vocabulary.iris_from(fork.iri_watermark).enumerate() {
        let minted = I::from(fork.iri_watermark + offset);
        let merged = parent.insert(iri.as_ref());
        if merged != minted {
            remap.insert_iri(minted, merged);
        }
    }

    for (offset, blank_id) in fork.vocabulary.blank_ids_from(fork.blank_id_watermark).enumerate() {
        let minted = B::from(fork.blank_id_watermark + offset);
        let merged = parent.insert_blank_id(blank_id.as_blank_id_ref());
        if merged != minted {
            remap.insert_blank_id(minted, merged);
        }
    }

    // Literals are folded in so the parent ends up holding everything the fork
    // learned, but they get no entry in the remap: expansion builds
    // `jsonld_core::object::value::Literal` values inline and never puts a
    // vocabulary literal identifier into its output, so a literal remap would
    // have nothing to rewrite.
    for literal in fork.vocabulary.literals_from(fork.literal_watermark) {
        let _ = parent.insert_literal(literal.as_ref());
    }

    remap
}

impl<I, B, L> ForkableVocabulary for IndexVocabulary<I, B, L>
where
    I: IndexedIri + Clone + Eq + Hash,
    B: IndexedBlankId + Clone + Eq + Hash,
    L: IndexedLiteral,
{
    type Fork = IndexVocabularyFork<I, B, L>;

    fn fork(&self) -> IndexVocabularyFork<I, B, L> {
        fork_index_vocabulary(self)
    }

    fn merge(&mut self, fork: IndexVocabularyFork<I, B, L>) -> VocabularyRemap<I, B> {
        merge_index_vocabulary(self, fork)
    }
}

impl<I, B, L> ForkableVocabulary for IndexVocabularyFork<I, B, L>
where
    I: IndexedIri + Clone + Eq + Hash,
    B: IndexedBlankId + Clone + Eq + Hash,
    L: IndexedLiteral,
{
    type Fork = IndexVocabularyFork<I, B, L>;

    fn fork(&self) -> IndexVocabularyFork<I, B, L> {
        // Watermarks are taken against this fork's own table, not the
        // watermarks it was created with, so a nested fork records where *it*
        // started interning.
        fork_index_vocabulary(&self.vocabulary)
    }

    fn merge(&mut self, fork: IndexVocabularyFork<I, B, L>) -> VocabularyRemap<I, B> {
        merge_index_vocabulary(&mut self.vocabulary, fork)
    }
}

impl ForkableVocabulary for NoVocabulary {
    type Fork = NoVocabulary;

    fn fork(&self) -> NoVocabulary {}

    fn merge(&mut self, fork: NoVocabulary) -> VocabularyRemap<IriBuf, BlankIdBuf> {
        // Nothing is interned, so identifiers are the terms themselves and a
        // fork cannot have minted anything that needs translating.
        let () = fork;
        VocabularyRemap::identity()
    }
}

impl<V: ForkableVocabulary> ForkableVocabulary for &mut V {
    type Fork = V::Fork;

    fn fork(&self) -> V::Fork {
        V::fork(self)
    }

    fn merge(&mut self, fork: V::Fork) -> VocabularyRemap<V::Iri, V::BlankId> {
        V::merge(self, fork)
    }
}

/// Marker trait for vocabularies usable from the concurrent algorithms.
///
/// When `parallel` is off, every type satisfies this trait via a blanket impl —
/// code that bounds on it still compiles in default builds.
#[cfg(not(feature = "parallel"))]
pub trait ParallelSafeVocabulary {}

#[cfg(not(feature = "parallel"))]
impl<T: ?Sized> ParallelSafeVocabulary for T {}

/// Marker trait for vocabularies usable from the concurrent algorithms.
///
/// With the `parallel` feature this demands [`ForkableVocabulary`] with a
/// `Send + Sync` fork, because each task works on its own fork and hands it
/// back to be merged. It deliberately does *not* demand `Clone`: a bare clone
/// is what made the concurrent paths unsound, since nothing folded the clone's
/// interned terms back into the parent.
///
/// `Fork: Send + Sync` is also what makes a fork itself `ParallelSafeVocabulary`
/// — a fork's own fork type is pinned to itself — so a task may recurse into a
/// nested array and fork again.
#[cfg(feature = "parallel")]
pub trait ParallelSafeVocabulary: Send + Sync + ForkableVocabulary<Fork: Send + Sync> {}

#[cfg(feature = "parallel")]
impl<T: Send + Sync + ForkableVocabulary<Fork: Send + Sync>> ParallelSafeVocabulary for T {}
