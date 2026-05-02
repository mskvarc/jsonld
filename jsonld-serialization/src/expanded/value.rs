use jsonld_core::{Direction, LangString, Value, object::Literal};
use ld_core::RdfLiteral;
use rdf_rs::{LiteralType, vocabulary::IriVocabularyMut};
use xsd_rs::XSD_STRING;

pub fn literal_to_value<V: IriVocabularyMut>(vocabulary: &mut V, lit: RdfLiteral) -> Value<V::Iri> {
    match lit {
        RdfLiteral::Any(s, ty) => match ty {
            LiteralType::Any(datatype) => {
                let iri = datatype.into_iri();
                if iri.as_ref() == XSD_STRING {
                    Value::Literal(Literal::String(s.into()), None)
                } else {
                    let id = vocabulary.insert_owned(iri);
                    Value::Literal(Literal::String(s.into()), Some(id))
                }
            }
            LiteralType::LangString(language) => Value::LangString(LangString::new(s.into(), Some(language.into()), None).unwrap()),
            LiteralType::DirLangString { tag, direction } => {
                let dir = match direction {
                    rdf_rs::Direction::Ltr => Direction::Ltr,
                    rdf_rs::Direction::Rtl => Direction::Rtl,
                };
                Value::LangString(LangString::new(s.into(), Some(tag.into()), Some(dir)).unwrap())
            }
        },
        RdfLiteral::Xsd(xsd) => xsd_to_value(vocabulary, xsd),
        RdfLiteral::Json(json) => Value::Json(json),
    }
}

fn xsd_to_value<V: IriVocabularyMut>(vocabulary: &mut V, value: xsd_rs::Value) -> Value<V::Iri> {
    let ty = value.datatype();
    let number = match value {
        xsd_rs::Value::Boolean(b) => return Value::Literal(Literal::Boolean(b.into()), None),
        xsd_rs::Value::String(s) => return Value::Literal(Literal::String(s.into()), None),
        xsd_rs::Value::Decimal(v) => v.to_string(),
        xsd_rs::Value::Integer(v) => v.to_string(),
        xsd_rs::Value::NonPositiveInteger(v) => v.to_string(),
        xsd_rs::Value::NegativeInteger(v) => v.to_string(),
        xsd_rs::Value::Long(v) => v.to_string(),
        xsd_rs::Value::Int(v) => v.to_string(),
        xsd_rs::Value::Short(v) => v.to_string(),
        xsd_rs::Value::Byte(v) => v.to_string(),
        xsd_rs::Value::NonNegativeInteger(v) => v.to_string(),
        xsd_rs::Value::UnsignedLong(v) => v.to_string(),
        xsd_rs::Value::UnsignedInt(v) => v.to_string(),
        xsd_rs::Value::UnsignedShort(v) => v.to_string(),
        xsd_rs::Value::UnsignedByte(v) => v.to_string(),
        xsd_rs::Value::PositiveInteger(v) => v.to_string(),
        other => {
            let ty = vocabulary.insert(ty.iri());
            return Value::Literal(Literal::String(other.to_string().into()), Some(ty));
        }
    };

    match jstrict::Number::new(&number) {
        Ok(_) => {
            let n = unsafe { jstrict::NumberBuf::new_unchecked(number.into_bytes().into()) };
            Value::Literal(Literal::Number(n), None)
        }
        Err(_) => {
            let ty = vocabulary.insert(ty.iri());
            Value::Literal(Literal::String(number.into()), Some(ty))
        }
    }
}
