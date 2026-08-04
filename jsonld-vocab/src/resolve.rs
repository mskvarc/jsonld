use crate::model::{
    ContextModel,
    DefinitionSource,
    InputContextFile,
    InputContexts,
    ParsedDocument,
    PrefixDefinition,
    ResolvedModel,
    ResolvedPrefix,
    ResolvedTerm,
    TermDefinition,
    TermTarget,
};
use heck::ToShoutySnakeCase;
use proc_macro2::Span;
use serde_json::{Map, Value};
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    path::Path,
};

/// Read, merge, and resolve the input contexts into a canonical vocabulary model.
pub fn load_contexts(inputs: &InputContexts) -> syn::Result<ResolvedModel> {
    let documents = inputs.iter().map(read_document).collect::<syn::Result<Vec<_>>>()?;

    let mut model = ContextModel::default();
    for document in &documents {
        collect_context_object(document.top_level_context()?, &document.input, &mut model)?;
    }

    let prefixes = resolve_prefixes(model.prefixes)?;
    let (class_terms, property_terms) = partition_terms(&model.terms, &prefixes)?;
    let include_paths = documents
        .iter()
        .map(|document| path_to_string(&document.input.resolved_path, document.input.span))
        .collect::<syn::Result<Vec<_>>>()?;

    Ok(ResolvedModel {
        prefixes,
        class_terms,
        property_terms,
        include_paths,
    })
}

fn read_document(input: &InputContextFile) -> syn::Result<ParsedDocument> {
    let input = InputContextFile {
        resolved_path: std::fs::canonicalize(&input.resolved_path).unwrap_or_else(|_| input.resolved_path.clone()),
        ..input.clone()
    };
    let json = std::fs::read_to_string(&input.resolved_path)
        .map_err(|error| syn::Error::new(input.span, format!("cannot read context file {}: {error}", input.resolved_path.display())))?;
    let root = serde_json::from_str(&json)
        .map_err(|error| syn::Error::new(input.span, format!("invalid JSON in context file {}: {error}", input.resolved_path.display())))?;

    Ok(ParsedDocument { input, root })
}

fn collect_context_object(context: &Map<String, Value>, file: &InputContextFile, model: &mut ContextModel) -> syn::Result<()> {
    for (compact, value) in context {
        if compact.starts_with('@') {
            continue;
        }

        if let Some(prefix_iri) = value.as_str().filter(|candidate| is_prefix_namespace(candidate)) {
            insert_prefix(model, compact, prefix_iri, file)?;
            continue;
        }

        // Expanded term definitions with an explicit `"@prefix": true` are
        // prefixes regardless of how their namespace IRI ends.
        if let Value::Object(object) = value
            && object.get("@prefix").and_then(Value::as_bool) == Some(true)
            && let Some(prefix_iri) = object.get("@id").and_then(Value::as_str)
        {
            insert_prefix(model, compact, prefix_iri, file)?;
            collect_nested_contexts(value, file, model)?;
            continue;
        }

        if let Some(target) = extract_term_target(compact, value, file.span)? {
            insert_term(model, compact, target, file)?;
        }

        collect_nested_contexts(value, file, model)?;
    }

    Ok(())
}

fn collect_nested_contexts(value: &Value, file: &InputContextFile, model: &mut ContextModel) -> syn::Result<()> {
    match value {
        Value::Object(object) => {
            if let Some(nested) = object.get("@context") {
                collect_context_value(nested, file, model)?;
            }
        }
        Value::Array(items) => {
            for item in items {
                collect_nested_contexts(item, file, model)?;
            }
        }
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => {}
    }

    Ok(())
}

fn collect_context_value(value: &Value, file: &InputContextFile, model: &mut ContextModel) -> syn::Result<()> {
    match value {
        Value::Object(object) => collect_context_object(object, file, model),
        Value::Array(items) => {
            for item in items {
                collect_context_value(item, file, model)?;
            }
            Ok(())
        }
        Value::String(_url) => {
            // Remote @context URLs are silently skipped: their terms simply
            // do not get materialised into generated constants.
            Ok(())
        }
        Value::Null | Value::Bool(_) | Value::Number(_) => Err(syn::Error::new(
            file.span,
            format!(
                "unsupported nested @context value in {}: expected object, array, or string URL",
                file.resolved_path.display()
            ),
        )),
    }
}

fn extract_term_target(compact: &str, value: &Value, span: Span) -> syn::Result<Option<TermTarget>> {
    match value {
        Value::String(raw) => parse_term_target(compact, raw, span),
        Value::Object(object) => {
            let Some(id) = object.get("@id").and_then(Value::as_str) else {
                return Ok(None);
            };
            parse_term_target(compact, id, span)
        }
        Value::Array(_) | Value::Null | Value::Bool(_) | Value::Number(_) => Ok(None),
    }
}

fn parse_term_target(compact: &str, raw: &str, span: Span) -> syn::Result<Option<TermTarget>> {
    if compact == "id" && raw == "@id" {
        return Ok(None);
    }
    if compact == "type" && raw == "@type" {
        return Ok(None);
    }
    if raw.starts_with('@') {
        return Ok(None);
    }
    if raw.starts_with("http://") || raw.starts_with("https://") || raw.starts_with("urn:") {
        return Ok(Some(TermTarget::Absolute(raw.to_string())));
    }
    if let Some((prefix, suffix)) = split_prefixed(raw) {
        return Ok(Some(TermTarget::Prefixed {
            prefix: prefix.to_string(),
            suffix: suffix.to_string(),
        }));
    }
    if raw.is_empty() {
        return Err(syn::Error::new(span, format!("term {compact} has an empty @id value")));
    }

    Ok(Some(TermTarget::LocalReference(raw.to_string())))
}

fn insert_prefix(model: &mut ContextModel, compact: &str, expanded: &str, file: &InputContextFile) -> syn::Result<()> {
    let definition = PrefixDefinition {
        compact: compact.to_string(),
        expanded: expanded.to_string(),
        source: source(file),
    };

    match model.prefixes.get(compact) {
        Some(previous) if previous.expanded == definition.expanded => Ok(()),
        Some(previous) => Err(syn::Error::new(
            file.span,
            format!(
                "prefix {compact} in {} conflicts with {} from {}",
                file.resolved_path.display(),
                previous.expanded,
                previous.source.path.display()
            ),
        )),
        None => {
            model.prefixes.insert(compact.to_string(), definition);
            Ok(())
        }
    }
}

fn insert_term(model: &mut ContextModel, compact: &str, target: TermTarget, file: &InputContextFile) -> syn::Result<()> {
    let definition = TermDefinition {
        compact: compact.to_string(),
        target,
        source: source(file),
    };

    match model.terms.get(compact) {
        Some(previous) if term_targets_equal(&previous.target, &definition.target) => Ok(()),
        Some(previous) => Err(syn::Error::new(
            file.span,
            format!(
                "term {compact} in {} conflicts with previous definition from {}",
                file.resolved_path.display(),
                previous.source.path.display()
            ),
        )),
        None => {
            model.terms.insert(compact.to_string(), definition);
            Ok(())
        }
    }
}

fn resolve_prefixes(prefixes: BTreeMap<String, PrefixDefinition>) -> syn::Result<Vec<ResolvedPrefix>> {
    let mut resolved = Vec::with_capacity(prefixes.len());
    let mut names = HashMap::<String, String>::new();

    for (_, prefix) in prefixes {
        let const_name = const_name(&prefix.compact, prefix.source.span)?;
        if let Some(previous) = names.insert(const_name.clone(), prefix.compact.clone()) {
            return Err(syn::Error::new(
                prefix.source.span,
                format!("prefix naming collision: {previous} and {} both map to {const_name}", prefix.compact),
            ));
        }
        resolved.push(ResolvedPrefix {
            compact: prefix.compact,
            expanded: prefix.expanded,
            const_name,
        });
    }

    Ok(resolved)
}

/// Turn a compact name into a valid `SHOUTY_SNAKE_CASE` constant name.
///
/// Digit-leading names (schema.org defines `3DModel`) get a `_` prefix;
/// anything that still fails to parse as a Rust identifier becomes a spanned
/// error instead of an unspanned `Ident::new` panic in codegen.
fn const_name(compact: &str, span: Span) -> syn::Result<String> {
    let mut name = compact.to_shouty_snake_case();
    if name.chars().next().is_some_and(|c| c.is_ascii_digit()) {
        name.insert(0, '_');
    }
    if name.is_empty() || syn::parse_str::<syn::Ident>(&name).is_err() {
        return Err(syn::Error::new(
            span,
            format!("term `{compact}` does not map to a valid Rust constant name (got `{name}`)"),
        ));
    }
    Ok(name)
}

fn partition_terms(terms: &BTreeMap<String, TermDefinition>, prefixes: &[ResolvedPrefix]) -> syn::Result<(Vec<ResolvedTerm>, Vec<ResolvedTerm>)> {
    let prefix_map = prefixes
        .iter()
        .map(|prefix| (prefix.compact.as_str(), prefix.expanded.as_str()))
        .collect::<HashMap<_, _>>();

    let mut cache = HashMap::<String, String>::new();
    let mut classes = Vec::new();
    let mut properties = Vec::new();
    let mut class_names = HashMap::<String, String>::new();
    let mut property_names = HashMap::<String, String>::new();

    for (compact, definition) in terms {
        let span = definition.source.span;
        let expanded = resolve_term(compact, terms, &prefix_map, &mut cache, &mut Vec::new(), &mut HashSet::new(), span)?;
        let const_name = const_name(compact, span)?;

        if starts_with_uppercase(compact) {
            if let Some(previous) = class_names.insert(const_name.clone(), compact.clone()) {
                return Err(syn::Error::new(
                    span,
                    format!("class term naming collision: {previous} and {compact} both map to {const_name}"),
                ));
            }
            classes.push(ResolvedTerm {
                compact: compact.clone(),
                expanded,
                const_name,
            });
        } else {
            if let Some(previous) = property_names.insert(const_name.clone(), compact.clone()) {
                return Err(syn::Error::new(
                    span,
                    format!("property term naming collision: {previous} and {compact} both map to {const_name}"),
                ));
            }
            properties.push(ResolvedTerm {
                compact: compact.clone(),
                expanded,
                const_name,
            });
        }
    }

    Ok((classes, properties))
}

#[allow(clippy::too_many_arguments)]
fn resolve_term(
    compact: &str,
    terms: &BTreeMap<String, TermDefinition>,
    prefixes: &HashMap<&str, &str>,
    cache: &mut HashMap<String, String>,
    stack: &mut Vec<String>,
    visiting: &mut HashSet<String>,
    referrer_span: Span,
) -> syn::Result<String> {
    if let Some(expanded) = cache.get(compact) {
        return Ok(expanded.clone());
    }
    let span = terms.get(compact).map_or(referrer_span, |definition| definition.source.span);
    if !visiting.insert(compact.to_string()) {
        stack.push(compact.to_string());
        return Err(syn::Error::new(
            span,
            format!("cycle detected while resolving local term references: {}", stack.join(" -> ")),
        ));
    }
    stack.push(compact.to_string());

    let definition = terms
        .get(compact)
        .ok_or_else(|| syn::Error::new(referrer_span, format!("unresolved local term reference: {compact}")))?;

    let expanded = match &definition.target {
        TermTarget::Absolute(iri) => iri.clone(),
        TermTarget::Prefixed { prefix, suffix } => {
            let namespace = prefixes.get(prefix.as_str()).ok_or_else(|| {
                syn::Error::new(
                    span,
                    format!(
                        "unknown prefix {prefix} while resolving term {} from {}",
                        definition.compact,
                        definition.source.path.display()
                    ),
                )
            })?;
            format!("{namespace}{suffix}")
        }
        TermTarget::LocalReference(target) => {
            resolve_term(target, terms, prefixes, cache, stack, visiting, span).map_err(|inner| {
                // Keep the inner failure (cycle, unknown prefix, ...) visible
                // instead of discarding it.
                let mut error = syn::Error::new(
                    span,
                    format!(
                        "unable to resolve local term reference {target} while resolving {} from {}",
                        definition.compact,
                        definition.source.path.display()
                    ),
                );
                error.combine(inner);
                error
            })?
        }
    };

    stack.pop();
    visiting.remove(compact);
    cache.insert(compact.to_string(), expanded.clone());
    Ok(expanded)
}

fn split_prefixed(value: &str) -> Option<(&str, &str)> {
    let (prefix, suffix) = value.split_once(':')?;
    if prefix.is_empty() || suffix.is_empty() {
        return None;
    }
    if !prefix.chars().all(is_prefix_char) {
        return None;
    }
    Some((prefix, suffix))
}

fn is_prefix_char(character: char) -> bool {
    character.is_ascii_alphanumeric() || matches!(character, '+' | '-' | '.' | '_')
}

/// A simple string-valued term definition is usable as a prefix when its
/// namespace IRI ends with a gen-delim character (RFC 3986 §2.2), matching
/// the JSON-LD 1.1 prefix rules.
fn is_prefix_namespace(value: &str) -> bool {
    value.chars().next_back().is_some_and(|c| matches!(c, ':' | '/' | '?' | '#' | '[' | ']' | '@'))
}

fn term_targets_equal(left: &TermTarget, right: &TermTarget) -> bool {
    match (left, right) {
        (TermTarget::Absolute(left), TermTarget::Absolute(right)) => left == right,
        (
            TermTarget::Prefixed {
                prefix: left_prefix,
                suffix: left_suffix,
            },
            TermTarget::Prefixed {
                prefix: right_prefix,
                suffix: right_suffix,
            },
        ) => left_prefix == right_prefix && left_suffix == right_suffix,
        (TermTarget::LocalReference(left), TermTarget::LocalReference(right)) => left == right,
        _ => false,
    }
}

fn source(file: &InputContextFile) -> DefinitionSource {
    DefinitionSource {
        path: file.resolved_path.clone(),
        span: file.span,
    }
}

fn starts_with_uppercase(value: &str) -> bool {
    value.chars().next().is_some_and(char::is_uppercase)
}

fn path_to_string(path: &Path, span: proc_macro2::Span) -> syn::Result<String> {
    path.to_str()
        .map(ToString::to_string)
        .ok_or_else(|| syn::Error::new(span, format!("path {} is not valid UTF-8", path.display())))
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn span() -> Span {
        Span::call_site()
    }

    fn file() -> InputContextFile {
        InputContextFile {
            resolved_path: PathBuf::from("test.jsonld"),
            span: span(),
        }
    }

    fn model_from(json: &str) -> syn::Result<ContextModel> {
        let root: Value = serde_json::from_str(json).unwrap();
        let mut model = ContextModel::default();
        collect_context_object(root.as_object().unwrap(), &file(), &mut model)?;
        Ok(model)
    }

    fn resolve(json: &str) -> syn::Result<(Vec<ResolvedTerm>, Vec<ResolvedTerm>)> {
        let model = model_from(json)?;
        let prefixes = resolve_prefixes(model.prefixes)?;
        partition_terms(&model.terms, &prefixes)
    }

    use std::path::PathBuf;

    #[test]
    fn absolute_http_iri_is_detected() {
        assert!(matches!(
            parse_term_target("description", "http://purl.org/dc/terms/description", span()).unwrap(),
            Some(TermTarget::Absolute(value)) if value == "http://purl.org/dc/terms/description"
        ));
    }

    #[test]
    fn absolute_urn_iri_is_detected() {
        assert!(matches!(
            parse_term_target("nullUri", "urn:ngsi-ld:null", span()).unwrap(),
            Some(TermTarget::Absolute(value)) if value == "urn:ngsi-ld:null"
        ));
    }

    #[test]
    fn prefixed_name_is_detected() {
        assert!(matches!(
            parse_term_target("Property", "ngsi-ld:Property", span()).unwrap(),
            Some(TermTarget::Prefixed { prefix, suffix }) if prefix == "ngsi-ld" && suffix == "Property"
        ));
    }

    #[test]
    fn underscored_prefix_name_is_detected() {
        assert!(matches!(
            parse_term_target("Thing", "my_ns:Thing", span()).unwrap(),
            Some(TermTarget::Prefixed { prefix, suffix }) if prefix == "my_ns" && suffix == "Thing"
        ));
    }

    #[test]
    fn compact_names_map_to_stable_constants() {
        assert_eq!("createdAt".to_shouty_snake_case(), "CREATED_AT");
        assert_eq!("GeoProperty".to_shouty_snake_case(), "GEO_PROPERTY");
        assert_eq!("ngsi-ld".to_shouty_snake_case(), "NGSI_LD");
        assert_eq!("jsonldContextRel".to_shouty_snake_case(), "JSONLD_CONTEXT_REL");
    }

    #[test]
    fn digit_leading_term_is_sanitized_not_panicking() {
        assert_eq!(const_name("3DModel", span()).unwrap(), "_3D_MODEL");
    }

    #[test]
    fn empty_const_name_is_a_spanned_error() {
        assert!(const_name("", span()).is_err());
    }

    #[test]
    fn gen_delim_namespaces_are_prefixes() {
        for namespace in ["http://a/", "http://a#", "urn:x:", "http://a?", "http://a@"] {
            assert!(is_prefix_namespace(namespace), "{namespace} should be a prefix namespace");
        }
        assert!(!is_prefix_namespace("http://schema.org"));
    }

    #[test]
    fn explicit_prefix_true_is_honored() {
        let model = model_from(r#"{"schema": {"@id": "http://schema.org", "@prefix": true}}"#).unwrap();
        assert!(model.prefixes.contains_key("schema"));
        assert!(model.terms.is_empty());
    }

    #[test]
    fn unknown_prefix_is_reported() {
        let error = resolve(r#"{"Thing": "myns:Thing"}"#).unwrap_err();
        assert!(error.to_string().contains("unknown prefix myns"), "{error}");
    }

    #[test]
    fn unresolved_local_reference_is_reported() {
        let error = resolve(r#"{"alias": "missingTerm"}"#).unwrap_err();
        let rendered = error.into_iter().map(|e| e.to_string()).collect::<Vec<_>>().join("\n");
        assert!(rendered.contains("unresolved local term reference: missingTerm"), "{rendered}");
    }

    #[test]
    fn cycles_are_reported_with_the_cycle_path() {
        let error = resolve(r#"{"a": "b", "b": "a"}"#).unwrap_err();
        let rendered = error.into_iter().map(|e| e.to_string()).collect::<Vec<_>>().join("\n");
        assert!(rendered.contains("cycle detected while resolving local term references"), "{rendered}");
        assert!(rendered.contains("a -> b -> a"), "{rendered}");
    }
}
