#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unreachable)]
use iri_rs::iri;
use jsonld::{JsonLdProcessor, RemoteDocument, syntax::Parse};
use tokio::runtime::Builder as RuntimeBuilder;

async fn custom_01() {
    let mut loader = jsonld::FsLoader::new();

    loader.mount(iri!("https://www.w3.org/").into(), "tests/custom/extern/www.w3.org/");
    loader.mount(iri!("https://w3id.org/").into(), "tests/custom/extern/w3id.org/");

    let input = std::fs::read_to_string("tests/custom/t01-in.jsonld").unwrap();
    let (json, _) = jsonld::syntax::Value::parse_str(&input).unwrap();
    let doc = RemoteDocument::new(None, None, json);

    let mut generator = rdfx::generator::Blank::new_with_prefix("b".to_string()).unwrap();

    eprintln!("available stack: {:?}", stacker::remaining_stack());
    doc.to_rdf(&mut generator, &loader).await.unwrap();
}

// `t01-in.jsonld` nests deeply enough that `to_rdf` needs more stack than a
// thread gets by default, so the test runs on a thread with an enlarged one.
// A 512 KiB stack is known to overflow on this input; the default stack is
// borderline and varies by platform.
#[test]
fn custom_01_high_memory() {
    let child = std::thread::Builder::new()
        .stack_size(3 * 512 * 1024)
        .spawn(|| RuntimeBuilder::new_current_thread().build().unwrap().block_on(custom_01()))
        .unwrap();

    child.join().unwrap()
}
