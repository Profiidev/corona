//! D-Bus values to JSON and back. JSON has no types of its own, so the way in
//! follows a signature, from the caller or the introspection data.

use anyhow::{Context as _, Result, bail};
use serde_json::{Map, Value as Json};
use zbus::{
  Message,
  zvariant::{Array, Dict, ObjectPath, Signature, Structure, StructureBuilder, Value},
};

pub fn to_json(value: &Value) -> Result<Json> {
  Ok(match value {
    Value::U8(n) => (*n).into(),
    Value::Bool(b) => (*b).into(),
    Value::I16(n) => (*n).into(),
    Value::U16(n) => (*n).into(),
    Value::I32(n) => (*n).into(),
    Value::U32(n) => (*n).into(),
    Value::I64(n) => (*n).into(),
    Value::U64(n) => (*n).into(),
    Value::F64(n) => serde_json::Number::from_f64(*n).map_or(Json::Null, Json::Number),
    Value::Str(s) => s.as_str().into(),
    Value::Signature(s) => s.to_string().into(),
    Value::ObjectPath(p) => p.as_str().into(),
    Value::Value(v) => to_json(v)?,
    Value::Array(a) => a.iter().map(to_json).collect::<Result<_>>()?,
    Value::Dict(d) => {
      let Signature::Dict { key, .. } = d.signature() else {
        unreachable!("a dict has a dict signature")
      };
      if is_string(key) {
        let mut map = Map::new();
        for (k, v) in d.iter() {
          let Json::String(k) = to_json(k)? else {
            unreachable!("string keys convert to strings")
          };
          map.insert(k, to_json(v)?);
        }
        Json::Object(map)
      } else {
        d.iter()
          .map(|(k, v)| Ok(Json::Array(vec![to_json(k)?, to_json(v)?])))
          .collect::<Result<_>>()?
      }
    }
    Value::Structure(s) => s.fields().iter().map(to_json).collect::<Result<_>>()?,
    _ => bail!(
      "{} values cannot be passed to a plugin",
      value.value_signature()
    ),
  })
}

/// Every argument of a message.
// ponytail: a body that is one struct reads as its fields, zbus parses `(si)` and `si` alike
pub fn body_to_json(message: &Message) -> Result<Vec<Json>> {
  let body = message.body();
  if body.signature() == &Signature::Unit {
    return Ok(Vec::new());
  }
  let fields: Structure = body.deserialize()?;
  fields.fields().iter().map(to_json).collect()
}

/// The body of a call with `args`, whose types `signature` lists without the
/// outer parens. `None` when there are none.
pub fn body(args: &[Json], signature: &str) -> Result<Option<Structure<'static>>> {
  if args.is_empty() && signature.is_empty() {
    return Ok(None);
  }
  let parsed = parse(&format!("({signature})"))?;
  let Signature::Structure(fields) = &parsed else {
    unreachable!("a parenthesized signature is a structure")
  };
  let fields: Vec<_> = fields.iter().collect();
  if fields.len() != args.len() {
    bail!(
      "signature `{signature}` takes {} arguments, got {}",
      fields.len(),
      args.len()
    );
  }
  let mut builder = StructureBuilder::new();
  for (arg, signature) in args.iter().zip(fields) {
    builder = builder.append_field(from_json(arg, signature)?);
  }
  Ok(Some(builder.build()?))
}

/// The signature error is no `std::error::Error`.
pub fn parse(signature: &str) -> Result<Signature> {
  Signature::try_from(signature)
    .ok()
    .with_context(|| format!("invalid signature `{signature}`"))
}

fn is_string(signature: &Signature) -> bool {
  matches!(
    signature,
    Signature::Str | Signature::ObjectPath | Signature::Signature
  )
}

fn int<T: TryFrom<i64> + TryFrom<u64>>(json: &Json) -> Result<T> {
  let n = match (json.as_i64(), json.as_u64()) {
    (Some(n), _) => T::try_from(n).ok(),
    (_, Some(n)) => T::try_from(n).ok(),
    _ => bail!("expected an integer, got {json}"),
  };
  n.with_context(|| format!("{json} is out of range"))
}

fn str(json: &Json) -> Result<String> {
  json
    .as_str()
    .map(str::to_owned)
    .with_context(|| format!("expected a string, got {json}"))
}

pub fn from_json(json: &Json, signature: &Signature) -> Result<Value<'static>> {
  Ok(match signature {
    Signature::U8 => Value::U8(int(json)?),
    Signature::Bool => Value::Bool(
      json
        .as_bool()
        .with_context(|| format!("expected a boolean, got {json}"))?,
    ),
    Signature::I16 => Value::I16(int(json)?),
    Signature::U16 => Value::U16(int(json)?),
    Signature::I32 => Value::I32(int(json)?),
    Signature::U32 => Value::U32(int(json)?),
    Signature::I64 => Value::I64(int(json)?),
    Signature::U64 => Value::U64(int(json)?),
    Signature::F64 => Value::F64(
      json
        .as_f64()
        .with_context(|| format!("expected a number, got {json}"))?,
    ),
    Signature::Str => Value::from(str(json)?),
    Signature::ObjectPath => Value::ObjectPath(ObjectPath::try_from(str(json)?)?),
    Signature::Signature => Value::Signature(parse(&str(json)?)?),
    Signature::Variant => Value::Value(Box::new(infer(json)?)),
    Signature::Array(child) => {
      let items = json
        .as_array()
        .with_context(|| format!("expected an array, got {json}"))?;
      let mut array = Array::new(child);
      for item in items {
        array.append(from_json(item, child)?)?;
      }
      Value::Array(array)
    }
    Signature::Dict { key, value } => {
      let mut dict = Dict::new(key, value);
      match json {
        Json::Object(map) => {
          for (k, v) in map {
            // non-string keys are written as JSON, e.g. `"1"` for a `u`
            let k = if is_string(key) {
              Json::String(k.clone())
            } else {
              serde_json::from_str(k).with_context(|| format!("invalid key `{k}`"))?
            };
            dict.append(from_json(&k, key)?, from_json(v, value)?)?;
          }
        }
        Json::Array(pairs) => {
          for pair in pairs {
            let Some([k, v]) = pair.as_array().map(Vec::as_slice) else {
              bail!("expected a [key, value] pair, got {pair}");
            };
            dict.append(from_json(k, key)?, from_json(v, value)?)?;
          }
        }
        _ => bail!("expected an object or [key, value] pairs, got {json}"),
      }
      Value::Dict(dict)
    }
    Signature::Structure(fields) => {
      let items = json
        .as_array()
        .filter(|items| items.len() == fields.iter().count())
        .with_context(|| format!("expected {} fields, got {json}", fields.iter().count()))?;
      let mut builder = StructureBuilder::new();
      for (item, field) in items.iter().zip(fields.iter()) {
        builder = builder.append_field(from_json(item, field)?);
      }
      Value::Structure(builder.build()?)
    }
    _ => bail!("cannot pass `{signature}` from a plugin"),
  })
}

/// The value inside a `v`, typed by what the JSON looks like.
fn infer(json: &Json) -> Result<Value<'static>> {
  Ok(match json {
    Json::Null => bail!("null has no D-Bus type"),
    Json::Bool(b) => Value::Bool(*b),
    Json::Number(n) => {
      if let Some(n) = n.as_i64() {
        i32::try_from(n).map_or(Value::I64(n), Value::I32)
      } else if let Some(n) = n.as_u64() {
        Value::U64(n)
      } else {
        Value::F64(n.as_f64().unwrap_or_default())
      }
    }
    Json::String(s) => Value::from(s.clone()),
    Json::Array(items) => {
      let mut array = Array::new(&Signature::Variant);
      for item in items {
        array.append(Value::Value(Box::new(infer(item)?)))?;
      }
      Value::Array(array)
    }
    Json::Object(map) => {
      let mut dict = Dict::new(&Signature::Str, &Signature::Variant);
      for (k, v) in map {
        dict.append(Value::from(k.clone()), Value::Value(Box::new(infer(v)?)))?;
      }
      Value::Dict(dict)
    }
  })
}

#[cfg(test)]
mod tests {
  use serde_json::json;

  use super::*;

  fn sig(s: &str) -> Signature {
    Signature::try_from(s).unwrap()
  }

  /// `json` as `signature` and back.
  fn round_trip(json: Json, signature: &str) -> Json {
    to_json(&from_json(&json, &sig(signature)).unwrap()).unwrap()
  }

  #[test]
  fn basic_types_round_trip() {
    for (json, signature) in [
      (json!(255), "y"),
      (json!(true), "b"),
      (json!(-3), "n"),
      (json!(3), "q"),
      (json!(-70000), "i"),
      (json!(70000), "u"),
      (json!(i64::MIN), "x"),
      (json!(u64::MAX), "t"),
      (json!(2.5), "d"),
      (json!("hi"), "s"),
      (json!("/a/b"), "o"),
      (json!("a{sv}"), "g"),
      (json!([1, 2]), "ay"),
      (json!([["a", 1], ["b", 2]]), "a(si)"),
      (json!({ "a": [1], "b": [] }), "a{sai}"),
    ] {
      assert_eq!(round_trip(json.clone(), signature), json, "{signature}");
    }
    // integral numbers are fine for a double
    assert_eq!(round_trip(json!(2), "d"), json!(2.0));
  }

  #[test]
  fn ranges_and_types_are_checked() {
    for (json, signature) in [
      (json!(256), "y"),
      (json!(-1), "u"),
      (json!(1.5), "i"),
      (json!(-1), "t"),
      (json!(70000), "n"),
      (json!("1"), "i"),
      (json!(1), "s"),
      (json!("not a path"), "o"),
      (json!("("), "g"),
      (json!([1, "a"]), "ai"),
      (json!(["a"]), "(si)"),
      (json!(null), "v"),
      (json!(1), "h"),
      (json!(1), "a{sv}"),
      (json!([["a"]]), "a{si}"),
    ] {
      assert!(
        from_json(&json, &sig(signature)).is_err(),
        "{json} {signature}"
      );
    }
  }

  #[test]
  fn variants_are_inferred() {
    let check = |json: Json, inner: &str| {
      let value = from_json(&json, &Signature::Variant).unwrap();
      let Value::Value(inner_value) = &value else {
        panic!("a variant");
      };
      assert_eq!(inner_value.value_signature().to_string(), inner, "{json}");
      // variants are unwrapped on the way back
      assert_eq!(to_json(&value).unwrap(), json);
    };
    check(json!(true), "b");
    check(json!(3), "i");
    check(json!(1i64 << 40), "x");
    check(json!(u64::MAX), "t");
    check(json!(2.5), "d");
    check(json!("s"), "s");
    check(json!([1, "a"]), "av");
    check(json!({ "a": { "b": 1 } }), "a{sv}");
  }

  #[test]
  fn dicts_with_other_keys_are_pairs() {
    // from an object, the keys parse as the key type
    let value = from_json(&json!({ "1": "a" }), &sig("a{us}")).unwrap();
    assert_eq!(to_json(&value).unwrap(), json!([[1, "a"]]));
    assert!(from_json(&json!({ "x": "a" }), &sig("a{us}")).is_err());
    let value = from_json(&json!([[2, true]]), &sig("a{ub}")).unwrap();
    assert_eq!(to_json(&value).unwrap(), json!([[2, true]]));
    let value = from_json(&json!({ "/a": 1 }), &sig("a{oi}")).unwrap();
    assert_eq!(to_json(&value).unwrap(), json!({ "/a": 1 }));
  }

  #[test]
  fn non_finite_doubles_are_null() {
    assert_eq!(to_json(&Value::F64(f64::NAN)).unwrap(), Json::Null);
  }

  #[test]
  fn bodies_follow_the_signature() {
    assert!(body(&[], "").unwrap().is_none());
    let body = body(&[json!("a"), json!([1, 2])], "say").unwrap().unwrap();
    assert_eq!(body.signature().to_string(), "(say)");
    // one struct argument
    let one = super::body(&[json!(["a", 1])], "(si)").unwrap().unwrap();
    assert_eq!(one.fields().len(), 1);

    assert!(super::body(&[json!("a")], "ss").is_err());
    assert!(super::body(&[json!("a")], "").is_err());
  }
}
