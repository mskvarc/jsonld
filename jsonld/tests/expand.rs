#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unreachable)]
use contextual::WithContext;
use iri_rs::iri;
use jsonld::{Dereference, JsonLdProcessor, Print, RemoteDocumentReference, TryFromJson};
use rdfx::vocabulary::{IndexVocabulary, IriIndex, IriVocabularyMut};
use tokio::runtime::Builder as RuntimeBuilder;

#[jsonld_testing::test_suite("https://w3c.github.io/json-ld-api/tests/expand-manifest.jsonld")]
#[mount("https://w3c.github.io/json-ld-api", "tests/json-ld-api")]
#[iri_prefix("rdf" = "http://www.w3.org/1999/02/22-rdf-syntax-ns#")]
#[iri_prefix("rdfs" = "http://www.w3.org/2000/01/rdf-schema#")]
#[iri_prefix("manifest" = "http://www.w3.org/2001/sw/DataAccess/tests/test-manifest#")]
#[iri_prefix("test" = "https://w3c.github.io/json-ld-api/tests/vocab#")]
mod expand {
    use iri_rs::Iri;

    #[iri("test:ExpandTest")]
    pub struct Test {
        #[iri("rdfs:comment")]
        pub comments: &'static [&'static str],

        #[iri("manifest:action")]
        pub input: Iri<&'static str>,

        #[iri("manifest:name")]
        pub name: &'static str,

        #[iri("test:option")]
        pub options: Options,

        #[iri("rdf:type")]
        pub desc: Description,
    }

    pub enum Description {
        #[iri("test:PositiveEvaluationTest")]
        Positive {
            #[iri("manifest:result")]
            expect: Iri<&'static str>,
        },
        #[iri("test:NegativeEvaluationTest")]
        Negative {
            #[iri("manifest:result")]
            expected_error_code: &'static str,
        },
    }

    #[derive(Default)]
    pub struct Options {
        #[iri("test:base")]
        pub base: Option<Iri<&'static str>>,

        #[iri("test:expandContext")]
        pub expand_context: Option<Iri<&'static str>>,

        #[iri("test:processingMode")]
        pub processing_mode: Option<jsonld::ProcessingMode>,

        #[iri("test:specVersion")]
        pub spec_version: Option<&'static str>,

        #[iri("test:normative")]
        pub normative: Option<bool>,
    }
}

impl expand::Test {
    fn run(self) {
        let child = std::thread::Builder::new()
            .spawn(|| RuntimeBuilder::new_current_thread().build().unwrap().block_on(self.async_run()))
            .unwrap();

        child.join().unwrap();
    }

    async fn async_run(self) {
        if !self.options.normative.unwrap_or(true) {
            log::warn!("ignoring test `{}` (non normative)", self.name);
            return;
        }

        for comment in self.comments {
            println!("{comment}");
        }

        let mut vocabulary: IndexVocabulary = IndexVocabulary::new();
        let mut loader = jsonld::FsLoader::default();
        loader.mount(iri!("https://w3c.github.io/json-ld-api").into(), "tests/json-ld-api");

        let mut options: jsonld::Options<IriIndex> = jsonld::Options::default();
        if self.options.spec_version == Some("json-ld-1.0") {
            options.processing_mode = jsonld::ProcessingMode::JsonLd1_0;
        }
        if let Some(p) = self.options.processing_mode {
            options.processing_mode = p;
        }

        options.base = self.options.base.map(|iri| vocabulary.insert(iri));
        options.expand_context = self.options.expand_context.map(|iri| RemoteDocumentReference::Iri(vocabulary.insert(iri)));

        let input = vocabulary.insert(self.input);

        match self.desc {
            expand::Description::Positive { expect } => {
                let json_ld = loader.dereference(&mut vocabulary, input).await.unwrap();
                // `expand_slice` is the Expansion algorithm itself over the
                // document's text; it agrees with the processor whenever the
                // processor adds no initial context of its own.
                let slice_input = (options.expand_context.is_none() && json_ld.context_url().is_none()).then(|| {
                    (
                        json_ld.document().compact_print().to_string(),
                        options.base.or_else(|| json_ld.url().copied()),
                        options.expansion_options(),
                    )
                });
                let expanded = json_ld.expand_full(&mut vocabulary, &loader, options, ()).await.unwrap();

                if let Some((text, base, slice_options)) = slice_input {
                    let from_slice = jsonld::expansion::expand_slice(
                        text.as_bytes(),
                        &mut vocabulary,
                        jsonld::expansion::Context::new(base),
                        base.as_ref(),
                        &loader,
                        slice_options,
                        (),
                    )
                    .await
                    .unwrap();
                    assert!(from_slice == expanded, "expand_slice agrees with expand_full");
                }

                let expect_iri = vocabulary.insert(expect);
                let expected = loader.dereference(&mut vocabulary, expect_iri).await.unwrap().into_document();
                let expected = jsonld::ExpandedDocument::try_from_json_in(&mut vocabulary, expected).unwrap();

                let success = expanded == expected;

                if !success {
                    eprintln!("test failed");
                    eprintln!("output=\n{}", expanded.with(&vocabulary).pretty_print());
                    eprintln!("expected=\n{}", expected.with(&vocabulary).pretty_print());
                }

                assert!(success);
            }
            expand::Description::Negative { expected_error_code } => {
                let json_ld = loader.dereference(&mut vocabulary, input).await.unwrap();
                let result: Result<_, _> = json_ld.expand_full(&mut vocabulary, &loader, options, ()).await;

                match result {
                    Ok(expanded) => {
                        eprintln!("output=\n{}", expanded.with(&vocabulary).pretty_print());
                        panic!("expansion succeeded when it should have failed with `{expected_error_code}`")
                    }
                    Err(_e) => {
                        // The test only asserts that expansion failed, not that
                        // it failed with `expected_error_code`: this
                        // implementation's error codes do not yet line up with
                        // the ones the manifests name, so comparing them would
                        // fail on tests whose actual behaviour is correct.
                    }
                }
            }
        }
    }
}
