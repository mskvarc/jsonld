#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unreachable)]
use iri_rs::iri;
use jsonld::{JsonLdProcessor, RemoteDocument, syntax::Parse};

async fn custom_01() {
    let mut loader = jsonld::FsLoader::new();

    loader.mount(iri!("https://www.w3.org/").into(), "tests/custom/extern/www.w3.org/");
    loader.mount(iri!("https://w3id.org/").into(), "tests/custom/extern/w3id.org/");

    let input = std::fs::read_to_string("tests/custom/t01-in.jsonld").unwrap();
    let (json, _) = jsonld::syntax::Value::parse_str(&input).unwrap();
    let doc = RemoteDocument::new(None, None, json);

    let mut generator = rdf_rs::generator::Blank::new_with_prefix("b".to_string()).unwrap();

    eprintln!("available stack: {:?}", stacker::remaining_stack());
    doc.to_rdf(&mut generator, &loader).await.unwrap();
}

// This may fail depending on the default stack size.
// #[async_std::test]
// async fn custom_01_default_memory() {
// 	custom_01().await
// }

// This will fail because not enough stack memory.
// #[test]
// fn custom_01_low_memory() {
// 	let child = std::thread::Builder::new()
// 		.stack_size(512 * 1024)
// 		.spawn(|| async_std::task::block_on(custom_01()))
// 		.unwrap();

// 	child.join().unwrap()
// }

#[test]
fn custom_01_high_memory() {
    let child = std::thread::Builder::new()
        .stack_size(3 * 512 * 1024)
        .spawn(|| async_std::task::block_on(custom_01()))
        .unwrap();

    child.join().unwrap()
}
