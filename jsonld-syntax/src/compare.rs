use jstrict::Value;

/// JSON-LD comparison.
pub trait Compare {
    fn compare(&self, other: &Self) -> bool;
}

impl Compare for Value {
    fn compare(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Null, Self::Null) => true,
            (Self::Boolean(a), Self::Boolean(b)) => a == b,
            (Self::Number(a), Self::Number(b)) => a == b,
            (Self::String(a), Self::String(b)) => a == b,
            (Self::Array(a), Self::Array(b)) => {
                if a.len() == b.len() {
                    let mut selected = Vec::new();
                    selected.resize(b.len(), false);

                    'next_item: for item in a {
                        for (other, selected) in b.iter().zip(selected.iter_mut()) {
                            if !*selected && item.compare(other) {
                                *selected = true;
                                continue 'next_item;
                            }
                        }

                        return false;
                    }

                    true
                } else {
                    false
                }
            }
            (Self::Object(a), Self::Object(b)) => {
                if a.len() == b.len() {
                    for entry in a {
                        // Duplicate keys mean the document isn't valid JSON-LD;
                        // treat as "not equal".
                        match b.get_unique(&*entry.key) {
                            Ok(Some(value)) => {
                                if !entry.value.compare(value) {
                                    return false;
                                }
                            }
                            Ok(None) | Err(_) => return false,
                        }
                    }

                    true
                } else {
                    false
                }
            }
            _ => false,
        }
    }
}
