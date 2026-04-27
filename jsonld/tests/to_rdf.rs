use contextual::{DisplayWithContext, WithContext};
use iri_rs::iri;
use jsonld::{JsonLdProcessor, Loader, Print, RemoteDocumentReference};
use rdf_rs::{
	GeneralizedQuad, LocalTerm, Term as RdfTerm,
	dataset::{IndexedBTreeDataset, isomorphism::dataset_equivalent},
	impl_resource,
	vocabulary::{
		BlankIdIndex, BlankIdVocabulary, BlankIdVocabularyMut, IndexVocabulary, IriIndex,
		IriVocabulary, IriVocabularyMut, LiteralIndex, LiteralVocabulary, LiteralVocabularyMut,
	},
};

/// Uniform resource type holding any RDF term that can flow through quads.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum IndexTerm {
	Iri(IriIndex),
	Blank(BlankIdIndex),
	Literal(LiteralIndex),
}

impl_resource!(IndexTerm);

impl IndexTerm {
	fn from_local_term(t: LocalTerm, vocabulary: &mut IndexVocabulary) -> Self {
		match t {
			LocalTerm::BlankId(b) => Self::Blank(vocabulary.insert_owned_blank_id(b)),
			LocalTerm::Named(RdfTerm::Iri(iri)) => Self::Iri(vocabulary.insert_owned(iri)),
			LocalTerm::Named(RdfTerm::Literal(lit)) => {
				Self::Literal(vocabulary.insert_owned_literal(lit))
			}
			LocalTerm::Triple(_) => panic!("triple terms are not supported in this test"),
		}
	}

	fn from_id_index(id: jsonld::ValidId<IriIndex, BlankIdIndex>) -> Self {
		match id {
			jsonld::ValidId::Iri(i) => Self::Iri(i),
			jsonld::ValidId::Blank(b) => Self::Blank(b),
		}
	}

	fn from_value(v: jsonld::rdf::Value<IriIndex, BlankIdIndex, LiteralIndex>) -> Self {
		use jsonld::rdf::Value;
		match v {
			Value::Id(id) => Self::from_id_index(id),
			Value::Literal(l) => Self::Literal(l),
		}
	}
}

impl<V> DisplayWithContext<V> for IndexTerm
where
	V: IriVocabulary<Iri = IriIndex>
		+ BlankIdVocabulary<BlankId = BlankIdIndex>
		+ LiteralVocabulary<Literal = LiteralIndex>,
{
	fn fmt_with(&self, vocabulary: &V, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			Self::Iri(i) => write!(f, "<{}>", vocabulary.iri(i).unwrap()),
			Self::Blank(b) => write!(f, "{}", vocabulary.blank_id(b).unwrap()),
			Self::Literal(l) => {
				let lit = vocabulary.literal(l).unwrap();
				write!(f, "{:?}", lit.value)
			}
		}
	}
}

#[jsonld_testing::test_suite("https://w3c.github.io/json-ld-api/tests/toRdf-manifest.jsonld")]
#[mount("https://w3c.github.io/json-ld-api", "tests/json-ld-api")]
#[iri_prefix("rdf" = "http://www.w3.org/1999/02/22-rdf-syntax-ns#")]
#[iri_prefix("rdfs" = "http://www.w3.org/2000/01/rdf-schema#")]
#[iri_prefix("manifest" = "http://www.w3.org/2001/sw/DataAccess/tests/test-manifest#")]
#[iri_prefix("jld" = "https://w3c.github.io/json-ld-api/tests/vocab#")]
#[ignore_test("#te122", see = "https://github.com/w3c/json-ld-api/issues/480")]
#[ignore_test("#tli12", see = "https://github.com/w3c/json-ld-api/issues/533")]
mod to_rdf {
	use iri_rs::Iri;
	use jsonld::rdf::RdfDirection;

	#[iri("jld:ToRDFTest")]
	pub struct Test {
		#[iri("rdfs:comment")]
		pub comments: &'static [&'static str],

		#[iri("manifest:action")]
		pub input: Iri<&'static str>,

		#[iri("manifest:name")]
		pub name: &'static str,

		#[iri("jld:option")]
		pub options: Options,

		#[iri("rdf:type")]
		pub desc: Description,
	}

	pub enum Description {
		#[iri("jld:PositiveEvaluationTest")]
		Positive {
			#[iri("manifest:result")]
			expect: Iri<&'static str>,
		},
		#[iri("jld:NegativeEvaluationTest")]
		Negative {
			#[iri("manifest:result")]
			expected_error_code: &'static str,
		},
		#[iri("jld:PositiveSyntaxTest")]
		PositiveSyntax {
			// ...
		},
	}

	#[derive(Default)]
	pub struct Options {
		#[iri("jld:base")]
		pub base: Option<Iri<&'static str>>,

		#[iri("jld:processingMode")]
		pub processing_mode: Option<jsonld::ProcessingMode>,

		#[iri("jld:specVersion")]
		pub spec_version: Option<&'static str>,

		#[iri("jld:normative")]
		pub normative: Option<bool>,

		#[iri("jld:expandContext")]
		pub expand_context: Option<Iri<&'static str>>,

		#[iri("jld:produceGeneralizedRdf")]
		pub produce_generalized_rdf: bool,

		#[iri("jld:rdfDirection")]
		pub rdf_direction: Option<RdfDirection>,
	}
}

impl to_rdf::Test {
	fn run(self) {
		let child = std::thread::Builder::new()
			.spawn(|| async_std::task::block_on(self.async_run()))
			.unwrap();

		child.join().unwrap()
	}

	async fn async_run(self) {
		if !self.options.normative.unwrap_or(true) {
			log::warn!("ignoring test `{}` (non normative)", self.name);
			return;
		}

		if self.options.spec_version == Some("json-ld-1.0") {
			log::warn!("ignoring test `{}` (unsupported spec version)", self.name);
			return;
		}

		for comment in self.comments {
			println!("{}", comment)
		}

		let mut vocabulary: IndexVocabulary = IndexVocabulary::new();
		let mut loader = jsonld::FsLoader::default();
		loader.mount(
			iri!("https://w3c.github.io/json-ld-api").into(),
			"tests/json-ld-api",
		);

		let mut options: jsonld::Options<IriIndex> = jsonld::Options::default();
		if let Some(p) = self.options.processing_mode {
			options.processing_mode = p
		}

		options.base = self.options.base.map(|iri| vocabulary.insert(iri));
		options.expand_context = self
			.options
			.expand_context
			.map(|iri| RemoteDocumentReference::Iri(vocabulary.insert(iri)));
		options.rdf_direction = self.options.rdf_direction;
		options.produce_generalized_rdf = self.options.produce_generalized_rdf;

		let input = vocabulary.insert(self.input);

		match self.desc {
			to_rdf::Description::Positive { expect } => {
				let json_ld = loader.load_with(&mut vocabulary, input).await.unwrap();

				let mut generator =
					rdf_rs::generator::Blank::new_with_prefix("b".to_string()).unwrap();
				let mut to_rdf = json_ld
					.to_rdf_full(&mut vocabulary, &mut generator, &loader, options, ())
					.await
					.unwrap();

				let dataset: IndexedBTreeDataset<IndexTerm> = to_rdf
					.quads()
					.cloned()
					.map(|rdf_rs::GeneralizedQuad(s, p, o, g)| {
						rdf_rs::Quad(
							IndexTerm::from_id_index(s),
							IndexTerm::from_id_index(p),
							IndexTerm::from_value(o),
							g.map(IndexTerm::from_id_index),
						)
					})
					.collect();

				let expected_content =
					std::fs::read_to_string(loader.filepath(expect).unwrap()).unwrap();
				let (parsed, _code_map) =
					n_quads::grdf_document_from_str(&expected_content).unwrap();
				let expected_dataset: IndexedBTreeDataset<IndexTerm> = parsed
					.into_iter()
					.map(|GeneralizedQuad(s, p, o, g)| {
						rdf_rs::Quad(
							IndexTerm::from_local_term(s, &mut vocabulary),
							IndexTerm::from_local_term(p, &mut vocabulary),
							IndexTerm::from_local_term(o, &mut vocabulary),
							g.map(|t| IndexTerm::from_local_term(t, &mut vocabulary)),
						)
					})
					.collect();

				let success = dataset_equivalent(&dataset, &expected_dataset);

				if !success {
					eprintln!("test failed");
					eprintln!("output=");
					for q in &dataset {
						eprintln!(
							"{} {} {}{}",
							q.0.with(&vocabulary),
							q.1.with(&vocabulary),
							q.2.with(&vocabulary),
							match &q.3 {
								Some(g) => format!(" {}", g.with(&vocabulary)),
								None => String::new(),
							}
						);
					}

					eprintln!("expected=");
					for q in &expected_dataset {
						eprintln!(
							"{} {} {}{}",
							q.0.with(&vocabulary),
							q.1.with(&vocabulary),
							q.2.with(&vocabulary),
							match &q.3 {
								Some(g) => format!(" {}", g.with(&vocabulary)),
								None => String::new(),
							}
						);
					}
				}

				assert!(success)
			}
			to_rdf::Description::Negative {
				expected_error_code,
			} => {
				let json_ld = loader.load_with(&mut vocabulary, input).await.unwrap();
				let result: Result<_, _> = json_ld
					.expand_full(&mut vocabulary, &loader, options, ())
					.await;

				match result {
					Ok(expanded) => {
						eprintln!("output=\n{}", expanded.with(&vocabulary).pretty_print());
						panic!(
							"expansion succeeded when it should have failed with `{}`",
							expected_error_code
						)
					}
					Err(_e) => {
						// TODO improve error codes.
						// assert_eq!(e.code().as_str(), expected_error_code)
					}
				}
			}
			to_rdf::Description::PositiveSyntax {} => {
				// ...
			}
		}
	}
}
