# Migrating from `json-ld-expandable` to `jsonld-expandable`

The `jsonld-expandable` workspace member supersedes the standalone
`json-ld-expandable` crate. Mechanical changes are listed below; the
behavior of every legacy attribute is preserved as an alias to the new
spec-aligned form.

## 1. Swap the dependency

```toml
# before
json-ld-expandable = { path = "json-ld-expandable" }

# after
jsonld = { version = "0.22", features = ["expandable-serde_json"] }
jsonld-expandable-core = "0.22"   # required for path resolution
```

Pick `expandable-jstrict` or `expandable-sonic_rs` instead if your call
sites use that backend. Multiple backends can be enabled together.

## 2. Rename the derive

```rust
#[derive(... JsonLdExpandable ...)]   →   #[derive(... Expandable ...)]
```

The new trait/derive is named `Expandable` (not `Expand`) to avoid a
collision with `jsonld_expansion::Expand`, which is already re-exported by
the umbrella crate at `jsonld::Expand`.

## 3. Update use statements

```rust
use json_ld_expandable::JsonLdExpandable;     // old
use jsonld::Expandable;                        // new (umbrella)
// or
use jsonld_expandable::Expandable;             // new (sub-crate, derive)
use jsonld_expandable_core::Expandable;        // new (sub-crate, trait)
```

## 4. Delete the locally-defined trait

Delete the `JsonLdExpandable` / `JsonLdLanguageMap` / `JsonLdTypeValue`
trait modules. Replace any custom `language_map` or `type_value`
implementations with the workspace traits:

```rust
use jsonld::{ExpandableLanguageMap, ExpandableTypeValue};
```

Drop the `#[jsonld(crate = "crate::jsonld")]` attribute or repoint it at
`#[jsonld(crate = "::jsonld_expandable_core")]` (the new default).

## 5. Update call sites

```rust
thing.to_expanded()                            // old
thing.expand::<serde_json::Value>()            // new (turbofish)

let v: serde_json::Value = thing.expand();     // new (inferred)
```

The method name is `expand` (matching the trait/derive `Expandable`) and
the return type is generic over `V: JsonValue`.

## 6. Prefixes

`ngsi:` and `xsd:` shorthands are no longer hardcoded. Either:

- Spell out the full IRI (already the dominant style in andromeda), or
- Add a per-derive prefix table:

  ```rust
  #[derive(Expandable)]
  #[jsonld(
      type = "ngsi:EntityTypeInfo",
      prefix(
          ngsi = "https://uri.etsi.org/ngsi-ld/",
          xsd  = "http://www.w3.org/2001/XMLSchema#",
      )
  )]
  pub struct EntityTypeInfo { ... }
  ```

## 7. Existing attribute spellings keep working

| Legacy spelling                              | Lowers to (canonical)                |
|----------------------------------------------|--------------------------------------|
| `nested`                                     | nested marker (still required)       |
| `vec`                                        | vec marker (still required)          |
| `vocab` / `id_ref`                           | `coerce = "@id"`                     |
| `vocab_vec`                                  | `coerce = "@id"` + `vec`             |
| `typed_value` + `datatype = "D"`             | `coerce = "D"`                       |
| `json_value`                                 | `coerce = "@json"`                   |
| `list`                                       | `container = "list"`                 |
| `language_map`                               | `container = "language"`             |
| `flatten_map`                                | `flatten_map` marker (canonical)     |
| `flatten_object`                             | `flatten` (canonical)                |
| `custom`                                     | `passthrough` (canonical)            |
| `vocab_polymorphic`                          | **errors** — use a concrete enum     |

## Reserved but not yet implemented

- `nest` / `reverse`
- `container = "id" | "type" | "graph"`

These parse as recognised attributes but produce a compile error from the
derive. None are used by andromeda's existing structs.

## `custom` / `passthrough` semantics

The field's `Expandable` impl (manual or derived) produces the final
JSON-LD shape for this property. The derive emits the value verbatim — no
`[{"@value": ...}]` or `[obj]` wrapping. Use this when you have control
over the inner type and want the freedom to emit any JSON-LD form.

Mutually exclusive with `coerce`, `container`, `nested`, `flatten`,
`flatten_map`, and `id`. Requires `property = "..."`.

## `list` + `coerce` combinations

`container = "list"` (legacy `list`) composes with `coerce` and `nested`:

| Spelling                              | Output |
|---------------------------------------|--------|
| `list`                                | `[{"@list": <to_json_value(field)>}]` |
| `list, id_ref` / `list, coerce="@id"` | `[{"@list": [{"@id": <item>}, ...]}]` |
| `list, typed_value, datatype="D"`     | `[{"@list": [{"@value": <item>, "@type": "D"}, ...]}]` |
| `list, json_value`                    | `[{"@list": [{"@value": <item>, "@type": "@json"}, ...]}]` |
| `list, nested, vec`                   | `[{"@list": [<expand(item)>, ...]}]` |
| `list, nested`                        | same as above (single-item) |

Source must be iterable (`Vec`, `IndexMap`, anything with `iter()` returning
items convertible per the per-item shape).

## Foreign-type interop

Several common foreign types come with built-in `ToJsonValue` impls so
fields of those types can participate in expansion without an
orphan-rule-blocked impl in the consumer crate:

- `serde_json::Value` (under feature `expandable-serde_json`):
  recursively translated into the chosen backend.
- `chrono::DateTime<Tz>`, `NaiveDateTime`, `NaiveDate`, `NaiveTime`
  (under feature `expandable-chrono`): rendered as their canonical
  lexical form (`to_rfc3339()` for `DateTime`, ISO formats otherwise).

## `flatten` / `flatten_object` / `flatten_map` semantics

- **`flatten` / `flatten_object`**: the field's type implements `Expandable`;
  its expanded object's properties are merged into the parent. `@id` and
  `@type` from the sub-fragment are dropped. Use `#[jsonld(fragment)]` on the
  inner type so it does not emit a `@type` of its own.
- **`flatten_map`**: the field is a map (`HashMap<K, V>` / `IndexMap<K, V>`);
  each entry's key (`K: AsRef<str>`) becomes a property of the parent and the
  value (`V: Expandable`) is recursively expanded. No property IRI is required
  — the keys *are* the properties.

Both attributes are mutually exclusive with `property`, `coerce`, `container`,
and `nested`.
