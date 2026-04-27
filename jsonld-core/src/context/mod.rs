//! Context processing algorithm and related types.
mod definition;
pub mod inverse;

use crate::{Direction, LenientLangTag, LenientLangTagBuf, Term};
use contextual::WithContext;
use iri_rs::IriBuf;
use jsonld_syntax::{KeywordType, Nullable};
use once_cell::sync::OnceCell;
use crate::ValidId as Id;
use rdf_rs::vocabulary::Vocabulary;
use rdf_rs::BlankIdBuf;
use std::borrow::Borrow;
use std::hash::Hash;
use std::sync::Arc;

pub use jsonld_syntax::context::{
	definition::{Key, KeyOrType, Type},
	term_definition::Nest,
};

pub use definition::*;
pub use inverse::InverseContext;

/// Processed JSON-LD context.
///
/// Represents the result of the [context processing algorithm][1] implemented
/// by the [`json-ld-context-processing`] crate.
///
/// [1]: <https://www.w3.org/TR/json-ld11-api/#context-processing-algorithm>
/// [`json-ld-context-processing`]: <https://crates.io/crates/json-ld-context-processing>
pub struct Context<T = IriBuf, B = BlankIdBuf> {
	original_base_url: Option<T>,
	base_iri: Option<T>,
	vocabulary: Option<Term<T, B>>,
	default_language: Option<LenientLangTagBuf>,
	default_base_direction: Option<Direction>,
	previous_context: Option<Arc<Self>>,
	definitions: Arc<Definitions<T, B>>,
	inverse: OnceCell<InverseContext<T, B>>,
}

impl<T, B> Default for Context<T, B> {
	fn default() -> Self {
		Self {
			original_base_url: None,
			base_iri: None,
			vocabulary: None,
			default_language: None,
			default_base_direction: None,
			previous_context: None,
			definitions: Arc::new(Definitions::default()),
			inverse: OnceCell::default(),
		}
	}
}

pub type DefinitionEntryRef<'a, T = IriBuf, B = BlankIdBuf> = (&'a Key, &'a TermDefinition<T, B>);

impl<T, B> Context<T, B> {
	/// Create a new context with the given base IRI.
	pub fn new(base_iri: Option<T>) -> Self
	where
		T: Clone,
	{
		Self {
			original_base_url: base_iri.clone(),
			base_iri,
			vocabulary: None,
			default_language: None,
			default_base_direction: None,
			previous_context: None,
			definitions: Arc::new(Definitions::default()),
			inverse: OnceCell::default(),
		}
	}

	/// Returns a reference to the given `term` definition, if any.
	pub fn get<Q>(&self, term: &Q) -> Option<TermDefinitionRef<'_, T, B>>
	where
		Key: Borrow<Q>,
		KeywordType: Borrow<Q>,
		Q: ?Sized + Hash + Eq,
	{
		self.definitions.get(term)
	}

	/// Returns a reference to the given `term` normal definition, if any.
	pub fn get_normal<Q>(&self, term: &Q) -> Option<&NormalTermDefinition<T, B>>
	where
		Key: Borrow<Q>,
		Q: ?Sized + Hash + Eq,
	{
		self.definitions.get_normal(term)
	}

	/// Returns a reference to the `@type` definition, if any.
	pub fn get_type(&self) -> Option<&TypeTermDefinition> {
		self.definitions.get_type()
	}

	/// Checks if the given `term` is defined.
	pub fn contains_term<Q>(&self, term: &Q) -> bool
	where
		Key: Borrow<Q>,
		KeywordType: Borrow<Q>,
		Q: ?Sized + Hash + Eq,
	{
		self.definitions.contains_term(term)
	}

	/// Returns the original base URL of the context.
	pub fn original_base_url(&self) -> Option<&T> {
		self.original_base_url.as_ref()
	}

	/// Returns the base IRI of the context.
	pub fn base_iri(&self) -> Option<&T> {
		self.base_iri.as_ref()
	}

	/// Returns the `@vocab` value, if any.
	pub fn vocabulary(&self) -> Option<&Term<T, B>> {
		match &self.vocabulary {
			Some(v) => Some(v),
			None => None,
		}
	}

	/// Returns the default `@language` value.
	pub fn default_language(&self) -> Option<&LenientLangTag> {
		self.default_language
			.as_ref()
			.map(|tag| tag.as_lenient_lang_tag_ref())
	}

	/// Returns the default `@direction` value.
	pub fn default_base_direction(&self) -> Option<Direction> {
		self.default_base_direction
	}

	/// Returns a reference to the previous context.
	pub fn previous_context(&self) -> Option<&Self> {
		match &self.previous_context {
			Some(c) => Some(c),
			None => None,
		}
	}

	/// Returns the address of the `Arc` backing the term definitions.
	///
	/// Two contexts that share this pointer share the exact same definitions
	/// (copy-on-write via [`Arc::make_mut`]). Useful as a cheap fingerprint
	/// for memoization keys.
	pub fn definitions_arc_ptr(&self) -> *const Definitions<T, B> {
		Arc::as_ptr(&self.definitions)
	}

	/// Returns the address of the `Arc` backing the previous context, or null
	/// when there is none. See [`Self::definitions_arc_ptr`].
	pub fn previous_context_arc_ptr(&self) -> *const Self {
		self.previous_context
			.as_ref()
			.map(Arc::as_ptr)
			.unwrap_or(std::ptr::null())
	}

	/// Returns the number of terms defined.
	pub fn len(&self) -> usize {
		self.definitions.len()
	}

	/// Checks if no terms are defined.
	pub fn is_empty(&self) -> bool {
		self.definitions.is_empty()
	}

	/// Returns a handle to the term definitions.
	pub fn definitions(&self) -> &Definitions<T, B> {
		&self.definitions
	}

	/// Checks if the context has a protected definition.
	pub fn has_protected_items(&self) -> bool {
		for binding in self.definitions() {
			if binding.definition().protected() {
				return true;
			}
		}

		false
	}

	/// Returns the inverse of this context.
	pub fn inverse(&self) -> &InverseContext<T, B>
	where
		T: Clone + Hash + Eq,
		B: Clone + Hash + Eq,
	{
		self.inverse.get_or_init(|| self.into())
	}

	/// Sets the normal definition for the given term `key`.
	pub fn set_normal(
		&mut self,
		key: Key,
		definition: Option<NormalTermDefinition<T, B>>,
	) -> Option<NormalTermDefinition<T, B>>
	where
		T: Clone,
		B: Clone,
	{
		self.inverse.take();
		Arc::make_mut(&mut self.definitions).set_normal(key, definition)
	}

	/// Sets the `@type` definition.
	pub fn set_type(&mut self, type_: Option<TypeTermDefinition>) -> Option<TypeTermDefinition>
	where
		T: Clone,
		B: Clone,
	{
		Arc::make_mut(&mut self.definitions).set_type(type_)
	}

	/// Sets the base IRI.
	pub fn set_base_iri(&mut self, iri: Option<T>) {
		self.inverse.take();
		self.base_iri = iri
	}

	/// Sets the `@vocab` value.
	pub fn set_vocabulary(&mut self, vocab: Option<Term<T, B>>) {
		self.inverse.take();
		self.vocabulary = vocab;
	}

	/// Sets the default `@language` value.
	pub fn set_default_language(&mut self, lang: Option<LenientLangTagBuf>) {
		self.inverse.take();
		self.default_language = lang;
	}

	/// Sets the default `@direction` value.
	pub fn set_default_base_direction(&mut self, dir: Option<Direction>) {
		self.inverse.take();
		self.default_base_direction = dir;
	}

	/// Sets the previous context.
	pub fn set_previous_context(&mut self, previous: Self) {
		self.inverse.take();
		self.previous_context = Some(Arc::new(previous))
	}

	/// Converts this context into its syntactic definition.
	pub fn into_syntax_definition(
		self,
		vocabulary: &impl Vocabulary<Iri = T, BlankId = B>,
	) -> jsonld_syntax::context::Definition
	where
		T: Clone,
		B: Clone,
	{
		let (bindings, type_) = Arc::unwrap_or_clone(self.definitions).into_parts();

		jsonld_syntax::context::Definition {
			base: self
				.base_iri
				.map(|i| {
					let iri: iri_rs::IriBuf = vocabulary.iri(&i).unwrap().into();
					Nullable::Some(iri.into())
				}),
			import: None,
			language: self.default_language.map(Nullable::Some),
			direction: self.default_base_direction.map(Nullable::Some),
			propagate: None,
			protected: None,
			type_: type_.map(TypeTermDefinition::into_syntax_definition),
			version: None,
			vocab: self.vocabulary.map(|v| match v {
				Term::Null => Nullable::Null,
				Term::Id(r) => Nullable::Some(r.with(vocabulary).to_string().into()),
				Term::Keyword(_) => panic!("invalid vocab"),
			}),
			bindings: bindings
				.into_iter()
				.map(|(key, definition)| (key, definition.into_syntax_definition(vocabulary)))
				.collect(),
		}
	}

	pub fn map_ids<U, C>(
		self,
		mut map_iri: impl FnMut(T) -> U,
		mut map_id: impl FnMut(Id<T, B>) -> Id<U, C>,
	) -> Context<U, C>
	where
		T: Clone,
		B: Clone,
	{
		self.map_ids_with(&mut map_iri, &mut map_id)
	}

	fn map_ids_with<U, C>(
		self,
		map_iri: &mut impl FnMut(T) -> U,
		map_id: &mut impl FnMut(Id<T, B>) -> Id<U, C>,
	) -> Context<U, C>
	where
		T: Clone,
		B: Clone,
	{
		Context {
			original_base_url: self.original_base_url.map(&mut *map_iri),
			base_iri: self.base_iri.map(&mut *map_iri),
			vocabulary: self.vocabulary.map(|v| v.map_id(&mut *map_id)),
			default_language: self.default_language,
			default_base_direction: self.default_base_direction,
			previous_context: self
				.previous_context
				.map(|c| Arc::new(Arc::unwrap_or_clone(c).map_ids_with(map_iri, map_id))),
			definitions: Arc::new(Arc::unwrap_or_clone(self.definitions).map_ids(map_iri, map_id)),
			inverse: OnceCell::new(),
		}
	}
}

/// Context fragment to syntax method.
pub trait IntoSyntax<T = IriBuf, B = BlankIdBuf> {
	fn into_syntax(
		self,
		vocabulary: &impl Vocabulary<Iri = T, BlankId = B>,
	) -> jsonld_syntax::context::Context;
}

impl<T, B> IntoSyntax<T, B> for jsonld_syntax::context::Context {
	fn into_syntax(
		self,
		_namespace: &impl Vocabulary<Iri = T, BlankId = B>,
	) -> jsonld_syntax::context::Context {
		self
	}
}

impl<T: Clone, B: Clone> IntoSyntax<T, B> for Context<T, B> {
	fn into_syntax(
		self,
		vocabulary: &impl Vocabulary<Iri = T, BlankId = B>,
	) -> jsonld_syntax::context::Context {
		jsonld_syntax::context::Context::One(jsonld_syntax::ContextEntry::Definition(
			self.into_syntax_definition(vocabulary),
		))
	}
}

impl<T: Clone, B: Clone> Clone for Context<T, B> {
	fn clone(&self) -> Self {
		Self {
			original_base_url: self.original_base_url.clone(),
			base_iri: self.base_iri.clone(),
			vocabulary: self.vocabulary.clone(),
			default_language: self.default_language.clone(),
			default_base_direction: self.default_base_direction,
			previous_context: self.previous_context.clone(),
			definitions: Arc::clone(&self.definitions),
			inverse: OnceCell::default(),
		}
	}
}

impl<T: PartialEq, B: PartialEq> PartialEq for Context<T, B> {
	fn eq(&self, other: &Self) -> bool {
		self.original_base_url == other.original_base_url
			&& self.base_iri == other.base_iri
			&& self.vocabulary == other.vocabulary
			&& self.default_language == other.default_language
			&& self.default_base_direction == other.default_base_direction
			&& self.previous_context == other.previous_context
	}
}
